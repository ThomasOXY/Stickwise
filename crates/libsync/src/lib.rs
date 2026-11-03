//! Sending playlists, sets and tracks to a paired Mac, AirDrop-style, and
//! taking them in on the other side.
//!
//! The sender offers ([`Offer`]); the receiving DJ is asked; the receiver
//! answers with the files it lacks ([`Want`]); the sender streams them; the
//! receiver checks each against its fingerprint, files it under the music
//! folder and takes the offer into its library.
//!
//! Mirroring ([`mirror`]) does it both ways at once for whole libraries:
//! each side offers everything, the DJ who started looks over what would
//! change on both Macs (and confirms playlists deleted on one of them),
//! then the files go one way, then the other.
//!
//! Music already on the receiving Mac is never overwritten: a new file
//! whose name is taken gets " (2)" instead.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use library::Library;
use library::peer::Tombstone;
use library::transfer::{ApplyReport, Deletion, Offer, Received, TrackData, Want};
use peerlink::Session;
use serde::{Deserialize, Serialize};

/// How long the sender waits for the other DJ to accept.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a mirror waits for the DJ to look over the changes.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// The other side fingerprinting and comparing whole libraries.
const PREPARE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
/// Taking the offer into the library after the last file.
const APPLY_TIMEOUT: Duration = Duration::from_secs(300);
const CHUNK: usize = 256 * 1024;
/// Room left on the receiving disk beyond the music.
const SPARE: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Peer(#[from] peerlink::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Library(#[from] library::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn other<T>(why: impl Into<String>) -> Result<T> {
    Err(Error::Other(why.into()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Wire {
    Ping,
    Pong,
    Offer { offer: Offer },
    Declined { reason: String },
    Want { want: Vec<Want> },
    /// Header of one track's files. Raw bytes follow: the audio file's
    /// (`audio` long) from byte `from` on, each analysis file's, then the
    /// artwork's (extension, length).
    File {
        fingerprint: String,
        audio: u64,
        #[serde(default)]
        from: u64,
        anlz: Vec<(String, u64)>,
        #[serde(default)]
        artwork: Option<(String, u64)>,
    },
    /// The sender could not read this track's file.
    Skip { fingerprint: String, reason: String },
    Finished,
    Applied { report: ApplyReport },
    Failed { message: String },
    /// Mirroring: the whole library of the side that started.
    MirrorOffer { offer: Offer },
    /// The other side's whole library, what it lacks of the first, and
    /// what taking it in would change there.
    MirrorState {
        offer: Offer,
        tombstones: Vec<Tombstone>,
        want: Vec<Want>,
        changes: Changes,
    },
    /// The DJ confirmed: what the starting side lacks, playlists to
    /// delete on the other side, and playlists of the offer it must not
    /// take (deleted on the starting side).
    MirrorGo { want: Vec<Want>, delete: Vec<String>, skip: Vec<String> },
    MirrorCancel,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The track being sent or received.
    pub current: String,
}

/// What the receiving DJ is asked about.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Names of the playlists and sets (not folders).
    pub playlists: Vec<String>,
    pub sets: usize,
    pub tracks: usize,
    /// Tracks this Mac does not have yet, and their size.
    pub new_tracks: usize,
    pub new_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Sent(ApplyReport),
    Declined(String),
}

/// What mirroring would do to one Mac's library.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub report: ApplyReport,
    /// Size of the music coming to this Mac.
    pub new_bytes: u64,
    /// Playlists that cannot go along: some of their tracks are missing.
    pub left_out: Vec<String>,
}

/// What the DJ looks over before mirroring.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MirrorPreview {
    pub here: Changes,
    pub there: Changes,
    /// Playlists here that the other Mac deleted.
    pub delete_here: Vec<Deletion>,
    /// Playlists the other Mac has that were deleted here.
    pub delete_there: Vec<Deletion>,
    /// The music coming here fits on the disk.
    pub room_here: bool,
}

/// The DJ's go: uids of the deletions to carry out on each side. A
/// deletion not chosen brings the playlist back to the Mac it was
/// deleted on.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MirrorChoice {
    pub delete_here: Vec<String>,
    pub delete_there: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MirrorOutcome {
    Done { here: ApplyReport, there: ApplyReport },
    Cancelled,
}

fn label(t: &TrackData) -> String {
    match (t.entry.artist.as_str(), t.entry.title.as_str()) {
        ("", title) => title.to_string(),
        (artist, title) => format!("{artist} – {title}"),
    }
}

// ---- Fingerprints -------------------------------------------------------

fn mtime(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// Fingerprint the tracks (of `ids`, or all) whose file is there but has
/// no fingerprint or an outdated one. `each(done, total)` before each
/// file; false stops. Unreadable files are passed over.
pub fn fingerprint(lib: &Mutex<Library>, ids: Option<&HashSet<i64>>, mut each: impl FnMut(usize, usize) -> bool) -> Result<()> {
    let stamps = lib.lock().unwrap().file_stamps()?;
    let todo: Vec<_> = stamps
        .into_iter()
        .filter(|s| ids.is_none_or(|ids| ids.contains(&s.id)))
        .filter_map(|s| {
            let m = std::fs::metadata(&s.path).ok()?;
            s.stale(m.len(), mtime(&m)).then_some((s, m.len(), mtime(&m)))
        })
        .collect();
    for (i, (s, size, mtime)) in todo.iter().enumerate() {
        if !each(i, todo.len()) {
            return other(i18n::t!("sync.stopped"));
        }
        let Ok(fp) = library::peer::fingerprint_file(Path::new(&s.path)) else { continue };
        lib.lock().unwrap().set_fingerprint(s.id, &fp, *size, *mtime)?;
    }
    Ok(())
}

// ---- Files both ways ----------------------------------------------------

/// Size of the music in `want` (what will come).
fn bytes_of(offer: &Offer, want: &[Want]) -> u64 {
    let sizes: HashMap<&str, u64> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t.entry.size)).collect();
    want.iter().filter(|w| w.audio).map(|w| sizes.get(w.fingerprint.as_str()).copied().unwrap_or(0)).sum()
}

/// One track's files to send.
struct Outgoing<'a> {
    want: &'a Want,
    track: &'a TrackData,
    audio: Option<PathBuf>,
    anlz: Vec<(String, PathBuf)>,
    artwork: Option<PathBuf>,
}

/// Stream the files of `want` (tracks of `offer` in this library) to the
/// other side, then [`Wire::Finished`].
fn push_files(
    session: &mut Session,
    lib: &Mutex<Library>,
    offer: &Offer,
    want: &[Want],
    progress: &mut dyn FnMut(&Progress) -> bool,
) -> Result<()> {
    let tracks: HashMap<&str, &TrackData> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t)).collect();

    // Where the files are here, and how much will go.
    let mut plan = Vec::new();
    for w in want {
        let Some(t) = tracks.get(w.fingerprint.as_str()) else { continue };
        let local = {
            let l = lib.lock().unwrap();
            match l.track_by_fingerprint(&w.fingerprint)? {
                Some(id) => l.track(id)?,
                None => None,
            }
        };
        let mut out = Outgoing { want: w, track: t, audio: None, anlz: Vec::new(), artwork: None };
        if let Some(local) = local {
            out.audio = w.audio.then(|| PathBuf::from(&local.path)).filter(|p| p.is_file());
            if w.anlz && !local.anlz.is_empty() {
                out.anlz = ["DAT", "EXT", "2EX"]
                    .iter()
                    .map(|e| (e.to_string(), Path::new(&local.anlz).with_extension(e)))
                    .filter(|(_, p)| p.is_file())
                    .collect();
            }
            out.artwork = w.artwork.then(|| PathBuf::from(&local.artwork)).filter(|p| !local.artwork.is_empty() && p.is_file());
        }
        plan.push(out);
    }
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let mut p = Progress {
        files_total: plan.len(),
        bytes_total: plan
            .iter()
            .map(|o| {
                o.audio.as_deref().map_or(0, |a| size(a).saturating_sub(o.want.resume_from))
                    + o.anlz.iter().map(|(_, f)| size(f)).sum::<u64>()
                    + o.artwork.as_deref().map_or(0, size)
            })
            .sum(),
        ..Default::default()
    };
    progress(&p);

    for o in plan {
        p.current = label(o.track);
        if !progress(&p) {
            return other(i18n::t!("sync.stopped"));
        }
        if o.want.audio && o.audio.is_none() {
            session.send(&Wire::Skip {
                fingerprint: o.want.fingerprint.clone(),
                reason: i18n::t!("sync.missingOnSender"),
            })?;
            p.files_done += 1;
            continue;
        }
        let mut files: Vec<(std::fs::File, u64)> = Vec::new();
        let (mut audio_len, mut from) = (0, 0);
        if let Some(a) = &o.audio {
            let mut f = std::fs::File::open(a)?;
            audio_len = f.metadata()?.len();
            // Resume only where the bytes before still fit.
            from = if o.want.resume_from < audio_len { o.want.resume_from } else { 0 };
            f.seek(SeekFrom::Start(from))?;
            files.push((f, audio_len - from));
        }
        let mut header_anlz = Vec::new();
        for (ext, path) in &o.anlz {
            let f = std::fs::File::open(path)?;
            let n = f.metadata()?.len();
            header_anlz.push((ext.clone(), n));
            files.push((f, n));
        }
        let mut artwork = None;
        if let Some(path) = &o.artwork {
            let f = std::fs::File::open(path)?;
            let n = f.metadata()?.len();
            artwork = Some((o.track.artwork.clone().unwrap_or_default(), n));
            files.push((f, n));
        }
        session.send(&Wire::File {
            fingerprint: o.want.fingerprint.clone(),
            audio: audio_len,
            from,
            anlz: header_anlz,
            artwork,
        })?;
        for (f, n) in files {
            copy_exact(f, session, n, |k| {
                p.bytes_done += k;
                progress(&p)
            })?;
        }
        session.flush()?;
        p.files_done += 1;
        progress(&p);
    }
    session.send(&Wire::Finished)?;
    Ok(())
}

/// What arrived (by fingerprint), and the tracks that did not with why.
type Pulled = (HashMap<String, Received>, Vec<(String, String)>);

/// Take the files of `want` (tracks of `offer`) the other side streams,
/// until [`Wire::Finished`]: music into `music`, analysis into `anlz_dir`.
/// Returns what arrived, and the tracks that did not with why.
fn pull_files(
    session: &mut Session,
    offer: &Offer,
    want: &[Want],
    music: &Path,
    anlz_dir: &Path,
    progress: &mut dyn FnMut(&Progress) -> bool,
) -> Result<Pulled> {
    let tracks: HashMap<&str, &TrackData> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t)).collect();
    let mut p = Progress { files_total: want.len(), bytes_total: bytes_of(offer, want), ..Default::default() };
    progress(&p);
    let mut received: HashMap<String, Received> = HashMap::new();
    let mut failed = Vec::new();
    loop {
        match session.recv::<Wire>()? {
            Wire::File { fingerprint, audio, from, anlz, artwork } => {
                let Some(t) = tracks.get(fingerprint.as_str()) else {
                    return other(i18n::t!("sync.unexpectedTrack"));
                };
                p.current = label(t);
                p.bytes_done += from.min(audio);
                if !progress(&p) {
                    return other(i18n::t!("sync.stopped"));
                }
                let mut got = Received::default();
                if audio > 0 {
                    match take_audio(session, t, audio, from, music, |k| {
                        p.bytes_done += k;
                        progress(&p)
                    })? {
                        Ok(a) => got.audio = Some(a),
                        Err(why) => failed.push((label(t), why)),
                    }
                }
                if !anlz.is_empty() {
                    got.anlz = Some(take_anlz(session, anlz_dir, &fingerprint, &anlz)?);
                }
                if let Some((ext, n)) = artwork {
                    got.artwork = Some(take_artwork(session, anlz_dir, &fingerprint, &ext, n)?);
                }
                received.insert(fingerprint, got);
                p.files_done += 1;
                progress(&p);
            }
            Wire::Skip { fingerprint, reason } => {
                if let Some(t) = tracks.get(fingerprint.as_str()) {
                    failed.push((label(t), reason));
                }
                p.files_done += 1;
                progress(&p);
            }
            Wire::Finished => return Ok((received, failed)),
            _ => return other(i18n::t!("sync.unexpectedMessage")),
        }
    }
}

/// Tracks whose file failed are among the skipped; say why.
fn explain(report: &mut ApplyReport, failed: Vec<(String, String)>) {
    for (name, why) in failed {
        match report.skipped.iter_mut().find(|s| **s == name) {
            Some(s) => *s = format!("{name} ({why})"),
            None => report.skipped.push(format!("{name} ({why})")),
        }
    }
}

fn enough_room(music: &Path, bytes: u64) -> Result<bool> {
    if bytes == 0 {
        return Ok(true);
    }
    std::fs::create_dir_all(music)?;
    Ok(free_space(music).is_none_or(|free| free >= bytes + SPARE))
}

// ---- Sending ------------------------------------------------------------

/// Offer to the Mac at the other end of `session` and send it what it
/// asks for. `progress` follows the files; returning false stops.
pub fn send(session: &mut Session, lib: &Mutex<Library>, offer: Offer, mut progress: impl FnMut(&Progress) -> bool) -> Result<Outcome> {
    session.set_timeout(Some(ANSWER_TIMEOUT + Duration::from_secs(30)))?;
    session.send(&Wire::Offer { offer: offer.clone() })?;
    let want = match session.recv::<Wire>()? {
        Wire::Want { want } => want,
        Wire::Declined { reason } => return Ok(Outcome::Declined(reason)),
        Wire::Failed { message } => return other(message),
        _ => return other(i18n::t!("sync.unclearAnswer")),
    };
    session.set_timeout(Some(IO_TIMEOUT))?;
    push_files(session, lib, &offer, &want, &mut progress)?;
    session.set_timeout(Some(APPLY_TIMEOUT))?;
    match session.recv::<Wire>()? {
        Wire::Applied { report } => Ok(Outcome::Sent(report)),
        Wire::Failed { message } => other(message),
        _ => other(i18n::t!("sync.unclearAnswer")),
    }
}

/// Exactly `n` bytes from `r` to `w`; a file that got shorter meanwhile
/// cannot be sent (the other side expects `n`). `done` after each chunk;
/// false stops.
fn copy_exact(mut r: impl Read, w: &mut impl Write, n: u64, mut done: impl FnMut(u64) -> bool) -> Result<()> {
    let mut buf = vec![0u8; CHUNK];
    let mut left = n;
    while left > 0 {
        let k = r.read(&mut buf[..CHUNK.min(left as usize)])?;
        if k == 0 {
            return other(i18n::t!("sync.fileShrank"));
        }
        w.write_all(&buf[..k])?;
        left -= k as u64;
        if !done(k as u64) {
            return other(i18n::t!("sync.stopped"));
        }
    }
    Ok(())
}

// ---- Receiving ----------------------------------------------------------

/// The app on this side: its library, where things go, and the DJ.
pub trait Host: Send + Sync {
    fn library(&self) -> &Mutex<Library>;
    /// Where music from other Macs is filed.
    fn music_dir(&self) -> PathBuf;
    /// Where received analysis files go.
    fn anlz_dir(&self) -> PathBuf;
    /// Ask the DJ whether to take this from `from`; blocks until answered.
    fn ask(&self, from: &str, summary: &Summary) -> bool;
    /// A transfer from `from` begins: an accepted offer, or a mirror the
    /// other Mac started.
    fn started(&self, from: &str, mirror: bool);
    /// Returning false stops.
    fn progress(&self, from: &str, p: &Progress) -> bool;
    fn finished(&self, from: &str, result: &std::result::Result<ApplyReport, String>);
    /// The other Mac called it off (a mirror its DJ did not confirm).
    fn cancelled(&self, from: &str);
}

/// Serve a paired Mac that connected: pings, and an offer or a mirror if
/// it starts one.
pub fn serve(mut session: Session, host: &dyn Host) {
    loop {
        let from = session.peer_name.clone();
        let result = match session.recv::<Wire>() {
            Ok(Wire::Ping) => {
                if session.send(&Wire::Pong).is_err() {
                    return;
                }
                continue;
            }
            Ok(Wire::Offer { offer }) => receive(&mut session, host, &offer),
            Ok(Wire::MirrorOffer { offer }) => mirror_respond(&mut session, host, offer),
            _ => return,
        };
        match result {
            Ok(Some(report)) => host.finished(&from, &Ok(report)),
            Ok(None) => {}
            Err(e) => {
                let _ = session.send(&Wire::Failed { message: e.to_string() });
                host.finished(&from, &Err(e.to_string()));
            }
        }
        return;
    }
}

pub fn summary(offer: &Offer, want: &[Want]) -> Summary {
    Summary {
        playlists: offer.playlists.iter().filter(|p| !p.is_folder).map(|p| p.name.clone()).collect(),
        sets: offer.playlists.iter().filter(|p| p.is_set).count(),
        tracks: offer.tracks.len(),
        new_tracks: want.iter().filter(|w| w.audio).count(),
        new_bytes: bytes_of(offer, want),
    }
}

/// `None` when the DJ said no.
fn receive(session: &mut Session, host: &dyn Host, offer: &Offer) -> Result<Option<ApplyReport>> {
    let from = session.peer_name.clone();
    let music = host.music_dir();
    let mut want = host.library().lock().unwrap().wants(offer)?;
    set_resume(&mut want, offer, &music);
    let sum = summary(offer, &want);
    if !enough_room(&music, sum.new_bytes)? {
        let reason = i18n::t!("sync.noRoomOnDisk", path = music.display());
        session.send(&Wire::Declined { reason })?;
        return other(i18n::t!("sync.nothingCameNoRoom", from = from));
    }
    if !host.ask(&from, &sum) {
        session.send(&Wire::Declined { reason: i18n::t!("sync.declined") })?;
        return Ok(None);
    }
    host.started(&from, false);
    session.send(&Wire::Want { want: want.clone() })?;
    session.set_timeout(Some(IO_TIMEOUT))?;
    let (received, failed) = pull_files(session, offer, &want, &music, &host.anlz_dir(), &mut |p| host.progress(&from, p))?;
    let mut report = host.library().lock().unwrap().apply_offer(offer, &received)?;
    explain(&mut report, failed);
    session.send(&Wire::Applied { report: report.clone() })?;
    Ok(Some(report))
}

// ---- Mirroring ----------------------------------------------------------

/// Fingerprint everything, then the whole library (with the playlists
/// left out) and its tombstones.
fn whole_library(host: &dyn Host, from: &str) -> Result<(Offer, Vec<String>, Vec<Tombstone>)> {
    let mut p = Progress { current: i18n::t!("sync.preparingLibrary"), ..Default::default() };
    fingerprint(host.library(), None, |done, total| {
        p.files_done = done;
        p.files_total = total;
        host.progress(from, &p)
    })?;
    let l = host.library().lock().unwrap();
    let (offer, left_out) = l.full_offer()?;
    Ok((offer, left_out, l.tombstones()?))
}

/// Make this library and the one at the other end of `session` the same,
/// both ways. `confirm` shows the DJ what would change and returns the go
/// (with the deletions chosen), or `None` to call it off. `host` gives the
/// library, folders and progress; its `ask`, `started` and `finished` are
/// not used here.
pub fn mirror(
    session: &mut Session,
    host: &dyn Host,
    confirm: impl FnOnce(&MirrorPreview) -> Option<MirrorChoice>,
) -> Result<MirrorOutcome> {
    let from = session.peer_name.clone();
    let lib = host.library();
    let (mine, mine_left, _) = whole_library(host, &from)?;
    session.set_timeout(Some(PREPARE_TIMEOUT))?;
    session.send(&Wire::MirrorOffer { offer: mine.clone() })?;
    let (theirs, their_tombstones, their_want, there) = match session.recv::<Wire>()? {
        Wire::MirrorState { offer, tombstones, want, changes } => (offer, tombstones, want, changes),
        Wire::Failed { message } => return other(message),
        _ => return other(i18n::t!("sync.unclearAnswer")),
    };

    let (want, preview) = {
        let mut l = lib.lock().unwrap();
        let mut want = l.wants(&theirs)?;
        set_resume(&mut want, &theirs, &host.music_dir());
        let new_bytes = bytes_of(&theirs, &want);
        let here = Changes { report: l.preview(&theirs, &want)?, new_bytes, left_out: mine_left };
        let preview = MirrorPreview {
            room_here: enough_room(&host.music_dir(), new_bytes)?,
            here,
            there,
            delete_here: l.deleted_there(&their_tombstones)?,
            delete_there: l.deleted_here(&theirs)?,
        };
        (want, preview)
    };
    let choice = if preview.room_here { confirm(&preview) } else { None };
    let Some(mut choice) = choice else {
        session.send(&Wire::MirrorCancel)?;
        return Ok(MirrorOutcome::Cancelled);
    };
    // Only what was offered for deletion.
    let offered = |list: &[Deletion], uid: &String| list.iter().any(|d| d.uid == *uid);
    choice.delete_here.retain(|u| offered(&preview.delete_here, u));
    choice.delete_there.retain(|u| offered(&preview.delete_there, u));

    session.send(&Wire::MirrorGo {
        want: want.clone(),
        delete: choice.delete_there.clone(),
        skip: choice.delete_here.clone(),
    })?;
    let mut progress = |p: &Progress| host.progress(&from, p);

    // Ours to them, then theirs to us.
    session.set_timeout(Some(IO_TIMEOUT))?;
    push_files(session, lib, &mine, &their_want, &mut progress)?;
    session.set_timeout(Some(APPLY_TIMEOUT))?;
    let there = match session.recv::<Wire>()? {
        Wire::Applied { report } => report,
        Wire::Failed { message } => return other(message),
        _ => return other(i18n::t!("sync.unclearAnswer")),
    };
    session.set_timeout(Some(IO_TIMEOUT))?;
    let (received, failed) = pull_files(session, &theirs, &want, &host.music_dir(), &host.anlz_dir(), &mut progress)?;
    let mut theirs = theirs;
    theirs.playlists.retain(|p| !choice.delete_there.contains(&p.uid));
    let here = {
        let mut l = lib.lock().unwrap();
        let mut r = l.apply_offer(&theirs, &received)?;
        r.playlists_deleted = l.delete_playlists_by_uid(&choice.delete_here)?;
        explain(&mut r, failed);
        r
    };
    session.send(&Wire::Applied { report: here.clone() })?;
    Ok(MirrorOutcome::Done { here, there })
}

/// The other side of [`mirror`]. `None` when its DJ called it off.
fn mirror_respond(session: &mut Session, host: &dyn Host, mut theirs: Offer) -> Result<Option<ApplyReport>> {
    let from = session.peer_name.clone();
    host.started(&from, true);
    let lib = host.library();
    let (mine, mine_left, tombstones) = whole_library(host, &from)?;
    let music = host.music_dir();
    let (want, changes) = {
        let mut l = lib.lock().unwrap();
        let mut want = l.wants(&theirs)?;
        set_resume(&mut want, &theirs, &music);
        let new_bytes = bytes_of(&theirs, &want);
        (want.clone(), Changes { report: l.preview(&theirs, &want)?, new_bytes, left_out: mine_left })
    };
    if !enough_room(&music, changes.new_bytes)? {
        return other(i18n::t!("sync.noRoomForMusic", path = music.display(), from = from));
    }
    session.send(&Wire::MirrorState { offer: mine.clone(), tombstones, want: want.clone(), changes })?;

    session.set_timeout(Some(CONFIRM_TIMEOUT + Duration::from_secs(60)))?;
    let (their_want, delete, skip) = match session.recv::<Wire>()? {
        Wire::MirrorGo { want, delete, skip } => (want, delete, skip),
        Wire::MirrorCancel => {
            host.cancelled(&from);
            return Ok(None);
        }
        _ => return other(i18n::t!("sync.unclearAnswer")),
    };
    let mut progress = |p: &Progress| host.progress(&from, p);

    // Theirs to us, then ours to them.
    session.set_timeout(Some(IO_TIMEOUT))?;
    let (received, failed) = pull_files(session, &theirs, &want, &music, &host.anlz_dir(), &mut progress)?;
    theirs.playlists.retain(|p| !skip.contains(&p.uid));
    let report = {
        let mut l = lib.lock().unwrap();
        let mut r = l.apply_offer(&theirs, &received)?;
        r.playlists_deleted = l.delete_playlists_by_uid(&delete)?;
        explain(&mut r, failed);
        r
    };
    session.send(&Wire::Applied { report: report.clone() })?;
    push_files(session, lib, &mine, &their_want, &mut progress)?;
    session.set_timeout(Some(APPLY_TIMEOUT))?;
    match session.recv::<Wire>()? {
        Wire::Applied { .. } => Ok(Some(report)),
        Wire::Failed { message } => other(message),
        _ => other(i18n::t!("sync.unclearAnswer")),
    }
}

/// Where a track's audio goes (its folder), and the hidden file it
/// arrives in. A transfer that breaks leaves that file for the next one to
/// go on from.
fn part_path(music: &Path, t: &TrackData) -> (PathBuf, PathBuf) {
    let dir = music.join(component(&t.entry.artist, "Neznámý interpret")).join(component(&t.entry.album, "Neznámé album"));
    let part = dir.join(format!(".{}.stickwise-part", &t.entry.fingerprint[..16.min(t.entry.fingerprint.len())]));
    (dir, part)
}

/// Ask for the rest only of audio files that arrived in part before.
fn set_resume(want: &mut [Want], offer: &Offer, music: &Path) {
    let tracks: HashMap<&str, &TrackData> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t)).collect();
    for w in want.iter_mut().filter(|w| w.audio) {
        if let Some(t) = tracks.get(w.fingerprint.as_str()) {
            w.resume_from = std::fs::metadata(part_path(music, t).1).map_or(0, |m| m.len());
        }
    }
}

/// Read one audio file (`n` bytes, those from `from` on) off the session
/// into the music folder. The outer `Err` breaks the connection; the inner
/// one only loses this track.
fn take_audio(
    session: &mut Session,
    t: &TrackData,
    n: u64,
    from: u64,
    music: &Path,
    done: impl FnMut(u64) -> bool,
) -> Result<std::result::Result<(String, u64, i64), String>> {
    let (dir, tmp) = part_path(music, t);
    std::fs::create_dir_all(&dir)?;
    let mut hasher = blake3::Hasher::new();
    let have = std::fs::metadata(&tmp).map_or(0, |m| m.len());
    let mut f = if from == 0 {
        std::fs::File::create(&tmp)?
    } else if have == from && from <= n {
        std::io::copy(&mut std::fs::File::open(&tmp)?, &mut hasher)?;
        std::fs::OpenOptions::new().append(true).open(&tmp)?
    } else {
        // Not the part the sender went on from: take the bytes, drop them.
        std::io::copy(&mut Read::take(&mut *session, n.saturating_sub(from)), &mut std::io::sink())?;
        let _ = std::fs::remove_file(&tmp);
        return Ok(Err(i18n::t!("sync.resumeFailed")));
    };
    {
        let mut w = HashingWriter { inner: &mut f, hasher: &mut hasher };
        // A break leaves the part for the next transfer.
        copy_exact(Read::take(&mut *session, n - from), &mut w, n - from, done)?;
        f.sync_all()?;
    }
    if hasher.finalize().to_hex().as_str() != t.entry.fingerprint {
        let _ = std::fs::remove_file(&tmp);
        return Ok(Err(i18n::t!("sync.fileCorrupted")));
    }
    let name = component(&t.file_name, &format!("{}.audio", &t.entry.fingerprint[..16]));
    let dest = place(&tmp, &dir, &name)?;
    let m = std::fs::metadata(&dest)?;
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    Ok(Ok((dest.display().to_string(), m.len(), mtime)))
}

/// Move `tmp` to `dir/name`, or `name (2)` and on if taken; never over a
/// file that is there.
fn place(tmp: &Path, dir: &Path, name: &str) -> Result<PathBuf> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for i in 1..1000 {
        let candidate = if i == 1 { dir.join(name) } else { dir.join(format!("{stem} ({i}){ext}")) };
        // A hard link fails if the name is taken, so nothing is replaced.
        match std::fs::hard_link(tmp, &candidate) {
            Ok(()) => {
                std::fs::remove_file(tmp)?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                let _ = std::fs::remove_file(tmp);
                return Err(e.into());
            }
        }
    }
    let _ = std::fs::remove_file(tmp);
    other(i18n::t!("sync.tooManyFiles", folder = dir.display(), name = name))
}

/// Analysis files of one track into `dir/peer/<fingerprint>/ANLZ0000.*`.
fn take_anlz(session: &mut Session, dir: &Path, fingerprint: &str, parts: &[(String, u64)]) -> Result<String> {
    let dir = dir.join("peer").join(&fingerprint[..16.min(fingerprint.len())]);
    std::fs::create_dir_all(&dir)?;
    for (ext, n) in parts {
        let ext = match ext.as_str() {
            "DAT" | "EXT" | "2EX" => ext.as_str(),
            _ => return other(i18n::t!("sync.unknownAnalysisFile")),
        };
        let path = dir.join(format!("ANLZ0000.{ext}"));
        let tmp = path.with_extension(format!("{ext}.part"));
        {
            let mut f = std::fs::File::create(&tmp)?;
            copy_exact(Read::take(&mut *session, *n), &mut f, *n, |_| true)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
    }
    Ok(dir.join("ANLZ0000.DAT").display().to_string())
}

/// A track's artwork into `dir/peer/<fingerprint>/artwork.<ext>`.
fn take_artwork(session: &mut Session, dir: &Path, fingerprint: &str, ext: &str, n: u64) -> Result<String> {
    let dir = dir.join("peer").join(&fingerprint[..16.min(fingerprint.len())]);
    std::fs::create_dir_all(&dir)?;
    let ext = if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) { ext } else { "jpg" };
    let path = dir.join(format!("artwork.{ext}"));
    let tmp = dir.join(format!("artwork.{ext}.part"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        copy_exact(Read::take(&mut *session, n), &mut f, n, |_| true)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path.display().to_string())
}

struct HashingWriter<'a, W: Write> {
    inner: &'a mut W,
    hasher: &'a mut blake3::Hasher,
}

impl<W: Write> Write for HashingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A safe folder or file name from a tag.
fn component(s: &str, empty: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| if matches!(c, '/' | ':' | '\0') || c.is_control() { '-' } else { c })
        .collect();
    let clean = clean.trim().trim_start_matches('.').trim();
    let clean: String = clean.chars().take(120).collect();
    if clean.is_empty() { empty.to_string() } else { clean }
}

/// Free bytes on the disk holding `dir`.
fn free_space(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize)
}

#[cfg(test)]
mod tests;
