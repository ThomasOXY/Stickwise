//! Commands for the local library: browse, import from rekordbox, edit
//! playlists and sets, cues, Smart Cue, and sync a selection to a stick.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use library::{Library, Playlist, RekordboxDump};
use pioneer_formats::anlz::CuePoint;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use usb_tools::sync::{self, SyncPlaylist, SyncRequest, SyncTrack};

use crate::commands::{
    CmdResult, blocking, build_detail, cue_dto, cue_point, display_color, err, plan_cues,
    smart_dto, suggestions_for, timestamp,
};
use crate::dto;
use crate::jobs::Jobs;
use crate::sidecar::Sidecar;

pub struct LibState(pub Mutex<Library>);

fn lib<T>(app: &AppHandle, f: impl FnOnce(&mut Library) -> library::Result<T>) -> CmdResult<T> {
    let state = app.state::<LibState>();
    let mut l = state.0.lock().map_err(err)?;
    f(&mut l).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    pub duration: u16,
    pub format: String,
    pub file_path: String,
    pub abs_path: String,
    pub rating: u8,
    pub file_size: u64,
    pub artwork: Option<String>,
    /// The audio file is not where the library says.
    pub missing: bool,
    /// Has rekordbox analysis (waveform, beat grid).
    pub analyzed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryData {
    pub tracks: Vec<LibTrack>,
    pub playlists: Vec<Playlist>,
    /// rekordbox library folders found on this Mac.
    pub rekordbox_sources: Vec<String>,
    pub last_import: Option<String>,
}

fn format_label(path: &str, bitrate: u32, depth: u32) -> String {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    match ext.as_str() {
        "MP3" | "M4A" | "AAC" => format!("{ext} {bitrate}"),
        "AIF" => format!("AIFF {depth}"),
        _ if depth > 0 => format!("{ext} {depth}"),
        _ => ext,
    }
}

/// rekordbox library folders (holding master.db), newest first.
fn rekordbox_sources() -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = Vec::new();
    let mut check = |dir: PathBuf| {
        if let Ok(m) = std::fs::metadata(dir.join("master.db"))
            && let Ok(t) = m.modified()
        {
            found.push((t, dir.display().to_string()));
        }
    };
    if let Ok(home) = std::env::var("HOME") {
        check(PathBuf::from(home).join("Library/Pioneer/rekordbox"));
    }
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        for v in rd.flatten() {
            check(v.path().join("PIONEER/Master"));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|f| f.1).collect()
}

#[tauri::command]
pub async fn library_data(app: AppHandle) -> CmdResult<LibraryData> {
    blocking(move || {
        let (tracks, playlists, last) = lib(&app, |l| {
            Ok((l.tracks()?, l.playlists()?, l.meta("last_import")?))
        })?;
        let tracks = tracks
            .into_iter()
            .map(|t| {
                let size = std::fs::metadata(&t.path).map(|m| m.len()).ok();
                LibTrack {
                    id: t.id,
                    format: format_label(&t.path, t.bitrate, t.sample_depth),
                    file_path: t.path.clone(),
                    abs_path: t.path.clone(),
                    missing: size.is_none(),
                    file_size: size.unwrap_or(0),
                    analyzed: !t.anlz.is_empty() && Path::new(&t.anlz).is_file(),
                    artwork: (!t.artwork.is_empty() && Path::new(&t.artwork).is_file())
                        .then(|| t.artwork.clone()),
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    genre: t.genre,
                    bpm: t.bpm,
                    key: t.key,
                    duration: t.duration.round() as u16,
                    rating: t.rating,
                }
            })
            .collect();
        Ok(LibraryData {
            tracks,
            playlists,
            rekordbox_sources: rekordbox_sources(),
            last_import: last,
        })
    })
    .await
}

/// Import (or refresh) the library from rekordbox.
#[tauri::command]
pub async fn library_import(app: AppHandle, source: String) -> CmdResult<library::ImportReport> {
    blocking(move || {
        let sc = app.state::<Sidecar>();
        if !sc.installed() {
            return Err(
                "Import potřebuje pomocné nástroje: v terminálu spusť  cd sidecar && uv sync"
                    .into(),
            );
        }
        let out = sc.data.join("rekordbox-dump.json");
        let status = std::process::Command::new(sc.dir.join(".venv/bin/python"))
            .arg(sc.dir.join("rekordbox_dump.py"))
            .arg(&source)
            .arg(&out)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("Import nejde spustit: {e}"))?;
        if !status.status.success() {
            let msg = String::from_utf8_lossy(&status.stderr);
            let last = msg
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("neznámá chyba");
            return Err(format!("Databázi rekordboxu nejde přečíst: {last}"));
        }
        let dump: RekordboxDump =
            serde_json::from_slice(&std::fs::read(&out).map_err(err)?).map_err(err)?;
        let report = lib(&app, |l| {
            let r = l.import_rekordbox(&dump)?;
            l.set_meta("last_import", &format!("{} z {source}", timestamp()))?;
            Ok(r)
        })?;
        let _ = std::fs::remove_file(&out);
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn library_create_playlist(
    app: AppHandle,
    parent: i64,
    name: String,
    kind: String,
) -> CmdResult<i64> {
    lib(&app, |l| {
        l.create_playlist(parent, &name, kind == "folder", kind == "set")
    })
}

#[tauri::command]
pub fn library_rename_playlist(app: AppHandle, id: i64, name: String) -> CmdResult<()> {
    lib(&app, |l| l.rename_playlist(id, &name))
}

#[tauri::command]
pub fn library_delete_playlist(app: AppHandle, id: i64) -> CmdResult<()> {
    lib(&app, |l| l.delete_playlist(id))
}

#[tauri::command]
pub fn library_move_playlist(app: AppHandle, id: i64, parent: i64, index: usize) -> CmdResult<()> {
    lib(&app, |l| l.move_playlist(id, parent, index))
}

#[tauri::command]
pub fn library_set_tracks(app: AppHandle, id: i64, track_ids: Vec<i64>) -> CmdResult<()> {
    lib(&app, |l| l.set_playlist_tracks(id, &track_ids))
}

fn track(app: &AppHandle, id: i64) -> CmdResult<library::Track> {
    lib(app, |l| l.track(id))?.ok_or_else(|| format!("skladba {id} není v knihovně"))
}

fn ai_key(t: &library::Track) -> String {
    let size = std::fs::metadata(&t.path).map(|m| m.len()).unwrap_or(0);
    Sidecar::key("library", &t.path, size)
}

#[tauri::command]
pub async fn library_track_detail(app: AppHandle, id: i64) -> CmdResult<dto::TrackDetail> {
    blocking(move || {
        let t = track(&app, id)?;
        let cues = lib(&app, |l| l.cues(id))?;
        let dat = (!t.anlz.is_empty()).then(|| PathBuf::from(&t.anlz));
        Ok(build_detail(
            id as u32,
            dat.as_deref(),
            Path::new(&t.path),
            &cues,
            false,
        ))
    })
    .await
}

#[tauri::command]
pub async fn library_save_cues(
    app: AppHandle,
    id: i64,
    cues: Vec<dto::Cue>,
) -> CmdResult<Vec<dto::Cue>> {
    blocking(move || {
        let points: Vec<CuePoint> = cues.iter().map(cue_point).collect();
        lib(&app, |l| l.set_cues(id, &points))?;
        let mut out: Vec<dto::Cue> = points.iter().map(cue_dto).collect();
        out.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn library_cue_summary(
    app: AppHandle,
    track_ids: Vec<i64>,
) -> CmdResult<Vec<dto::CueSummary>> {
    blocking(move || {
        let mut out = Vec::new();
        for id in track_ids {
            let cues = lib(&app, |l| l.cues(id))?;
            let mut hot = vec![None; 8];
            for c in cues.iter().filter(|c| (1..=8).contains(&c.slot)) {
                hot[c.slot as usize - 1] = Some(display_color(c));
            }
            out.push(dto::CueSummary {
                track_id: id as u32,
                hot,
            });
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn library_smart_cue(app: AppHandle, id: i64) -> CmdResult<Vec<dto::SmartCue>> {
    blocking(move || {
        let t = track(&app, id)?;
        let s = suggestions_for(&app, Path::new(&t.anlz), t.bpm as f32, &ai_key(&t))
            .ok_or("Skladba nemá analýzu z rekordboxu, Smart Cue ji zatím neumí rozebrat.")?;
        Ok(smart_dto(&s))
    })
    .await
}

#[tauri::command]
pub async fn library_smart_cue_apply(
    app: AppHandle,
    track_ids: Vec<i64>,
    replace: bool,
) -> CmdResult<dto::SmartCueBatch> {
    blocking(move || {
        let mut r = dto::SmartCueBatch {
            applied: 0,
            skipped: 0,
            failed: 0,
        };
        for id in track_ids {
            let Ok(t) = track(&app, id) else { continue };
            let cues = lib(&app, |l| l.cues(id))?;
            if cues.iter().any(|c| c.slot > 0) && !replace {
                r.skipped += 1;
                continue;
            }
            let Some(s) = suggestions_for(&app, Path::new(&t.anlz), t.bpm as f32, &ai_key(&t))
            else {
                r.failed += 1;
                continue;
            };
            let next = plan_cues(&cues, &s);
            lib(&app, |l| l.set_cues(id, &next))?;
            r.applied += 1;
        }
        Ok(r)
    })
    .await
}

#[tauri::command]
pub async fn library_ai_status(
    app: AppHandle,
    track_ids: Vec<i64>,
) -> CmdResult<crate::commands::AiStatus> {
    blocking(move || {
        let sc = app.state::<Sidecar>();
        let analyzed = track_ids
            .into_iter()
            .filter(|&id| track(&app, id).is_ok_and(|t| sc.cached(&ai_key(&t)).is_some()))
            .map(|id| id as u32)
            .collect();
        Ok(crate::commands::AiStatus {
            installed: sc.installed(),
            analyzed,
        })
    })
    .await
}

fn emit_job(app: &AppHandle, ev: &dto::JobEvent) {
    let _ = app.emit("job", ev.clone());
}

#[tauri::command]
pub fn library_ai_analyze(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    track_ids: Vec<i64>,
) -> CmdResult<u32> {
    let todo: Vec<(String, PathBuf)> = {
        let sc = app.state::<Sidecar>();
        track_ids
            .iter()
            .filter_map(|&id| track(&app, id).ok())
            .map(|t| (ai_key(&t), PathBuf::from(&t.path)))
            .filter(|(k, p)| sc.cached(k).is_none() && p.is_file())
            .collect()
    };
    let (id, cancel) = jobs.start();
    let total = todo.len();
    std::thread::spawn(move || {
        let mut ev = dto::JobEvent {
            id,
            label: "AI analýza cue".into(),
            state: "running",
            files_done: 0,
            files_total: total,
            bytes_done: 0,
            bytes_total: total as u64,
            current: String::new(),
            message: String::new(),
            dest: String::new(),
        };
        emit_job(&app, &ev);
        let result = app.state::<Sidecar>().run(&todo, &cancel, |done, name| {
            let mut e = ev.clone();
            e.files_done = done;
            e.bytes_done = done as u64;
            e.current = name.to_string();
            emit_job(&app, &e);
        });
        ev.files_done = total;
        ev.bytes_done = total as u64;
        match result {
            Ok((done, errors)) => {
                ev.state = if cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else if errors.is_empty() {
                    "done"
                } else {
                    "error"
                };
                ev.message = if errors.is_empty() {
                    format!("AI analýza hotová u {done} skladeb")
                } else {
                    format!(
                        "{} skladeb nešlo analyzovat: {}",
                        errors.len(),
                        errors.join("; ")
                    )
                };
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e;
            }
        }
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

/// Selected playlists with the folders above them and everything inside
/// selected folders, plus their tracks.
fn sync_request(app: &AppHandle, stick: &str, playlist_ids: &[i64]) -> CmdResult<SyncRequest> {
    let (all, tracks) = lib(app, |l| Ok((l.playlists()?, l.tracks()?)))?;
    let by_id: HashMap<i64, &Playlist> = all.iter().map(|p| (p.id, p)).collect();
    let mut chosen: HashSet<i64> = HashSet::new();
    let mut stack: Vec<i64> = playlist_ids.to_vec();
    while let Some(id) = stack.pop() {
        if chosen.insert(id) {
            stack.extend(all.iter().filter(|p| p.parent_id == id).map(|p| p.id));
        }
    }
    for &id in playlist_ids {
        let mut p = by_id.get(&id).map(|p| p.parent_id).unwrap_or(0);
        while p != 0 && chosen.insert(p) {
            p = by_id.get(&p).map(|x| x.parent_id).unwrap_or(0);
        }
    }
    let lists: Vec<&Playlist> = all.iter().filter(|p| chosen.contains(&p.id)).collect();
    let wanted: HashSet<i64> = lists
        .iter()
        .flat_map(|p| p.track_ids.iter().copied())
        .collect();
    let mut sync_tracks = Vec::new();
    for t in tracks.into_iter().filter(|t| wanted.contains(&t.id)) {
        let cues = lib(app, |l| l.cues(t.id))?;
        sync_tracks.push(SyncTrack {
            key: t.id,
            audio: PathBuf::from(&t.path),
            title: t.title,
            artist: t.artist,
            album: t.album,
            genre: t.genre,
            label: t.label,
            remixer: t.remixer,
            key_name: t.key,
            bpm: t.bpm,
            duration: t.duration,
            bitrate: t.bitrate,
            sample_rate: t.sample_rate,
            sample_depth: t.sample_depth,
            rating: t.rating,
            color: t.color,
            comment: t.comment,
            year: t.year,
            track_number: t.track_number,
            date_added: t.date_added,
            anlz_dat: (!t.anlz.is_empty()).then(|| PathBuf::from(t.anlz)),
            cues,
        });
    }
    let name = Path::new(stick)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(SyncRequest {
        stick: PathBuf::from(stick),
        tracks: sync_tracks,
        playlists: lists
            .iter()
            .map(|p| SyncPlaylist {
                key: p.id,
                parent: if chosen.contains(&p.parent_id) {
                    p.parent_id
                } else {
                    0
                },
                name: p.name.clone(),
                is_folder: p.is_folder,
                sort: p.sort_order,
                tracks: p.track_ids.clone(),
            })
            .collect(),
        device_name: name,
        date: timestamp()[..10].to_string(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlanDto {
    pub tracks: usize,
    pub playlists: usize,
    pub copy_files: usize,
    pub copy_bytes: u64,
    pub reuse_files: usize,
    pub free_bytes: u64,
    pub missing_audio: Vec<String>,
    pub no_analysis: Vec<String>,
}

fn selection_key(stick: &str) -> String {
    format!(
        "sync:{}",
        Path::new(stick)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    )
}

/// Playlists last synced to this stick.
#[tauri::command]
pub fn sync_selection(app: AppHandle, stick: String) -> CmdResult<Vec<i64>> {
    let v = lib(&app, |l| l.meta(&selection_key(&stick)))?;
    Ok(v.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn sync_plan(
    app: AppHandle,
    stick: String,
    playlist_ids: Vec<i64>,
) -> CmdResult<SyncPlanDto> {
    blocking(move || {
        let req = sync_request(&app, &stick, &playlist_ids)?;
        let p = sync::plan(&req);
        Ok(SyncPlanDto {
            tracks: p.tracks.len(),
            playlists: req.playlists.iter().filter(|p| !p.is_folder).count(),
            copy_files: p.copy_files,
            copy_bytes: p.copy_bytes,
            reuse_files: p.reuse_files,
            free_bytes: p.free_bytes,
            missing_audio: p.missing_audio,
            no_analysis: p.no_analysis,
        })
    })
    .await
}

const PHASES: [(&str, &str); 4] = [
    ("copy", "Kopírování skladeb"),
    ("analysis", "Analýzy a cue"),
    ("database", "Databáze"),
    ("verify", "Kontrola"),
];

#[tauri::command]
pub fn sync_start(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    stick: String,
    playlist_ids: Vec<i64>,
) -> CmdResult<u32> {
    let req = sync_request(&app, &stick, &playlist_ids)?;
    lib(&app, |l| {
        l.set_meta(
            &selection_key(&stick),
            &serde_json::to_string(&playlist_ids).unwrap_or_default(),
        )
    })?;
    let (id, cancel) = jobs.start();
    std::thread::spawn(move || {
        let plan = sync::plan(&req);
        let name = req.device_name.clone();
        let mut ev = dto::JobEvent {
            id,
            label: format!("Sync {name}"),
            state: "running",
            files_done: 0,
            files_total: plan.copy_files,
            bytes_done: 0,
            bytes_total: plan.copy_bytes.max(1),
            current: String::new(),
            message: String::new(),
            dest: req.stick.display().to_string(),
        };
        emit_job(&app, &ev);
        let home = std::env::var("HOME").unwrap_or_default();
        let backup = PathBuf::from(home)
            .join("RB-EXPORT/backups")
            .join(&name)
            .join(format!("sync-{}", timestamp()));
        let result = sync::execute(&req, &plan, &backup, &cancel, |p| {
            let mut e = ev.clone();
            e.files_done = p.done;
            e.files_total = p.total;
            e.bytes_done = p.bytes_done;
            e.bytes_total = p.bytes_total.max(1);
            let phase = PHASES
                .iter()
                .find(|x| x.0 == p.phase)
                .map(|x| x.1)
                .unwrap_or("");
            e.current = if p.current.is_empty() {
                phase.to_string()
            } else {
                format!("{phase}: {}", p.current)
            };
            emit_job(&app, &e);
        });
        match result {
            Ok(r) if r.cancelled => {
                ev.state = "cancelled";
                ev.message =
                    "Synchronizace zastavena. Flashka má dál svou předchozí knihovnu.".into();
            }
            Ok(r) => {
                ev.state = if r.warnings.is_empty() {
                    "done"
                } else {
                    "error"
                };
                ev.files_done = ev.files_total;
                ev.bytes_done = ev.bytes_total;
                let mut msg = format!(
                    "Na flashce je {} skladeb v {} playlistech. Zkopírováno {}, použito z flashky {}.",
                    r.tracks, r.playlists, r.copied, r.reused
                );
                if !r.moved_aside.is_empty() {
                    msg += &format!(" Odsunuto stranou: {}.", r.moved_aside.join(", "));
                }
                if !r.warnings.is_empty() {
                    msg += &format!(" Upozornění: {}", r.warnings.join("; "));
                }
                ev.message = msg;
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e.to_string();
            }
        }
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}
