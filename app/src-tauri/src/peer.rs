//! Sync with Stickwise on other Macs: its settings, the other Macs on the
//! network and pairing with them, and fingerprinting the library's tracks
//! in the background so the other side can tell which tracks both have,
//! wherever their files lie.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use peerlink::{Event, Node, Peer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::{CmdResult, blocking, err};
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

// ---- Other Macs ---------------------------------------------------------

/// The node talking to other Macs; `Err` says why it could not start.
pub struct PeerNode(pub Result<Node, String>);

impl PeerNode {
    fn get(&self) -> CmdResult<&Node> {
        self.0.as_ref().map_err(|e| format!("Sync mezi Macy neběží: {e}"))
    }
}

/// This Mac's name as the DJ named it in System Settings.
fn computer_name() -> String {
    std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Mac".into())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PairRequestDto {
    request: u64,
    peer: Peer,
    code: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PairResultDto {
    peer: Peer,
    accepted: bool,
    message: String,
}

pub fn start_node(app: &AppHandle, dir: PathBuf) -> PeerNode {
    let handle = app.clone();
    let on_event = Arc::new(move |e: Event| {
        let _ = match e {
            Event::PeersChanged => handle.emit("peer-peers", ()),
            Event::PairRequest { request, peer, code } => {
                handle.emit("peer-pair-request", PairRequestDto { request, peer, code })
            }
            Event::PairRequestGone { request } => handle.emit("peer-pair-gone", request),
            Event::PairResult { peer, accepted, message } => {
                handle.emit("peer-pair-result", PairResultDto { peer, accepted, message })
            }
        };
    });
    PeerNode(
        Node::start(peerlink::Config {
            dir,
            name: computer_name(),
            discovery: true,
            on_event,
            on_session: None,
        })
        .map_err(err),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerList {
    /// This Mac's name.
    pub name: String,
    pub peers: Vec<Peer>,
    /// Why sync between Macs is not running, if it is not.
    pub error: Option<String>,
}

#[tauri::command]
pub fn peer_list(node: State<'_, PeerNode>) -> PeerList {
    match &node.0 {
        Ok(n) => PeerList { name: computer_name(), peers: n.peers(), error: None },
        Err(e) => PeerList { name: computer_name(), peers: Vec::new(), error: Some(e.clone()) },
    }
}

/// Ask a Mac to pair; returns the code both Macs show.
#[tauri::command]
pub async fn peer_pair(app: AppHandle, id: String) -> CmdResult<String> {
    blocking(move || app.state::<PeerNode>().get()?.pair(&id).map_err(err)).await
}

#[tauri::command]
pub fn peer_answer(node: State<'_, PeerNode>, request: u64, accept: bool) -> CmdResult<()> {
    node.get()?.answer_pair(request, accept);
    Ok(())
}

#[tauri::command]
pub fn peer_unpair(node: State<'_, PeerNode>, id: String) -> CmdResult<()> {
    node.get()?.unpair(&id).map_err(err)
}

/// Check the connection to a paired Mac; milliseconds there and back.
#[tauri::command]
pub async fn peer_ping(app: AppHandle, id: String) -> CmdResult<u32> {
    blocking(move || {
        let d = app.state::<PeerNode>().get()?.ping(&id).map_err(err)?;
        Ok(d.as_millis().max(1) as u32)
    })
    .await
}
