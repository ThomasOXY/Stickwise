//! Sending playlists, sets and tracks to a paired Mac, AirDrop-style, and
//! taking them in on the other side.
//!
//! The sender offers ([`Offer`]); the receiving DJ is asked; the receiver
//! answers with the files it lacks ([`Want`]); the sender streams them; the
//! receiver checks each against its fingerprint, files it under the music
//! folder and takes the offer into its library.
//!
//! Music already on the receiving Mac is never overwritten: a new file
//! whose name is taken gets " (2)" instead.

use std::collections::HashMap;
use std::ffi::CString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use library::Library;
use library::transfer::{ApplyReport, Offer, Received, TrackData, Want};
use peerlink::Session;
use serde::{Deserialize, Serialize};

/// How long the sender waits for the other DJ to accept.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
/// Taking the offer into the library after the last file.
const APPLY_TIMEOUT: Duration = Duration::from_secs(300);
const CHUNK: usize = 256 * 1024;
/// Room left on the receiving disk beyond the music.
const SPARE: u64 = 512 * 1024 * 1024;
const STOPPED: &str = "Přenos zastaven.";

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
    /// Header of one track's files; `audio` bytes of audio, then each
    /// analysis file's bytes, follow raw.
    File { fingerprint: String, audio: u64, anlz: Vec<(String, u64)> },
    /// The sender could not read this track's file.
    Skip { fingerprint: String, reason: String },
    Finished,
    Applied { report: ApplyReport },
    Failed { message: String },
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

fn label(t: &TrackData) -> String {
    match (t.entry.artist.as_str(), t.entry.title.as_str()) {
        ("", title) => title.to_string(),
        (artist, title) => format!("{artist} – {title}"),
    }
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
        _ => return other("Druhý Mac odpověděl nesrozumitelně."),
    };
    session.set_timeout(Some(IO_TIMEOUT))?;
    let tracks: HashMap<&str, &TrackData> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t)).collect();

    // Where the files are here, and how much will go.
    let mut plan = Vec::new();
    for w in &want {
        let Some(t) = tracks.get(w.fingerprint.as_str()) else { continue };
        let local = {
            let l = lib.lock().unwrap();
            match l.track_by_fingerprint(&w.fingerprint)? {
                Some(id) => l.track(id)?,
                None => None,
            }
        };
        let Some(local) = local else {
            plan.push((w, *t, None, Vec::new()));
            continue;
        };
        let audio = w.audio.then(|| PathBuf::from(&local.path)).filter(|p| p.is_file());
        let anlz: Vec<(String, PathBuf)> = if w.anlz && !local.anlz.is_empty() {
            ["DAT", "EXT", "2EX"]
                .iter()
                .map(|e| (e.to_string(), Path::new(&local.anlz).with_extension(e)))
                .filter(|(_, p)| p.is_file())
                .collect()
        } else {
            Vec::new()
        };
        if w.audio && audio.is_none() {
            plan.push((w, *t, None, Vec::new()));
        } else {
            plan.push((w, *t, audio, anlz));
        }
    }
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let mut p = Progress {
        files_total: plan.len(),
        bytes_total: plan
            .iter()
            .map(|(_, _, a, z)| a.as_deref().map_or(0, size) + z.iter().map(|(_, f)| size(f)).sum::<u64>())
            .sum(),
        ..Default::default()
    };
    progress(&p);

    for (w, t, audio, anlz) in plan {
        p.current = label(t);
        if !progress(&p) {
            return other(STOPPED);
        }
        if w.audio && audio.is_none() {
            session.send(&Wire::Skip {
                fingerprint: w.fingerprint.clone(),
                reason: "soubor na odesílajícím Macu chybí".into(),
            })?;
            p.files_done += 1;
            continue;
        }
        let mut files: Vec<(std::fs::File, u64)> = Vec::new();
        let mut header_anlz = Vec::new();
        let audio_len = match &audio {
            Some(a) => {
                let f = std::fs::File::open(a)?;
                let n = f.metadata()?.len();
                files.push((f, n));
                n
            }
            None => 0,
        };
        for (ext, path) in &anlz {
            let f = std::fs::File::open(path)?;
            let n = f.metadata()?.len();
            header_anlz.push((ext.clone(), n));
            files.push((f, n));
        }
        session.send(&Wire::File { fingerprint: w.fingerprint.clone(), audio: audio_len, anlz: header_anlz })?;
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
    session.set_timeout(Some(APPLY_TIMEOUT))?;
    match session.recv::<Wire>()? {
        Wire::Applied { report } => Ok(Outcome::Sent(report)),
        Wire::Failed { message } => other(message),
        _ => other("Druhý Mac odpověděl nesrozumitelně."),
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
            return other("Soubor se během posílání zkrátil.");
        }
        w.write_all(&buf[..k])?;
        left -= k as u64;
        if !done(k as u64) {
            return other(STOPPED);
        }
    }
    Ok(())
}

// ---- Receiving ----------------------------------------------------------

/// The receiving app: its library, where things go, and the DJ.
pub trait Host: Send + Sync {
    fn library(&self) -> &Mutex<Library>;
    /// Where music from other Macs is filed.
    fn music_dir(&self) -> PathBuf;
    /// Where received analysis files go.
    fn anlz_dir(&self) -> PathBuf;
    /// Ask the DJ whether to take this from `from`; blocks until answered.
    fn ask(&self, from: &str, summary: &Summary) -> bool;
    /// Returning false stops receiving.
    fn progress(&self, from: &str, p: &Progress) -> bool;
    fn finished(&self, from: &str, result: &std::result::Result<ApplyReport, String>);
}

/// Serve a paired Mac that connected: pings, and an offer if it makes one.
pub fn serve(mut session: Session, host: &dyn Host) {
    loop {
        match session.recv::<Wire>() {
            Ok(Wire::Ping) => {
                if session.send(&Wire::Pong).is_err() {
                    return;
                }
            }
            Ok(Wire::Offer { offer }) => {
                let from = session.peer_name.clone();
                match receive(&mut session, host, &offer) {
                    Ok(Some(report)) => host.finished(&from, &Ok(report)),
                    Ok(None) => {}
                    Err(e) => {
                        let _ = session.send(&Wire::Failed { message: e.to_string() });
                        host.finished(&from, &Err(e.to_string()));
                    }
                }
                return;
            }
            _ => return,
        }
    }
}

pub fn summary(offer: &Offer, want: &[Want]) -> Summary {
    let sizes: HashMap<&str, u64> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t.entry.size)).collect();
    let new: Vec<&Want> = want.iter().filter(|w| w.audio).collect();
    Summary {
        playlists: offer.playlists.iter().filter(|p| !p.is_folder).map(|p| p.name.clone()).collect(),
        sets: offer.playlists.iter().filter(|p| p.is_set).count(),
        tracks: offer.tracks.len(),
        new_tracks: new.len(),
        new_bytes: new.iter().map(|w| sizes.get(w.fingerprint.as_str()).copied().unwrap_or(0)).sum(),
    }
}

/// `None` when the DJ said no.
fn receive(session: &mut Session, host: &dyn Host, offer: &Offer) -> Result<Option<ApplyReport>> {
    let from = session.peer_name.clone();
    let want = host.library().lock().unwrap().wants(offer)?;
    let sum = summary(offer, &want);
    let music = host.music_dir();
    if sum.new_tracks > 0 {
        std::fs::create_dir_all(&music)?;
        if let Some(free) = free_space(&music)
            && free < sum.new_bytes + SPARE
        {
            let reason = format!("Na disku s {} není dost místa.", music.display());
            session.send(&Wire::Declined { reason })?;
            return other(format!("Z {from} nepřišlo nic: na disku s hudbou není dost místa."));
        }
    }
    if !host.ask(&from, &sum) {
        session.send(&Wire::Declined { reason: "Druhý Mac to odmítl.".into() })?;
        return Ok(None);
    }
    session.send(&Wire::Want { want: want.clone() })?;
    session.set_timeout(Some(IO_TIMEOUT))?;

    let tracks: HashMap<&str, &TrackData> = offer.tracks.iter().map(|t| (t.entry.fingerprint.as_str(), t)).collect();
    let mut p = Progress { files_total: want.len(), bytes_total: sum.new_bytes, ..Default::default() };
    host.progress(&from, &p);
    let mut received: HashMap<String, Received> = HashMap::new();
    let mut failed: Vec<String> = Vec::new();
    loop {
        match session.recv::<Wire>()? {
            Wire::File { fingerprint, audio, anlz } => {
                let Some(t) = tracks.get(fingerprint.as_str()) else {
                    return other("Druhý Mac poslal skladbu, kterou nenabídl.");
                };
                p.current = label(t);
                host.progress(&from, &p);
                let mut got = Received::default();
                if audio > 0 {
                    match take_audio(session, t, audio, &music, |k| {
                        p.bytes_done += k;
                        host.progress(&from, &p)
                    })? {
                        Ok(a) => got.audio = Some(a),
                        Err(why) => failed.push(format!("{} ({why})", label(t))),
                    }
                }
                if !anlz.is_empty() {
                    got.anlz = Some(take_anlz(session, &host.anlz_dir(), &fingerprint, &anlz)?);
                }
                received.insert(fingerprint, got);
                p.files_done += 1;
                host.progress(&from, &p);
            }
            Wire::Skip { fingerprint, reason } => {
                if let Some(t) = tracks.get(fingerprint.as_str()) {
                    failed.push(format!("{} ({reason})", label(t)));
                }
                p.files_done += 1;
                host.progress(&from, &p);
            }
            Wire::Finished => break,
            _ => return other("Druhý Mac poslal nesrozumitelnou zprávu."),
        }
    }

    let mut report = host.library().lock().unwrap().apply_offer(offer, &received)?;
    // Tracks whose file failed are among the skipped; say why.
    for f in failed {
        let name = f.split(" (").next().unwrap_or_default().to_string();
        report.skipped.retain(|s| *s != name);
        report.skipped.push(f);
    }
    session.send(&Wire::Applied { report: report.clone() })?;
    Ok(Some(report))
}

/// Read one audio file off the session into the music folder. The outer
/// `Err` breaks the connection; the inner one only loses this track.
fn take_audio(
    session: &mut Session,
    t: &TrackData,
    n: u64,
    music: &Path,
    done: impl FnMut(u64) -> bool,
) -> Result<std::result::Result<(String, u64, i64), String>> {
    let dir = music.join(component(&t.entry.artist, "Neznámý interpret")).join(component(&t.entry.album, "Neznámé album"));
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!(".{}.stickwise-part", &t.entry.fingerprint[..16.min(t.entry.fingerprint.len())]));
    let mut hasher = blake3::Hasher::new();
    {
        let mut f = std::fs::File::create(&tmp)?;
        let mut w = HashingWriter { inner: &mut f, hasher: &mut hasher };
        let copied = copy_exact(Read::take(&mut *session, n), &mut w, n, done);
        if copied.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        copied?;
        f.sync_all()?;
    }
    if hasher.finalize().to_hex().as_str() != t.entry.fingerprint {
        let _ = std::fs::remove_file(&tmp);
        return Ok(Err("soubor dorazil poškozený nebo se mezitím změnil".into()));
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
    other(format!("Ve složce {} je moc souborů jménem {name}.", dir.display()))
}

/// Analysis files of one track into `dir/peer/<fingerprint>/ANLZ0000.*`.
fn take_anlz(session: &mut Session, dir: &Path, fingerprint: &str, parts: &[(String, u64)]) -> Result<String> {
    let dir = dir.join("peer").join(&fingerprint[..16.min(fingerprint.len())]);
    std::fs::create_dir_all(&dir)?;
    for (ext, n) in parts {
        let ext = match ext.as_str() {
            "DAT" | "EXT" | "2EX" => ext.as_str(),
            _ => return other("Druhý Mac poslal neznámý soubor analýzy."),
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
