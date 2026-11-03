//! Sync with Stickwise on other Macs: its settings, the other Macs on the
//! network and pairing with them, and fingerprinting the library's tracks
//! in the background so the other side can tell which tracks both have,
//! wherever their files lie.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, UNIX_EPOCH};

use libsync::{Host, MirrorChoice, MirrorOutcome, MirrorPreview, Outcome, Progress, Summary};
use library::peer::Unreadable;
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
    /// Tracks without a fingerprint whose file can't be read.
    pub skipped: usize,
    /// Tracks without a fingerprint whose file is not there.
    pub waiting: usize,
    /// Mirror with paired Macs by itself when they come on the network.
    pub auto_mirror: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FingerprintProgress {
    fingerprinted: usize,
    total: usize,
    skipped: usize,
    waiting: usize,
}

/// Of the tracks without a fingerprint, as the last pass found them: files
/// that could not be read and are skipped, and files not there (a drive not
/// plugged in).
#[derive(Default)]
pub struct FingerprintGaps {
    skipped: AtomicUsize,
    waiting: AtomicUsize,
}

impl FingerprintGaps {
    fn get(&self) -> (usize, usize) {
        (self.skipped.load(Ordering::Relaxed), self.waiting.load(Ordering::Relaxed))
    }
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
        skipped: app.state::<FingerprintGaps>().get().0,
        waiting: app.state::<FingerprintGaps>().get().1,
        auto_mirror: auto_mirror_on(app),
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
                return Err(i18n::t!("app.peer.folderMissing", path = p));
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
            let mut unreadable = Unreadable::default();
            loop {
                fingerprint_pass(&app, all, &mut unreadable);
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
/// in) wait for a later pass; files that can't be read are skipped (see
/// [`Unreadable`]).
fn fingerprint_pass(app: &AppHandle, all: bool, unreadable: &mut Unreadable) {
    let Ok(stamps) = lib(app, |l| l.file_stamps()) else { return };
    let mut last_emit = Instant::now();
    let mut changed = false;
    let (mut skipped, mut waiting) = (0, 0);
    for s in stamps.iter().filter(|s| all || s.fingerprint.is_empty()) {
        let new = s.fingerprint.is_empty() as usize;
        let m = match std::fs::metadata(&s.path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                waiting += new;
                continue;
            }
            Err(e) => {
                if !unreadable.skipped(s.id, 0, 0) {
                    unreadable.failed(s.id, 0, 0, &e);
                }
                skipped += new * unreadable.skipped(s.id, 0, 0) as usize;
                continue;
            }
        };
        let (size, mt) = (m.len(), mtime(&m));
        if !s.stale(size, mt) {
            continue;
        }
        if unreadable.skipped(s.id, size, mt) {
            skipped += new;
            continue;
        }
        // Hashing reads the whole file: done without holding the library.
        let fp = match library::peer::fingerprint_file(Path::new(&s.path)) {
            Ok(fp) => fp,
            Err(e) => {
                unreadable.failed(s.id, size, mt, &e);
                skipped += new * unreadable.skipped(s.id, size, mt) as usize;
                continue;
            }
        };
        unreadable.read(s.id);
        // Changed while being read: the next pass gets it right.
        let Ok(after) = std::fs::metadata(&s.path) else { continue };
        if after.len() != size || mtime(&after) != mt {
            continue;
        }
        if lib(app, |l| l.set_fingerprint(s.id, &fp, size, mt)).is_err() {
            continue;
        }
        changed = true;
        if last_emit.elapsed() > Duration::from_millis(500) {
            emit_progress(app);
            last_emit = Instant::now();
        }
    }
    let gaps = app.state::<FingerprintGaps>();
    if changed || gaps.get() != (skipped, waiting) {
        gaps.skipped.store(skipped, Ordering::Relaxed);
        gaps.waiting.store(waiting, Ordering::Relaxed);
        emit_progress(app);
    }
}

fn emit_progress(app: &AppHandle) {
    if let Ok((fingerprinted, total)) = lib(app, |l| l.fingerprint_progress()) {
        let (skipped, waiting) = app.state::<FingerprintGaps>().get();
        let _ = app.emit("peer-fingerprints", FingerprintProgress { fingerprinted, total, skipped, waiting });
    }
}

// ---- Other Macs ---------------------------------------------------------

/// The node talking to other Macs; `Err` says why it could not start.
pub struct PeerNode(pub Result<Node, String>);

impl PeerNode {
    fn get(&self) -> CmdResult<&Node> {
        self.0.as_ref().map_err(|e| i18n::t!("app.peer.notRunning", error = e))
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
            Event::PeersChanged => {
                auto_mirror_check(&handle);
                handle.emit("peer-peers", ())
            }
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

/// "12 nových skladeb, 2 playlisty, cue u 3 skladeb" — what changed.
fn report_text(r: &ApplyReport) -> String {
    let mut parts = Vec::new();
    if r.tracks_added > 0 {
        parts.push(i18n::t!("app.peer.newTracks", count = r.tracks_added));
    }
    let lists = r.playlists_added + r.playlists_updated;
    if lists > 0 {
        parts.push(i18n::t!("app.peer.playlists", count = lists));
    }
    if r.cues_updated > 0 {
        parts.push(i18n::t!("app.peer.newCues", count = r.cues_updated));
    }
    if r.playlists_deleted > 0 {
        parts.push(i18n::t!("app.peer.deleted", count = r.playlists_deleted));
    }
    let mut text = if parts.is_empty() { i18n::t!("app.peer.nothingNew") } else { parts.join(", ") };
    if !r.skipped.is_empty() {
        text.push_str(". ");
        text.push_str(&i18n::t!("app.peer.notArrived", tracks = r.skipped.join("; ")));
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
        *self.label.lock().unwrap() = if mirror {
            i18n::t!("app.peer.mirrorLabel", name = from)
        } else {
            i18n::t!("app.peer.receiveLabel", name = from)
        };
        // The other Mac started a mirror: this one need not start its own.
        if mirror
            && let Ok(n) = self.app.state::<PeerNode>().get()
            && let Some(p) = n.peers().into_iter().find(|p| p.name == from)
        {
            self.app.state::<AutoMirror>().last.lock().unwrap().insert(p.id, Instant::now());
        }
        let mut job = self.job.lock().unwrap();
        if job.is_none() {
            *job = Some(self.app.state::<Jobs>().start());
        }
    }

    fn cancelled(&self, from: &str) {
        if let Some((ev, _)) = self.event("cancelled", &Progress::default(), i18n::t!("app.peer.mirrorNotConfirmed", name = from)) {
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
            Ok(r) => ("done", i18n::t!("app.peer.received", name = from, report = report_text(r))),
            Err(e) => ("error", i18n::t!("app.peer.receiveFailed", name = from, error = e)),
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
            label: i18n::t!("app.peer.sendLabel", name = name),
            state: "running",
            current: i18n::t!("app.peer.waitingAccept", name = name),
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
            Ok(Outcome::Sent(r)) => ("done", i18n::t!("app.peer.sent", name = name, report = report_text(&r))),
            Ok(Outcome::Declined(why)) => ("cancelled", why),
            Err(_) if cancel.load(Ordering::Relaxed) => ("cancelled", i18n::t!("app.peer.sendStopped")),
            Err(e) => ("error", i18n::t!("app.peer.sendFailed", name = name, error = e)),
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
pub fn peer_mirror(app: AppHandle, id: String) -> CmdResult<u32> {
    if app.state::<MirrorAsk>().pending.lock().unwrap().is_some() {
        return Err(i18n::t!("app.peer.mirrorPending"));
    }
    start_mirror(&app, id, false)
}

/// Mirror with `id` in the background. `auto`: started because the Mac
/// came on the network; it goes ahead without asking when nothing would
/// be deleted, else the DJ sees the preview as usual.
fn start_mirror(app: &AppHandle, id: String, auto: bool) -> CmdResult<u32> {
    let node = app.state::<PeerNode>();
    let name = node.get()?.peers().into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_else(|| "Mac".into());
    let auto_state = app.state::<AutoMirror>();
    if !auto_state.running.lock().unwrap().insert(id.clone()) {
        return Err(i18n::t!("app.peer.mirrorRunning", name = name));
    }
    auto_state.last.lock().unwrap().insert(id.clone(), Instant::now());
    let (job, cancel) = app.state::<Jobs>().start();
    let app = app.clone();
    std::thread::spawn(move || {
        let label = if auto {
            i18n::t!("app.peer.autoMirrorLabel", name = name)
        } else {
            i18n::t!("app.peer.mirrorLabel", name = name)
        };
        let host = AppHost::new(&app, Some((job, cancel.clone())), label.clone());
        let mut ev = JobEvent { id: job, label, state: "running", bytes_total: 1, ..Default::default() };
        ev.current = i18n::t!("app.peer.comparing");
        emit_job(&app, &ev);
        let result = (|| -> CmdResult<MirrorOutcome> {
            let mut session = app.state::<PeerNode>().get()?.connect(&id).map_err(err)?;
            libsync::mirror(&mut session, &host, |preview| {
                let quiet = preview.delete_here.is_empty() && preview.delete_there.is_empty() && preview.room_here;
                if auto && quiet {
                    return Some(MirrorChoice::default());
                }
                let asks = app.state::<MirrorAsk>();
                let request = asks.next.fetch_add(1, Ordering::Relaxed) + 1;
                let (tx, rx) = mpsc::channel();
                {
                    let mut pending = asks.pending.lock().unwrap();
                    if pending.is_some() {
                        return None;
                    }
                    *pending = Some((request, tx));
                }
                let mut wait = ev.clone();
                wait.current = i18n::t!("app.peer.waitingConfirm");
                emit_job(&app, &wait);
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
                ("done", i18n::t!("app.peer.mirrored", here = report_text(here), name = name, there = report_text(there)))
            }
            Ok(MirrorOutcome::Cancelled) => ("cancelled", i18n::t!("app.peer.mirrorCancelled")),
            Err(_) if cancel.load(Ordering::Relaxed) => ("cancelled", i18n::t!("app.peer.mirrorStopped")),
            Err(e) => ("error", i18n::t!("app.peer.mirrorFailed", name = name, error = e)),
        };
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(job);
        app.state::<AutoMirror>().running.lock().unwrap().remove(&id);
        if matches!(result, Ok(MirrorOutcome::Done { .. })) {
            let _ = app.emit("library-changed", ());
        }
    });
    Ok(job)
}

// ---- Mirroring by itself ------------------------------------------------

const AUTO_MIRROR_KEY: &str = "peer.auto_mirror";
/// Not again with the same Mac sooner than this.
const AUTO_EVERY: Duration = Duration::from_secs(10 * 60);
/// The Mac with the higher id waits this long for the other to start.
const AUTO_DEFER: Duration = Duration::from_secs(45);

/// Mirrors running, the last one with each Mac, and the paired Macs seen
/// on the network (to tell when one comes).
#[derive(Default)]
pub struct AutoMirror {
    running: Mutex<std::collections::HashSet<String>>,
    last: Mutex<std::collections::HashMap<String, Instant>>,
    online: Mutex<std::collections::HashSet<String>>,
}

fn auto_mirror_on(app: &AppHandle) -> bool {
    lib(app, |l| l.meta(AUTO_MIRROR_KEY)).ok().flatten().as_deref() == Some("1")
}

/// A paired Mac came on the network: mirror with it if the DJ wants that.
/// Of the two Macs the one with the lower id starts; the other only if no
/// mirror happened meanwhile (its DJ may have this off).
fn auto_mirror_check(app: &AppHandle) {
    let Some(node) = app.try_state::<PeerNode>() else { return };
    let Ok(n) = node.get() else { return };
    let Some(state) = app.try_state::<AutoMirror>() else { return };
    let now: std::collections::HashSet<String> =
        n.peers().into_iter().filter(|p| p.paired && p.online).map(|p| p.id).collect();
    let came: Vec<String> = {
        let mut seen = state.online.lock().unwrap();
        let came = now.difference(&seen).cloned().collect();
        *seen = now;
        came
    };
    if !auto_mirror_on(app) {
        return;
    }
    for id in came {
        let wait = if n.id() < id.as_str() { Duration::ZERO } else { AUTO_DEFER };
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            let recent = app.state::<AutoMirror>().last.lock().unwrap().get(&id).is_some_and(|t| t.elapsed() < AUTO_EVERY);
            let waiting = app.state::<MirrorAsk>().pending.lock().unwrap().is_some();
            if !recent && !waiting {
                let _ = start_mirror(&app, id, true);
            }
        });
    }
}

#[tauri::command]
pub fn peer_set_auto_mirror(app: AppHandle, on: bool) -> CmdResult<PeerSettings> {
    if on {
        lib(&app, |l| l.set_meta(AUTO_MIRROR_KEY, "1"))?;
        // Macs on the network now count as just come.
        app.state::<AutoMirror>().online.lock().unwrap().clear();
        auto_mirror_check(&app);
    } else {
        lib(&app, |l| l.clear_meta(AUTO_MIRROR_KEY))?;
    }
    settings(&app)
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
