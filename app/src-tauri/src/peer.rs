//! Sync with Stickwise on other Macs: its settings, the other Macs on the
//! network and pairing with them, and fingerprinting the library's tracks
//! in the background so the other side can tell which tracks both have,
//! wherever their files lie.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, UNIX_EPOCH};

use libsync::{Host, MirrorChoice, MirrorOutcome, MirrorPreview, Outcome, Progress, Summary};
use library::transfer::ApplyReport;
use peerlink::{Event, Node, Peer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::{CmdResult, blocking, err};
use crate::dto::JobEvent;
use crate::jobs::Jobs;
use crate::library_cmds::{LibState, analysis_dir, emit_job, lib};

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

fn music_dir(app: &AppHandle) -> PathBuf {
    lib(app, |l| l.meta(MUSIC_DIR_KEY))
        .ok()
        .flatten()
        .map(PathBuf::from)
        .unwrap_or_else(default_music_dir)
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
    let serving = app.clone();
    let on_session: peerlink::OnSession = Arc::new(move |s| {
        libsync::serve(s, &AppHost::new(&serving, None, String::new()));
    });
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
            on_session: Some(on_session),
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

// ---- Sending and receiving ----------------------------------------------

/// The offer from another Mac waiting for the DJ's answer (one at a time).
#[derive(Default)]
pub struct OfferAsk {
    pending: Mutex<Option<(u64, mpsc::Sender<bool>)>>,
    next: AtomicU64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OfferDto {
    request: u64,
    from: String,
    summary: Summary,
}

fn plural(n: usize, one: &str, few: &str, many: &str) -> String {
    let w = match n {
        1 => one,
        2..=4 => few,
        _ => many,
    };
    format!("{n} {w}")
}

/// "12 nových skladeb, 2 playlisty, cue u 3 skladeb" — what changed.
fn report_text(r: &ApplyReport) -> String {
    let mut parts = Vec::new();
    if r.tracks_added > 0 {
        parts.push(plural(r.tracks_added, "nová skladba", "nové skladby", "nových skladeb"));
    }
    let lists = r.playlists_added + r.playlists_updated;
    if lists > 0 {
        parts.push(plural(lists, "playlist", "playlisty", "playlistů"));
    }
    if r.cues_updated > 0 {
        parts.push(format!("nové cue u {}", plural(r.cues_updated, "skladby", "skladeb", "skladeb")));
    }
    if r.playlists_deleted > 0 {
        parts.push(format!("smazáno: {}", plural(r.playlists_deleted, "playlist", "playlisty", "playlistů")));
    }
    let mut text = if parts.is_empty() { "všechno už tu bylo".to_string() } else { parts.join(", ") };
    if !r.skipped.is_empty() {
        text.push_str(&format!(". Nepřišlo: {}", r.skipped.join("; ")));
    }
    text
}

/// This app as a side of [`libsync`]: one per connection.
struct AppHost {
    app: AppHandle,
    /// The job showing this transfer, with its cancel flag.
    job: Mutex<Option<(u32, Arc<AtomicBool>)>>,
    label: Mutex<String>,
}

impl AppHost {
    fn new(app: &AppHandle, job: Option<(u32, Arc<AtomicBool>)>, label: String) -> Self {
        AppHost { app: app.clone(), job: Mutex::new(job), label: Mutex::new(label) }
    }

    fn event(&self, state: &'static str, p: &Progress, message: String) -> Option<(JobEvent, Arc<AtomicBool>)> {
        let (id, cancel) = self.job.lock().unwrap().clone()?;
        let ev = JobEvent {
            id,
            label: self.label.lock().unwrap().clone(),
            state,
            files_done: p.files_done,
            files_total: p.files_total,
            bytes_done: p.bytes_done,
            bytes_total: p.bytes_total.max(1),
            current: p.current.clone(),
            message,
            dest: music_dir(&self.app).display().to_string(),
            ..Default::default()
        };
        Some((ev, cancel))
    }
}

impl Host for AppHost {
    fn library(&self) -> &Mutex<library::Library> {
        &self.app.state::<LibState>().inner().0
    }

    fn music_dir(&self) -> PathBuf {
        music_dir(&self.app)
    }

    fn anlz_dir(&self) -> PathBuf {
        analysis_dir(&self.app).unwrap_or_else(|_| default_music_dir().join(".analysis"))
    }

    fn ask(&self, from: &str, summary: &Summary) -> bool {
        let asks = self.app.state::<OfferAsk>();
        let request = asks.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = asks.pending.lock().unwrap();
            if pending.is_some() {
                return false;
            }
            *pending = Some((request, tx));
        }
        let _ = self.app.emit("peer-offer", OfferDto { request, from: from.to_string(), summary: summary.clone() });
        let answer = rx.recv_timeout(libsync::ANSWER_TIMEOUT - Duration::from_secs(5)).ok();
        *asks.pending.lock().unwrap() = None;
        if answer.is_none() {
            let _ = self.app.emit("peer-offer-gone", request);
        }
        answer == Some(true)
    }

    fn started(&self, from: &str, mirror: bool) {
        *self.label.lock().unwrap() = if mirror { format!("Zrcadlení s {from}") } else { format!("Příjem z {from}") };
        let mut job = self.job.lock().unwrap();
        if job.is_none() {
            *job = Some(self.app.state::<Jobs>().start());
        }
    }

    fn cancelled(&self, from: &str) {
        if let Some((ev, _)) = self.event("cancelled", &Progress::default(), format!("{from} zrcadlení nepotvrdil.")) {
            emit_job(&self.app, &ev);
            self.app.state::<Jobs>().finish(ev.id);
        }
    }

    fn progress(&self, _: &str, p: &Progress) -> bool {
        let Some((ev, cancel)) = self.event("running", p, String::new()) else { return true };
        emit_job(&self.app, &ev);
        !cancel.load(Ordering::Relaxed)
    }

    fn finished(&self, from: &str, result: &Result<ApplyReport, String>) {
        let p = Progress::default();
        let (state, message) = match result {
            Ok(r) => ("done", format!("Z {from}: {}", report_text(r))),
            Err(e) => ("error", format!("Příjem z {from} se nepovedl: {e}")),
        };
        if let Some((mut ev, _)) = self.event(state, &p, message) {
            ev.files_done = ev.files_total;
            emit_job(&self.app, &ev);
            self.app.state::<Jobs>().finish(ev.id);
        }
        if result.is_ok() {
            let _ = self.app.emit("library-changed", ());
        }
    }
}

/// The DJ's answer to an offer from another Mac.
#[tauri::command]
pub fn peer_answer_offer(asks: State<'_, OfferAsk>, request: u64, accept: bool) {
    if let Some((r, tx)) = asks.pending.lock().unwrap().as_ref()
        && *r == request
    {
        let _ = tx.send(accept);
    }
}

/// Send playlists / sets (folders whole) and tracks to a paired Mac, as a
/// background job; returns the job id.
#[tauri::command]
pub fn peer_send(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    id: String,
    playlist_ids: Vec<i64>,
    track_ids: Vec<i64>,
) -> CmdResult<u32> {
    let node = app.state::<PeerNode>();
    let name = node.get()?.peers().into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_else(|| "Mac".into());
    let (job, cancel) = jobs.start();
    std::thread::spawn(move || {
        let mut ev = JobEvent {
            id: job,
            label: format!("Posílání na {name}"),
            state: "running",
            current: format!("Čekám, až to {name} přijme…"),
            bytes_total: 1,
            ..Default::default()
        };
        emit_job(&app, &ev);
        let result = (|| -> CmdResult<Outcome> {
            let (_, tracks) = lib(&app, |l| l.selection(&playlist_ids, &track_ids))?;
            let state = app.state::<LibState>();
            libsync::fingerprint(&state.0, Some(&tracks.into_iter().collect()), |_, _| !cancel.load(Ordering::Relaxed))
                .map_err(err)?;
            let offer = lib(&app, |l| l.offer(&playlist_ids, &track_ids))?;
            let mut session = app.state::<PeerNode>().get()?.connect(&id).map_err(err)?;
            libsync::send(&mut session, &state.0, offer, |p| {
                let mut e = ev.clone();
                e.files_done = p.files_done;
                e.files_total = p.files_total;
                e.bytes_done = p.bytes_done;
                e.bytes_total = p.bytes_total.max(1);
                e.current = p.current.clone();
                emit_job(&app, &e);
                !cancel.load(Ordering::Relaxed)
            })
            .map_err(err)
        })();
        ev.current.clear();
        ev.bytes_done = ev.bytes_total;
        (ev.state, ev.message) = match result {
            Ok(Outcome::Sent(r)) => ("done", format!("Na {name}: {}", report_text(&r))),
            Ok(Outcome::Declined(why)) => ("cancelled", why),
            Err(_) if cancel.load(Ordering::Relaxed) => ("cancelled", "Posílání zastaveno.".into()),
            Err(e) => ("error", format!("Poslat na {name} se nepovedlo: {e}")),
        };
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(job);
    });
    Ok(job)
}

/// The mirror preview waiting for the DJ (one at a time).
#[derive(Default)]
pub struct MirrorAsk {
    pending: Mutex<Option<(u64, mpsc::Sender<Option<MirrorChoice>>)>>,
    next: AtomicU64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MirrorPreviewDto {
    request: u64,
    peer: String,
    preview: MirrorPreview,
}

/// Mirror the library with a paired Mac, as a background job; the DJ
/// confirms the changes in between. Returns the job id.
#[tauri::command]
pub fn peer_mirror(app: AppHandle, jobs: State<'_, Jobs>, id: String) -> CmdResult<u32> {
    let node = app.state::<PeerNode>();
    let name = node.get()?.peers().into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_else(|| "Mac".into());
    if app.state::<MirrorAsk>().pending.lock().unwrap().is_some() {
        return Err("Jedno zrcadlení už čeká na potvrzení.".into());
    }
    let (job, cancel) = jobs.start();
    std::thread::spawn(move || {
        let label = format!("Zrcadlení s {name}");
        let host = AppHost::new(&app, Some((job, cancel.clone())), label.clone());
        let mut ev = JobEvent { id: job, label, state: "running", bytes_total: 1, ..Default::default() };
        ev.current = "Porovnávám knihovny…".into();
        emit_job(&app, &ev);
        let result = (|| -> CmdResult<MirrorOutcome> {
            let mut session = app.state::<PeerNode>().get()?.connect(&id).map_err(err)?;
            libsync::mirror(&mut session, &host, |preview| {
                let mut wait = ev.clone();
                wait.current = "Čekám na potvrzení změn…".into();
                emit_job(&app, &wait);
                let asks = app.state::<MirrorAsk>();
                let request = asks.next.fetch_add(1, Ordering::Relaxed) + 1;
                let (tx, rx) = mpsc::channel();
                *asks.pending.lock().unwrap() = Some((request, tx));
                let _ = app.emit("peer-mirror-preview", MirrorPreviewDto { request, peer: name.clone(), preview: preview.clone() });
                let answer = rx.recv_timeout(libsync::CONFIRM_TIMEOUT).ok().flatten();
                *asks.pending.lock().unwrap() = None;
                let _ = app.emit("peer-mirror-gone", request);
                answer
            })
            .map_err(err)
        })();
        ev.current.clear();
        ev.bytes_done = ev.bytes_total;
        (ev.state, ev.message) = match &result {
            Ok(MirrorOutcome::Done { here, there }) => {
                ("done", format!("Tady: {}. Na {name}: {}.", report_text(here), report_text(there)))
            }
            Ok(MirrorOutcome::Cancelled) => ("cancelled", "Zrcadlení zrušeno, nic se nezměnilo.".into()),
            Err(_) if cancel.load(Ordering::Relaxed) => ("cancelled", "Zrcadlení zastaveno.".into()),
            Err(e) => ("error", format!("Zrcadlení s {name} se nepovedlo: {e}")),
        };
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(job);
        if matches!(result, Ok(MirrorOutcome::Done { .. })) {
            let _ = app.emit("library-changed", ());
        }
    });
    Ok(job)
}

/// The DJ's go for a mirror (`None` calls it off).
#[tauri::command]
pub fn peer_mirror_answer(asks: State<'_, MirrorAsk>, request: u64, choice: Option<MirrorChoice>) {
    if let Some((r, tx)) = asks.pending.lock().unwrap().as_ref()
        && *r == request
    {
        let _ = tx.send(choice);
    }
}
