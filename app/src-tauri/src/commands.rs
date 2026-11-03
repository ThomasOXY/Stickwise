use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pioneer_formats::anlz::{
    AnlzFile, CuePoint, HOT_CUE_PALETTE, hot_cue_color, read_cue_points, tag,
};
use pioneer_formats::pdb::file_type;
use tauri::{AppHandle, Emitter, Manager, State};
use usb_tools::Stick;
use usb_tools::compat::{self, Player};
use usb_tools::extract::{self, Duplicates, ExtractOptions, Naming};
use usb_tools::probe::{self, Codec};
use usb_tools::verify::{self, Problem, filesystem_type};

use crate::dto;
use crate::edits::{self, Edits, StoredCue};
use crate::jobs::Jobs;
use crate::sidecar::Sidecar;

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

fn disk_space(path: &Path) -> (u64, u64) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return (0, 0);
    };
    // SAFETY: statfs fills the zeroed struct for a valid path.
    unsafe {
        let mut s: libc::statfs = std::mem::zeroed();
        if libc::statfs(c.as_ptr(), &mut s) != 0 {
            return (0, 0);
        }
        let b = s.f_bsize as u64;
        (s.f_blocks * b, s.f_bavail * b)
    }
}

fn stick_info(path: &Path) -> dto::StickInfo {
    let (total, free) = disk_space(path);
    dto::StickInfo {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        total_bytes: total,
        free_bytes: free,
        filesystem: filesystem_type(path).unwrap_or_default(),
    }
}

/// Mounted volumes that carry a rekordbox device library.
#[tauri::command]
pub fn list_sticks() -> Vec<dto::StickInfo> {
    let Ok(rd) = std::fs::read_dir("/Volumes") else {
        return Vec::new();
    };
    let mut out: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| !p.is_symlink() && p.join("PIONEER/rekordbox/export.pdb").is_file())
        .map(|p| stick_info(&p))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn format_label(t: &pioneer_formats::pdb::Track) -> String {
    let depth = if t.sample_depth > 0 {
        format!(" {}", t.sample_depth)
    } else {
        String::new()
    };
    match t.file_type {
        file_type::MP3 => format!("MP3 {}", t.bitrate),
        file_type::M4A => "M4A".into(),
        file_type::FLAC => format!("FLAC{depth}"),
        file_type::ALAC => format!("ALAC{depth}"),
        file_type::WAV => format!("WAV{depth}"),
        file_type::AIFF => format!("AIFF{depth}"),
        _ => Path::new(&t.file_path)
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default(),
    }
}

#[tauri::command]
pub async fn open_stick(path: String) -> CmdResult<dto::StickData> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let db = &stick.db;
        let name = |v: Option<&str>| v.unwrap_or_default().to_string();
        let tracks = db
            .tracks
            .iter()
            .map(|t| dto::Track {
                id: t.id,
                title: t.title.clone(),
                artist: name(db.artist_name(t.artist_id)),
                album: name(db.album_name(t.album_id)),
                genre: name(db.genre_name(t.genre_id)),
                bpm: t.bpm(),
                key: name(db.key_name(t.key_id)),
                duration: t.duration,
                format: format_label(t),
                file_path: t.file_path.clone(),
                abs_path: stick.audio_path(t).display().to_string(),
                rating: t.rating,
                file_size: t.file_size as u64,
                artwork: artwork_path(&stick, t.artwork_id),
                date_added: t.date_added.clone(),
            })
            .collect();
        let playlists = db
            .playlists
            .iter()
            .map(|p| dto::Playlist {
                id: p.id,
                parent_id: p.parent_id,
                name: p.name.clone(),
                is_folder: p.is_folder,
                sort_order: p.sort_order,
                track_ids: if p.is_folder {
                    Vec::new()
                } else {
                    db.playlist_tracks(p.id)
                },
            })
            .collect();
        let history = db
            .history_playlists
            .iter()
            .map(|h| {
                let mut e: Vec<_> = db
                    .history_entries
                    .iter()
                    .filter(|e| e.playlist_id == h.id)
                    .collect();
                e.sort_by_key(|e| e.entry_index);
                dto::Playlist {
                    id: h.id,
                    parent_id: 0,
                    name: h.name.clone(),
                    is_folder: false,
                    sort_order: h.id,
                    track_ids: e.iter().map(|e| e.track_id).collect(),
                }
            })
            .collect();
        Ok(dto::StickData {
            info: stick_info(Path::new(&path)),
            playlists,
            history,
            tracks,
            problems: db.problems.clone(),
        })
    })
    .await
}

/// Prefer the larger `_m` variant rekordbox writes next to the small one.
fn artwork_path(stick: &Stick, id: u32) -> Option<String> {
    if id == 0 {
        return None;
    }
    let art = stick.db.artwork.iter().find(|a| a.id == id)?;
    let small = stick.resolve(&art.path);
    let stem = small.file_stem()?.to_string_lossy().into_owned();
    let large = small.with_file_name(format!("{stem}_m.jpg"));
    [large, small]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

/// rekordbox's memory cue colours (rows of the `colors` table).
const MEMORY_COLORS: [[u8; 3]; 8] = [
    [0xff, 0x00, 0x7f],
    [0xff, 0x00, 0x00],
    [0xff, 0xa5, 0x00],
    [0xff, 0xff, 0x00],
    [0x00, 0xff, 0x00],
    [0x25, 0xfd, 0xe9],
    [0x00, 0x00, 0xff],
    [0x66, 0x00, 0x99],
];

pub(crate) fn display_color(c: &CuePoint) -> [u8; 3] {
    if c.slot > 0 {
        match hot_cue_color(c.hot_color) {
            Some((display, _)) => display,
            None if c.loop_end_ms.is_some() => [0xff, 0x8c, 0x00],
            None => [0x28, 0xe2, 0x14],
        }
    } else if (1..=8).contains(&c.memory_color) {
        MEMORY_COLORS[c.memory_color as usize - 1]
    } else {
        [0xe5, 0x48, 0x4d]
    }
}

pub(crate) fn cue_dto(c: &CuePoint) -> dto::Cue {
    dto::Cue {
        slot: c.slot,
        time_ms: c.time_ms,
        loop_end_ms: c.loop_end_ms,
        loop_beats: c.loop_beats,
        comment: c.comment.clone(),
        hot_color: c.hot_color,
        hot_led: c.hot_led,
        memory_color: c.memory_color,
        active_loop: c.active_loop,
        color: Some(display_color(c)),
    }
}

pub(crate) fn cue_point(c: &dto::Cue) -> CuePoint {
    CuePoint {
        slot: c.slot.min(8),
        time_ms: c.time_ms,
        loop_end_ms: c.loop_end_ms.filter(|&e| e > c.time_ms),
        loop_beats: c.loop_beats,
        comment: c.comment.clone(),
        hot_color: c.hot_color,
        hot_led: c.hot_led,
        memory_color: c.memory_color,
        active_loop: c.active_loop && c.slot == 0 && c.loop_end_ms.is_some(),
    }
}

fn stick_name(stick: &Stick) -> String {
    stick
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Cue points of a track: unsaved edits if any, otherwise from the stick.
fn track_cues(
    app: &AppHandle,
    stick: &Stick,
    t: &pioneer_formats::pdb::Track,
) -> (Vec<CuePoint>, bool) {
    if let Some(e) = app
        .state::<Edits>()
        .get(&edits::key(&stick_name(stick), &t.file_path))
    {
        return (e.iter().map(CuePoint::from).collect(), true);
    }
    let dat_path = stick.resolve(&t.analyze_path);
    let Some(dat) = read_anlz(&dat_path) else {
        return (Vec::new(), false);
    };
    let ext = read_anlz(&sibling(&dat_path, "EXT"));
    (
        read_cue_points(&dat, ext.as_ref()).unwrap_or_default(),
        false,
    )
}

/// `ANLZ0000.DAT` path → sibling with another extension.
pub(crate) fn sibling(dat: &Path, ext: &str) -> PathBuf {
    dat.with_extension(ext)
}

pub(crate) fn read_anlz(path: &Path) -> Option<AnlzFile> {
    std::fs::read(path)
        .ok()
        .and_then(|b| AnlzFile::parse(&b).ok())
}

fn find_track(stick: &Stick, id: u32) -> CmdResult<&pioneer_formats::pdb::Track> {
    stick
        .db
        .track(id)
        .ok_or_else(|| format!("track {id} not on the stick"))
}

/// Hot cue colours for the table.
#[tauri::command]
pub async fn cue_summary(
    app: AppHandle,
    path: String,
    track_ids: Vec<u32>,
) -> CmdResult<Vec<dto::CueSummary>> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let mut out = Vec::with_capacity(track_ids.len());
        for id in track_ids {
            let Ok(t) = find_track(&stick, id) else {
                continue;
            };
            let mut hot = vec![None; 8];
            let cues = track_cues(&app, &stick, t).0;
            for c in &cues {
                if (1..=8).contains(&c.slot) {
                    hot[c.slot as usize - 1] = Some(display_color(c));
                }
            }
            let memory = cues.iter().filter(|c| c.slot == 0).count();
            out.push(dto::CueSummary {
                track_id: id,
                hot,
                memory,
            });
        }
        Ok(out)
    })
    .await
}

fn audio_dto(i: &probe::AudioInfo) -> dto::AudioInfo {
    dto::AudioInfo {
        container: format!("{:?}", i.container),
        codec: match i.codec {
            Codec::Pcm => "PCM".into(),
            Codec::Float => "float".into(),
            Codec::Flac => "FLAC".into(),
            Codec::Mp3 => "MP3".into(),
            Codec::Mp4Audio => "AAC/ALAC".into(),
            Codec::Other(t) => format!("{t:#x}"),
            Codec::Unknown => "?".into(),
        },
        bits: i.bits,
        sample_rate: i.sample_rate,
        channels: i.channels,
        description: i.describe(),
    }
}

#[tauri::command]
pub async fn track_detail(
    app: AppHandle,
    path: String,
    track_id: u32,
) -> CmdResult<dto::TrackDetail> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let t = find_track(&stick, track_id)?;
        let dat_path = (!t.analyze_path.is_empty()).then(|| stick.resolve(&t.analyze_path));
        let (points, edited) = track_cues(&app, &stick, t);
        Ok(build_detail(
            track_id,
            dat_path.as_deref(),
            &stick.audio_path(t),
            &points,
            edited,
        ))
    })
    .await
}

/// Waveforms, beat grid, cues and audio format of a track, from its
/// analysis files (stick or rekordbox library) and audio file.
pub(crate) fn build_detail(
    track_id: u32,
    dat_path: Option<&Path>,
    audio_path: &Path,
    points: &[CuePoint],
    edited: bool,
) -> dto::TrackDetail {
    let dat = dat_path.and_then(read_anlz);
    let ext = dat_path.and_then(|p| read_anlz(&sibling(p, "EXT")));
    let two = dat_path.and_then(|p| read_anlz(&sibling(p, "2EX")));
    let pick = |file: &Option<AnlzFile>, t: [u8; 4], kind: &'static str, entry_len: usize| {
        file.as_ref()
            .and_then(|f| f.section(t))
            .map(|s| dto::Waveform {
                kind,
                entry_len,
                data: s.body.clone(),
            })
    };
    let previews: Vec<_> = [
        pick(&two, tag::WAVE_3BAND_PREVIEW, "three-band", 3),
        pick(&dat, tag::WAVE_PREVIEW, "mono", 1),
    ]
    .into_iter()
    .flatten()
    .collect();
    let details: Vec<_> = [
        pick(&two, tag::WAVE_3BAND_SCROLL, "three-band", 3),
        pick(&ext, tag::WAVE_COLOR_SCROLL, "color", 2),
        pick(&ext, tag::WAVE_SCROLL, "mono", 1),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut cues: Vec<dto::Cue> = points.iter().map(cue_dto).collect();
    cues.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));
    let beats = dat
        .as_ref()
        .and_then(|d| d.beat_grid())
        .and_then(|g| g.ok())
        .map(|g| g.beats.iter().map(|b| (b.time_ms, b.beat_number)).collect::<Vec<_>>())
        .unwrap_or_default();
    let grid = analysis::grid::Grid::from_beats(&beats).map(|g| dto::GridInfo {
        bpm: g.bpm,
        anchor_ms: g.anchor_ms,
        edited: false,
    });
    dto::TrackDetail {
        track_id,
        previews,
        details,
        cues,
        edited,
        beats,
        grid,
        audio: probe::probe_file(audio_path).ok().map(|i| audio_dto(&i)),
        file_size: std::fs::metadata(audio_path).map(|m| m.len()).unwrap_or(0),
    }
}

fn issue_text(
    p: &Problem,
    audio: Option<&probe::AudioInfo>,
) -> (&'static str, &'static str, String) {
    match p {
        Problem::MissingAudio => ("error", "missing", i18n::t!("app.verify.missingAudio")),
        Problem::MissingAnalysis(m) => ("error", "analysis", i18n::t!("app.verify.missingAnalysis", path = m)),
        Problem::SizeMismatch { database, actual } => (
            "warning",
            "size",
            i18n::t!("app.verify.sizeMismatch", actual = actual, database = database),
        ),
        Problem::BrokenTag { junk } => (
            "error",
            "broken-tag",
            i18n::t!("app.verify.brokenTag", junk = junk),
        ),
        Problem::WrongExtension { extension, actual } => (
            "error",
            "extension",
            i18n::t!("app.verify.wrongExtension", extension = extension, actual = actual),
        ),
        Problem::Unsupported { reason, .. } => {
            if audio.is_some_and(|a| a.codec == Codec::Float) {
                (
                    "error",
                    "float",
                    i18n::t!("app.verify.floatWav", bits = audio.and_then(|a| a.bits).unwrap_or(32)),
                )
            } else {
                ("error", "unsupported", reason.clone())
            }
        }
        Problem::Unreadable(e) => ("error", "unreadable", i18n::t!("app.verify.unreadable", error = e)),
    }
}

/// Earlier health check results, kept in the app's data folder so a stick
/// that is opened again only has its changed files read.
pub struct VerifyCache {
    path: PathBuf,
    data: std::sync::Mutex<verify::VerifyCache>,
}

impl VerifyCache {
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        VerifyCache {
            path,
            data: std::sync::Mutex::new(data),
        }
    }

    /// Best effort: without the file the next check reads everything again.
    fn save(&self, data: &verify::VerifyCache) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("json.tmp");
        if serde_json::to_vec(data).is_ok_and(|b| std::fs::write(&tmp, b).is_ok()) {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

#[tauri::command]
pub async fn verify_stick(app: AppHandle, path: String) -> CmdResult<dto::VerifyResult> {
    blocking(move || {
        let started = Instant::now();
        let stick = Stick::open(&path).map_err(err)?;
        let cancel = AtomicBool::new(false);
        let mut last = Instant::now();
        let cached = app.state::<VerifyCache>();
        let mut cache = cached.data.lock().unwrap();
        let r = verify::verify_cached(&stick, &Player::ALL, &mut cache, &cancel, |done, total| {
            if last.elapsed() > Duration::from_millis(80) || done == total {
                last = Instant::now();
                let _ = app.emit("verify-progress", (done, total));
            }
        });
        cached.save(&cache);
        drop(cache);

        let fs = r.filesystem.clone().unwrap_or_default();
        let mut issues = Vec::new();
        // Tracks every player refuses regardless of format rules.
        let mut unplayable_everywhere = Vec::new();
        let mut per_player: BTreeMap<Player, Vec<u32>> = BTreeMap::new();
        let mut notes: BTreeMap<Player, BTreeMap<String, usize>> = BTreeMap::new();
        for i in &r.issues {
            let (severity, kind, message) = issue_text(&i.problem, i.audio.as_ref());
            match &i.problem {
                Problem::Unsupported { players, .. } => {
                    for p in players {
                        per_player.entry(*p).or_default().push(i.track_id);
                        let label = if kind == "float" {
                            "WAV 32f".to_string()
                        } else {
                            i18n::t!("app.verify.noteUnsupported")
                        };
                        *notes.entry(*p).or_default().entry(label).or_default() += 1;
                    }
                }
                Problem::BrokenTag { .. }
                | Problem::MissingAudio
                | Problem::WrongExtension { .. }
                | Problem::Unreadable(_) => {
                    unplayable_everywhere.push(i.track_id);
                    let label = match kind {
                        "broken-tag" => i18n::t!("app.verify.noteBrokenTag"),
                        "missing" => i18n::t!("app.verify.noteMissing"),
                        _ => i18n::t!("app.verify.noteUnreadable"),
                    };
                    for p in Player::ALL {
                        *notes
                            .entry(p)
                            .or_default()
                            .entry(label.clone())
                            .or_default() += 1;
                    }
                }
                _ => {}
            }
            issues.push(dto::Issue {
                track_id: i.track_id,
                title: i.display.clone(),
                file_path: i.file_path.clone(),
                playlists: i.playlists.clone(),
                severity,
                kind,
                message,
                audio: i.audio.as_ref().map(|a| a.describe()),
            });
        }
        let players = Player::ALL
            .iter()
            .map(|&p| {
                let mount = compat::can_mount(p, &fs);
                let mut unplayable = per_player.remove(&p).unwrap_or_default();
                unplayable.extend(&unplayable_everywhere);
                unplayable.sort();
                unplayable.dedup();
                let format_note = notes
                    .get(&p)
                    .map(|m| {
                        m.iter()
                            .map(|(k, n)| format!("{n} × {k}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                dto::PlayerResult {
                    player: p.name().into(),
                    filesystem_ok: mount.is_ok(),
                    filesystem_note: match mount {
                        Ok(()) => i18n::t!("app.verify.fsSupported", fs = fs_name(&fs)),
                        Err(_) => i18n::t!("app.verify.fsUnsupported", fs = fs_name(&fs)),
                    },
                    unplayable,
                    format_note,
                }
            })
            .collect();
        Ok(dto::VerifyResult {
            tracks: r.tracks,
            filesystem: fs,
            players,
            issues,
            apple_double: r.apple_double.len(),
            db_problems: r.db_problems,
            formats: r.formats.into_iter().collect(),
            seconds: started.elapsed().as_secs_f64(),
        })
    })
    .await
}

fn fs_name(fs: &str) -> &str {
    match fs {
        "msdos" => "FAT32",
        "exfat" => "exFAT",
        "hfs" => "HFS+",
        "apfs" => "APFS",
        other => other,
    }
}

#[tauri::command]
pub fn start_extract(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    req: dto::ExtractRequest,
) -> CmdResult<u32> {
    let stick = Stick::open(&req.stick).map_err(err)?;
    let opts = ExtractOptions {
        playlists: req.playlists.clone(),
        history: req.history,
        numbered: req.numbered,
        naming: if req.artist_title {
            Naming::ArtistTitle
        } else {
            Naming::Original
        },
        duplicates: match req.duplicates.as_str() {
            "copy" => Duplicates::Copy,
            "symlink" => Duplicates::Symlink,
            _ => Duplicates::Clone,
        },
        m3u: req.m3u,
    };
    let plan = extract::plan(&stick, &opts);
    let (id, cancel) = jobs.start();
    let label = format!(
        "Export {}",
        stick.root.file_name().unwrap_or_default().to_string_lossy()
    );
    let dest = PathBuf::from(&req.dest);
    std::thread::spawn(move || {
        let mut ev = dto::JobEvent {
            id,
            label,
            state: "running",
            files_done: 0,
            files_total: plan.total_files,
            bytes_done: 0,
            bytes_total: plan.total_bytes,
            current: String::new(),
            message: String::new(),
            dest: dest.display().to_string(),
            ..Default::default()
        };
        let _ = app.emit("job", ev.clone());
        let mut last = Instant::now();
        let result = extract::execute(&plan, &dest, &opts, &cancel, |p| {
            if last.elapsed() > Duration::from_millis(100) {
                last = Instant::now();
                let mut e = ev.clone();
                e.files_done = p.files_done;
                e.bytes_done = p.bytes_done;
                e.current = p.current.clone();
                let _ = app.emit("job", e);
            }
        });
        match result {
            Ok(report) => {
                ev.files_done = plan.total_files;
                ev.bytes_done = plan.total_bytes;
                ev.state = if report.cancelled {
                    "cancelled"
                } else if report.errors.is_empty() {
                    "done"
                } else {
                    "error"
                };
                ev.message = if report.errors.is_empty() {
                    i18n::t!(
                        "app.extract.done",
                        copied = report.copied,
                        cloned = report.cloned,
                        existing = report.skipped_existing
                    )
                } else {
                    i18n::t!("app.extract.errors", count = report.errors.len(), errors = report.errors.join("; "))
                };
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e.to_string();
            }
        }
        let _ = app.emit("job", ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

#[tauri::command]
pub fn cancel_job(jobs: State<'_, Jobs>, id: u32) {
    jobs.cancel(id);
}

#[tauri::command]
pub fn reveal(path: String) -> CmdResult<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn eject(path: String) -> CmdResult<()> {
    blocking(move || {
        let out = std::process::Command::new("diskutil")
            .arg("eject")
            .arg(&path)
            .output()
            .map_err(err)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    })
    .await
}

#[tauri::command]
pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// Whether macOS gave the app Full Disk Access: the privacy database is
/// readable only with it, and trying does not ask the DJ anything.
#[tauri::command]
pub fn full_disk_access() -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return true;
    };
    std::fs::File::open(Path::new(&home).join("Library/Application Support/com.apple.TCC/TCC.db"))
        .is_ok()
}

/// System Settings at Privacy & Security › Full Disk Access.
#[tauri::command]
pub fn open_full_disk_settings() -> CmdResult<()> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .map_err(err)?;
    Ok(())
}

/// Keep edited cues in the app until they are written to the stick.
#[tauri::command]
pub async fn save_cues(
    app: AppHandle,
    path: String,
    track_id: u32,
    cues: Vec<dto::Cue>,
) -> CmdResult<Vec<dto::Cue>> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let t = find_track(&stick, track_id)?;
        let points: Vec<CuePoint> = cues.iter().map(cue_point).collect();
        app.state::<Edits>().set(
            edits::key(&stick_name(&stick), &t.file_path),
            points.iter().map(StoredCue::from).collect(),
        )?;
        let mut out: Vec<dto::Cue> = points.iter().map(cue_dto).collect();
        out.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));
        Ok(out)
    })
    .await
}

/// Drop unsaved edits of a track, going back to what is on the stick.
#[tauri::command]
pub async fn revert_cues(app: AppHandle, path: String, track_id: u32) -> CmdResult<()> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let t = find_track(&stick, track_id)?;
        app.state::<Edits>()
            .remove(&edits::key(&stick_name(&stick), &t.file_path))
    })
    .await
}

/// Track ids on this stick with unsaved cue edits.
#[tauri::command]
pub async fn pending_cues(app: AppHandle, path: String) -> CmdResult<Vec<u32>> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let paths = app.state::<Edits>().pending(&stick_name(&stick));
        Ok(stick
            .db
            .tracks
            .iter()
            .filter(|t| paths.contains(&t.file_path))
            .map(|t| t.id)
            .collect())
    })
    .await
}

pub(crate) fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    // SAFETY: localtime_r fills the zeroed struct for a valid time.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02}_{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// Write every pending cue edit of this stick into its analysis files.
#[tauri::command]
pub async fn write_cues(app: AppHandle, path: String) -> CmdResult<dto::CueWriteResult> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let name = stick_name(&stick);
        let store = app.state::<Edits>();
        let pending = store.pending(&name);
        let edits: Vec<(u32, Vec<CuePoint>)> = stick
            .db
            .tracks
            .iter()
            .filter(|t| pending.contains(&t.file_path))
            .filter_map(|t| {
                store
                    .get(&edits::key(&name, &t.file_path))
                    .map(|c| (t.id, c.iter().map(CuePoint::from).collect()))
            })
            .collect();
        let home = std::env::var("HOME").unwrap_or_default();
        let backup = PathBuf::from(home)
            .join("Stickwise/backups")
            .join(&name)
            .join(format!("cues-{}", timestamp()));
        let report = usb_tools::cue_write::write_cues(&stick, &edits, &backup);
        for id in &report.written {
            if let Ok(t) = find_track(&stick, *id) {
                store.remove(&edits::key(&name, &t.file_path))?;
            }
        }
        Ok(dto::CueWriteResult {
            written: report.written.len(),
            failed: report
                .failed
                .iter()
                .map(|(id, e)| {
                    let title = stick
                        .db
                        .track(*id)
                        .map(|t| t.title.clone())
                        .unwrap_or_default();
                    format!("{title}: {e}")
                })
                .collect(),
            backup_dir: backup.display().to_string(),
        })
    })
    .await
}

#[tauri::command]
pub fn cue_palette() -> Vec<dto::PaletteColor> {
    HOT_CUE_PALETTE
        .iter()
        .filter_map(|&code| hot_cue_color(code).map(|(color, _)| dto::PaletteColor { code, color }))
        .collect()
}

fn smart_kind(k: analysis::smart_cue::Kind) -> &'static str {
    use analysis::smart_cue::Kind::*;
    match k {
        MixIn => "mix-in",
        Drop => "drop",
        Break => "break",
        Build => "build",
        Up => "up",
        Down => "down",
        Outro => "outro",
        MixOut => "mix-out",
        Cue => "cue",
    }
}

/// Smart Cue suggestions for a track, from its beat grid and scroll waveform.
fn ai_key(stick: &Stick, t: &pioneer_formats::pdb::Track) -> String {
    let size = std::fs::metadata(stick.audio_path(t))
        .map(|m| m.len())
        .unwrap_or(t.file_size as u64);
    Sidecar::key(&stick_name(stick), &t.file_path, size)
}

/// Smart Cue for a track: the energy analysis, merged with CUE-DETR's
/// points when the track has been through the AI analysis.
fn suggestions(
    app: &AppHandle,
    stick: &Stick,
    t: &pioneer_formats::pdb::Track,
) -> Option<Vec<analysis::smart_cue::Suggestion>> {
    let dat = stick.resolve(&t.analyze_path);
    suggestions_for(app, &dat, t.bpm() as f32, &ai_key(stick, t), None)
}

pub(crate) fn suggestions_for(
    app: &AppHandle,
    dat_path: &Path,
    bpm: f32,
    ai_cache_key: &str,
    edited_beats: Option<&[(u32, u16)]>,
) -> Option<Vec<analysis::smart_cue::Suggestion>> {
    use analysis::smart_cue::{
        AI_THRESHOLD, AiCandidate, Input, analyze, bands_from_color, bands_from_three_band, with_ai,
    };
    let dat = read_anlz(dat_path)?;
    let columns = if let Some(s) = read_anlz(&sibling(dat_path, "2EX"))
        .and_then(|f| f.section(tag::WAVE_3BAND_SCROLL).cloned())
    {
        bands_from_three_band(&s.body)
    } else {
        let ext = read_anlz(&sibling(dat_path, "EXT"))?;
        bands_from_color(&ext.section(tag::WAVE_COLOR_SCROLL)?.body)
    };
    let beats: Vec<(u32, u16)> = match edited_beats {
        Some(b) => b.to_vec(),
        None => dat
            .beat_grid()
            .and_then(|g| g.ok())
            .map(|g| g.beats.iter().map(|b| (b.time_ms, b.beat_number)).collect())
            .unwrap_or_default(),
    };
    let input = Input {
        beats: &beats,
        columns: &columns,
        rate: 150.0,
        bpm,
    };
    let structure = analyze(&input);
    match app.state::<Sidecar>().cached(ai_cache_key) {
        Some(ai) => {
            let cands: Vec<AiCandidate> = ai
                .candidates
                .iter()
                .map(|c| AiCandidate {
                    time_ms: (c.time * 1000.0) as u32,
                    score: c.score as f32,
                })
                .collect();
            Some(with_ai(&input, &structure, &cands, AI_THRESHOLD))
        }
        None => Some(structure),
    }
}

pub(crate) fn smart_dto(s: &[analysis::smart_cue::Suggestion]) -> Vec<dto::SmartCue> {
    let plan = analysis::smart_cue::hot_cue_plan(s);
    s.iter()
        .map(|x| dto::SmartCue {
            kind: smart_kind(x.kind),
            time_ms: x.time_ms,
            bar: x.bar,
            label: x.label.clone(),
            confidence: x.confidence,
            color_code: x.kind.color_code(),
            color: hot_cue_color(x.kind.color_code())
                .map(|c| c.0)
                .unwrap_or([255, 255, 255]),
            in_plan: plan.iter().any(|p| p.time_ms == x.time_ms),
            ai: x.ai,
        })
        .collect()
}

/// Hot cues A–H from the Smart Cue plan, keeping memory cues.
pub(crate) fn plan_cues(
    existing: &[CuePoint],
    s: &[analysis::smart_cue::Suggestion],
) -> Vec<CuePoint> {
    let mut cues: Vec<CuePoint> = existing.iter().filter(|c| c.slot == 0).cloned().collect();
    for (i, x) in analysis::smart_cue::hot_cue_plan(s).iter().enumerate() {
        cues.push(CuePoint {
            hot_color: x.kind.color_code(),
            comment: x.label.clone(),
            ..CuePoint::hot(i as u8 + 1, x.time_ms)
        });
    }
    cues
}

#[tauri::command]
pub async fn smart_cue(
    app: AppHandle,
    path: String,
    track_id: u32,
) -> CmdResult<Vec<dto::SmartCue>> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let t = find_track(&stick, track_id)?;
        let s = suggestions(&app, &stick, t)
            .ok_or_else(|| i18n::t!("app.smartCue.noGrid"))?;
        Ok(smart_dto(&s))
    })
    .await
}

/// Put the Smart Cue plan on hot cues A–H of many tracks at once. Tracks
/// that already have hot cues are skipped unless `replace` is set.
#[tauri::command]
pub async fn smart_cue_apply(
    app: AppHandle,
    path: String,
    track_ids: Vec<u32>,
    replace: bool,
) -> CmdResult<dto::SmartCueBatch> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let name = stick_name(&stick);
        let mut r = dto::SmartCueBatch {
            applied: 0,
            skipped: 0,
            failed: 0,
        };
        for id in track_ids {
            let Ok(t) = find_track(&stick, id) else {
                continue;
            };
            let (mut cues, _) = track_cues(&app, &stick, t);
            if cues.iter().any(|c| c.slot > 0) && !replace {
                r.skipped += 1;
                continue;
            }
            let Some(s) = suggestions(&app, &stick, t) else {
                r.failed += 1;
                continue;
            };
            cues = plan_cues(&cues, &s);
            app.state::<Edits>().set(
                edits::key(&name, &t.file_path),
                cues.iter().map(StoredCue::from).collect(),
            )?;
            r.applied += 1;
        }
        Ok(r)
    })
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub installed: bool,
    /// Tracks of the request that already have AI results.
    pub analyzed: Vec<u32>,
}

#[tauri::command]
pub async fn ai_status(app: AppHandle, path: String, track_ids: Vec<u32>) -> CmdResult<AiStatus> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let sc = app.state::<Sidecar>();
        let analyzed = track_ids
            .into_iter()
            .filter(|&id| {
                stick
                    .db
                    .track(id)
                    .is_some_and(|t| sc.cached(&ai_key(&stick, t)).is_some())
            })
            .collect();
        Ok(AiStatus {
            installed: sc.available(),
            analyzed,
        })
    })
    .await
}

/// Run CUE-DETR on the tracks that have no AI results yet, as a background job.
#[tauri::command]
pub fn ai_analyze(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    path: String,
    track_ids: Vec<u32>,
) -> CmdResult<u32> {
    let stick = Stick::open(&path).map_err(err)?;
    let todo: Vec<(String, PathBuf)> = {
        let sc = app.state::<Sidecar>();
        track_ids
            .iter()
            .filter_map(|&id| stick.db.track(id))
            .map(|t| (ai_key(&stick, t), stick.audio_path(t)))
            .filter(|(k, p)| sc.cached(k).is_none() && p.is_file())
            .collect()
    };
    let (id, cancel) = jobs.start();
    let total = todo.len();
    std::thread::spawn(move || {
        let mut ev = dto::JobEvent {
            id,
            label: i18n::t!("app.ai.label"),
            state: "running",
            files_done: 0,
            files_total: total,
            bytes_done: 0,
            bytes_total: total as u64,
            current: String::new(),
            message: String::new(),
            dest: String::new(),
            ..Default::default()
        };
        let _ = app.emit("job", ev.clone());
        let result = crate::sidecar::ensure_installed(&app).and_then(|()| {
            app.state::<Sidecar>().run(&todo, &cancel, |done, name| {
                let mut e = ev.clone();
                e.files_done = done;
                e.bytes_done = done as u64;
                e.current = name.to_string();
                let _ = app.emit("job", e);
            })
        });
        ev.files_done = total;
        ev.bytes_done = total as u64;
        match result {
            Ok((done, errors)) => {
                ev.state = if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    "cancelled"
                } else if errors.is_empty() {
                    "done"
                } else {
                    "error"
                };
                ev.message = if errors.is_empty() {
                    i18n::t!("app.ai.done", count = done)
                } else {
                    i18n::t!("app.analysis.failed", count = errors.len(), errors = errors.join("; "))
                };
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e;
            }
        }
        let _ = app.emit("job", ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}
