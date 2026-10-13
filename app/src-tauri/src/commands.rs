use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pioneer_formats::anlz::{AnlzFile, CueListKind, CueType, tag};
use pioneer_formats::pdb::file_type;
use tauri::{AppHandle, Emitter, Manager, State};
use usb_tools::compat::{self, Player};
use usb_tools::extract::{self, Duplicates, ExtractOptions, Naming};
use usb_tools::probe::{self, Codec};
use usb_tools::verify::{self, Problem, filesystem_type};
use usb_tools::Stick;

use crate::dto;
use crate::jobs::Jobs;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

fn disk_space(path: &Path) -> (u64, u64) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else { return (0, 0) };
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
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        total_bytes: total,
        free_bytes: free,
        filesystem: filesystem_type(path).unwrap_or_default(),
    }
}

/// Mounted volumes that carry a rekordbox device library.
#[tauri::command]
pub fn list_sticks() -> Vec<dto::StickInfo> {
    let Ok(rd) = std::fs::read_dir("/Volumes") else { return Vec::new() };
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
    let depth = if t.sample_depth > 0 { format!(" {}", t.sample_depth) } else { String::new() };
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
                track_ids: if p.is_folder { Vec::new() } else { db.playlist_tracks(p.id) },
            })
            .collect();
        let history = db
            .history_playlists
            .iter()
            .map(|h| {
                let mut e: Vec<_> = db.history_entries.iter().filter(|e| e.playlist_id == h.id).collect();
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
    [large, small].into_iter().find(|p| p.is_file()).map(|p| p.display().to_string())
}

/// `ANLZ0000.DAT` path → sibling with another extension.
fn sibling(dat: &Path, ext: &str) -> PathBuf {
    dat.with_extension(ext)
}

fn read_anlz(path: &Path) -> Option<AnlzFile> {
    std::fs::read(path).ok().and_then(|b| AnlzFile::parse(&b).ok())
}

fn find_track(stick: &Stick, id: u32) -> CmdResult<&pioneer_formats::pdb::Track> {
    stick.db.track(id).ok_or_else(|| format!("track {id} not on the stick"))
}

/// Hot cue colours for the table.
#[tauri::command]
pub async fn cue_summary(path: String, track_ids: Vec<u32>) -> CmdResult<Vec<dto::CueSummary>> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let mut out = Vec::with_capacity(track_ids.len());
        for id in track_ids {
            let Ok(t) = find_track(&stick, id) else { continue };
            let mut hot = vec![None; 8];
            let dat = stick.resolve(&t.analyze_path);
            if let Some(ext) = read_anlz(&sibling(&dat, "EXT")) {
                for list in ext.ext_cue_lists().unwrap_or_default() {
                    if list.kind != CueListKind::Hot {
                        continue;
                    }
                    for c in list.cues {
                        if (1..=8).contains(&c.hot_cue) {
                            hot[c.hot_cue as usize - 1] = Some(c.color.map(|(_, rgb)| rgb).unwrap_or([0, 0, 0]));
                        }
                    }
                }
            }
            out.push(dto::CueSummary { track_id: id, hot });
        }
        Ok(out)
    })
    .await
}

fn waveform(file: &AnlzFile, preferences: &[([u8; 4], &'static str, usize)]) -> Option<dto::Waveform> {
    preferences.iter().find_map(|(t, kind, entry_len)| {
        file.section(*t).map(|s| dto::Waveform { kind, entry_len: *entry_len, data: s.body.clone() })
    })
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
pub async fn track_detail(path: String, track_id: u32) -> CmdResult<dto::TrackDetail> {
    blocking(move || {
        let stick = Stick::open(&path).map_err(err)?;
        let t = find_track(&stick, track_id)?;
        let dat_path = stick.resolve(&t.analyze_path);
        let dat = read_anlz(&dat_path);
        let ext = read_anlz(&sibling(&dat_path, "EXT"));
        let two = read_anlz(&sibling(&dat_path, "2EX"));

        let preview = two
            .as_ref()
            .and_then(|f| waveform(f, &[(tag::WAVE_3BAND_PREVIEW, "three-band", 3)]))
            .or_else(|| ext.as_ref().and_then(|f| waveform(f, &[(tag::WAVE_COLOR_SCROLL, "color", 2)])))
            .or_else(|| dat.as_ref().and_then(|f| waveform(f, &[(tag::WAVE_PREVIEW, "mono", 1)])));
        let detail = two
            .as_ref()
            .and_then(|f| waveform(f, &[(tag::WAVE_3BAND_SCROLL, "three-band", 3)]))
            .or_else(|| ext.as_ref().and_then(|f| waveform(f, &[(tag::WAVE_COLOR_SCROLL, "color", 2)])))
            .or_else(|| ext.as_ref().and_then(|f| waveform(f, &[(tag::WAVE_SCROLL, "mono", 1)])));

        let mut cues = Vec::new();
        let loop_end = |kind: CueType, end: u32| (kind == CueType::Loop && end != u32::MAX).then_some(end);
        let ext_lists = ext.as_ref().and_then(|f| f.ext_cue_lists().ok()).unwrap_or_default();
        if !ext_lists.is_empty() {
            for list in ext_lists {
                for c in list.cues {
                    cues.push(dto::Cue {
                        slot: c.hot_cue,
                        time_ms: c.time_ms,
                        loop_end_ms: loop_end(c.cue_type, c.loop_time_ms),
                        color: c.color.map(|(_, rgb)| rgb),
                        comment: c.comment.unwrap_or_default(),
                    });
                }
            }
        } else if let Some(d) = &dat {
            for list in d.cue_lists().unwrap_or_default() {
                for c in list.cues {
                    cues.push(dto::Cue {
                        slot: c.hot_cue,
                        time_ms: c.time_ms,
                        loop_end_ms: loop_end(c.cue_type, c.loop_time_ms),
                        color: None,
                        comment: String::new(),
                    });
                }
            }
        }
        cues.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));

        let beats = dat
            .as_ref()
            .and_then(|d| d.beat_grid())
            .and_then(|g| g.ok())
            .map(|g| g.beats.iter().map(|b| (b.time_ms, b.beat_number)).collect())
            .unwrap_or_default();
        let audio_path = stick.audio_path(t);
        Ok(dto::TrackDetail {
            track_id,
            preview,
            detail,
            cues,
            beats,
            audio: probe::probe_file(&audio_path).ok().map(|i| audio_dto(&i)),
            file_size: std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0),
        })
    })
    .await
}

fn issue_text(p: &Problem, audio: Option<&probe::AudioInfo>) -> (&'static str, &'static str, String) {
    match p {
        Problem::MissingAudio => ("error", "missing", "Soubor chybí na flashce.".into()),
        Problem::MissingAnalysis(m) => ("error", "analysis", format!("Chybí analýza: {m}")),
        Problem::SizeMismatch { database, actual } => (
            "warning",
            "size",
            format!("Soubor má {actual} B, databáze uvádí {database} B. Hrát půjde."),
        ),
        Problem::BrokenTag { junk } => (
            "error",
            "broken-tag",
            format!("Poškozený tag MP3 ({junk} B smetí před zvukem). Přehrávač skladbu může odmítnout."),
        ),
        Problem::WrongExtension { extension, actual } => {
            ("error", "extension", format!("Soubor .{extension} je ve skutečnosti {actual}."))
        }
        Problem::Unsupported { reason, .. } => {
            if audio.is_some_and(|a| a.codec == Codec::Float) {
                (
                    "error",
                    "float",
                    format!("{}-bit float WAV. CDJ přehraje jen 16 a 24 bitů.", audio.and_then(|a| a.bits).unwrap_or(32)),
                )
            } else {
                ("error", "unsupported", reason.clone())
            }
        }
        Problem::Unreadable(e) => ("error", "unreadable", format!("Soubor nejde přečíst: {e}")),
    }
}

#[tauri::command]
pub async fn verify_stick(app: AppHandle, path: String) -> CmdResult<dto::VerifyResult> {
    blocking(move || {
        let started = Instant::now();
        let stick = Stick::open(&path).map_err(err)?;
        let cancel = AtomicBool::new(false);
        let mut last = Instant::now();
        let r = verify::verify(&stick, &Player::ALL, &cancel, |done, total| {
            if last.elapsed() > Duration::from_millis(80) || done == total {
                last = Instant::now();
                let _ = app.emit("verify-progress", (done, total));
            }
        });

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
                        let label = if kind == "float" { "WAV 32f".to_string() } else { "nepodporovaný formát".into() };
                        *notes.entry(*p).or_default().entry(label).or_default() += 1;
                    }
                }
                Problem::BrokenTag { .. } | Problem::MissingAudio | Problem::WrongExtension { .. } | Problem::Unreadable(_) => {
                    unplayable_everywhere.push(i.track_id);
                    let label = match kind {
                        "broken-tag" => "poškozený tag",
                        "missing" => "chybí soubor",
                        _ => "nečitelný soubor",
                    };
                    for p in Player::ALL {
                        *notes.entry(p).or_default().entry(label.to_string()).or_default() += 1;
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
                    .map(|m| m.iter().map(|(k, n)| format!("{n} × {k}")).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                dto::PlayerResult {
                    player: p.name().into(),
                    filesystem_ok: mount.is_ok(),
                    filesystem_note: match mount {
                        Ok(()) => format!("{} podporuje", fs_name(&fs)),
                        Err(_) => format!("{} nepodporuje, použij FAT32", fs_name(&fs)),
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
pub fn start_extract(app: AppHandle, jobs: State<'_, Jobs>, req: dto::ExtractRequest) -> CmdResult<u32> {
    let stick = Stick::open(&req.stick).map_err(err)?;
    let opts = ExtractOptions {
        playlists: req.playlists.clone(),
        history: req.history,
        numbered: req.numbered,
        naming: if req.artist_title { Naming::ArtistTitle } else { Naming::Original },
        duplicates: match req.duplicates.as_str() {
            "copy" => Duplicates::Copy,
            "symlink" => Duplicates::Symlink,
            _ => Duplicates::Clone,
        },
        m3u: req.m3u,
    };
    let plan = extract::plan(&stick, &opts);
    let (id, cancel) = jobs.start();
    let label = format!("Export {}", stick.root.file_name().unwrap_or_default().to_string_lossy());
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
                    format!(
                        "Zkopírováno {}, klonováno {}, už existovalo {}",
                        report.copied, report.cloned, report.skipped_existing
                    )
                } else {
                    format!("{} chyb: {}", report.errors.len(), report.errors.join("; "))
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
    std::process::Command::new("open").arg("-R").arg(&path).status().map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn eject(path: String) -> CmdResult<()> {
    blocking(move || {
        let out = std::process::Command::new("diskutil").arg("eject").arg(&path).output().map_err(err)?;
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
