//! Sync with Stickwise on other Macs: its settings, and fingerprinting the
//! library's tracks in the background so the other side can tell which
//! tracks both have, wherever their files lie.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::commands::CmdResult;
use crate::library_cmds::lib;

const MUSIC_DIR_KEY: &str = "peer.music_dir";

/// Look for new or changed files this often after the first pass.
const RESCAN: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PeerSettings {
    /// Where music coming from another Mac is saved.
    pub music_dir: String,
    /// `music_dir` is the default, not the DJ's choice.
    pub default_music_dir: bool,
    pub fingerprinted: usize,
    pub total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FingerprintProgress {
    fingerprinted: usize,
    total: usize,
}

fn default_music_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Music/Stickwise")
}

fn settings(app: &AppHandle) -> CmdResult<PeerSettings> {
    let (dir, (fingerprinted, total)) = lib(app, |l| Ok((l.meta(MUSIC_DIR_KEY)?, l.fingerprint_progress()?)))?;
    Ok(PeerSettings {
        default_music_dir: dir.is_none(),
        music_dir: dir.unwrap_or_else(|| default_music_dir().display().to_string()),
        fingerprinted,
        total,
    })
}

#[tauri::command]
pub fn peer_settings(app: AppHandle) -> CmdResult<PeerSettings> {
    settings(&app)
}

/// Save where synced music goes; `None` goes back to the default.
#[tauri::command]
pub fn peer_set_music_dir(app: AppHandle, path: Option<String>) -> CmdResult<PeerSettings> {
    match path {
        Some(p) => {
            if !Path::new(&p).is_absolute() || !Path::new(&p).is_dir() {
                return Err(format!("Složka {p} neexistuje."));
            }
            lib(&app, |l| l.set_meta(MUSIC_DIR_KEY, &p))?;
        }
        None => lib(&app, |l| l.clear_meta(MUSIC_DIR_KEY))?,
    }
    settings(&app)
}

/// Fingerprint the library's tracks for as long as the app runs: all of
/// them at start (files may have changed while it was closed), then new
/// ones as they come.
pub fn start_fingerprints(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("fingerprints".into())
        .spawn(move || {
            let mut all = true;
            loop {
                fingerprint_pass(&app, all);
                all = false;
                std::thread::sleep(RESCAN);
            }
        });
}

fn mtime(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// One look over the tracks: hash those never hashed and, with `all`, those
/// whose file changed since. Files that are not there (a drive not plugged
/// in) wait for a later pass.
fn fingerprint_pass(app: &AppHandle, all: bool) {
    let Ok(stamps) = lib(app, |l| l.file_stamps()) else { return };
    let mut last_emit = Instant::now();
    let mut changed = false;
    for s in stamps.iter().filter(|s| all || s.fingerprint.is_empty()) {
        let Ok(m) = std::fs::metadata(&s.path) else { continue };
        if !s.stale(m.len(), mtime(&m)) {
            continue;
        }
        // Hashing reads the whole file: done without holding the library.
        let Ok(fp) = library::peer::fingerprint_file(Path::new(&s.path)) else { continue };
        // Changed while being read: the next pass gets it right.
        let Ok(after) = std::fs::metadata(&s.path) else { continue };
        if after.len() != m.len() || mtime(&after) != mtime(&m) {
            continue;
        }
        if lib(app, |l| l.set_fingerprint(s.id, &fp, m.len(), mtime(&m))).is_err() {
            continue;
        }
        changed = true;
        if last_emit.elapsed() > Duration::from_millis(500) {
            emit_progress(app);
            last_emit = Instant::now();
        }
    }
    if changed {
        emit_progress(app);
    }
}

fn emit_progress(app: &AppHandle) {
    if let Ok((fingerprinted, total)) = lib(app, |l| l.fingerprint_progress()) {
        let _ = app.emit("peer-fingerprints", FingerprintProgress { fingerprinted, total });
    }
}
