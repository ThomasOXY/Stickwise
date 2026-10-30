//! Sync a selection of the library (playlists, folders, sets) onto a stick.
//!
//! Safe by construction:
//! 1. the stick's current database files are backed up to the Mac;
//! 2. audio is copied only where missing or changed (files a previous
//!    rekordbox export left on the stick are recognised and reused), each
//!    through a temporary file, flushed, renamed and size-checked;
//! 3. analysis files are written into new folders the current library does
//!    not use;
//! 4. stale `exportExt.pdb` / `exportLibrary.db` are moved aside;
//! 5. **`export.pdb` is replaced last, atomically** — until that moment the
//!    stick still holds its previous, complete library, so pulling it out
//!    at any point leaves a working stick;
//! 6. the DJ's player settings, if given, are written after it;
//! 7. for a hybrid stick, the Denon library (`Engine Library`) is built over
//!    the same audio files and swapped in the same way (see [`crate::engine`]).
//!
//! Files that are no longer part of the library stay on the stick; nothing
//! of the DJ's is deleted.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pioneer_formats::anlz::{AnlzFile, CuePoint, build_path, tag, write_cue_points};
use pioneer_formats::pdb::{
    self, Album, Artist, Database, Key, Named, PlaylistEntry, PlaylistNode, Track, WriteOptions,
};
use pioneer_formats::setting::MySettings;
use serde::{Deserialize, Serialize};

use crate::cue_write::replace_file;
use crate::names::{sanitize, split_ext};

/// A library track to put on the stick.
#[derive(Debug, Clone, Default)]
pub struct SyncTrack {
    /// Library id; stable across syncs.
    pub key: i64,
    pub audio: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub key_name: String,
    pub bpm: f64,
    pub duration: f64,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub sample_depth: u32,
    pub rating: u8,
    pub color: u8,
    pub comment: String,
    pub year: u32,
    pub track_number: u32,
    pub date_added: String,
    /// The track's `ANLZ0000.DAT` (with `.EXT` / `.2EX` beside it), if analysed.
    pub anlz_dat: Option<PathBuf>,
    pub cues: Vec<CuePoint>,
}

#[derive(Debug, Clone)]
pub struct SyncPlaylist {
    pub key: i64,
    /// Key of the parent folder, 0 at the top.
    pub parent: i64,
    pub name: String,
    pub is_folder: bool,
    pub sort: i64,
    pub tracks: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct SyncRequest {
    pub stick: PathBuf,
    pub tracks: Vec<SyncTrack>,
    pub playlists: Vec<SyncPlaylist>,
    /// Shown by the players as the device name.
    pub device_name: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Player settings ("My Settings") to put on the stick as well.
    pub settings: Option<MySettings>,
    /// Also write a Denon / Engine DJ library in this schema.
    pub engine: Option<engine_library::Schema>,
}

/// What a track needs on the stick.
#[derive(Debug, Clone)]
pub struct PlannedTrack {
    pub key: i64,
    pub device_path: String,
    pub size: u64,
    /// Copy the audio (false: an identical file is already there).
    pub copy: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SyncPlan {
    pub tracks: Vec<PlannedTrack>,
    pub copy_files: usize,
    pub copy_bytes: u64,
    pub reuse_files: usize,
    pub free_bytes: u64,
    /// Library tracks whose audio file is missing on the Mac (left out).
    pub missing_audio: Vec<String>,
    /// Tracks without rekordbox analysis: they play, but without waveform,
    /// beat grid or cues.
    pub no_analysis: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    /// Library key → where it lives on the stick.
    tracks: BTreeMap<i64, ManifestTrack>,
    /// Analysis folders this app wrote (removed when no longer used).
    anlz_dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestTrack {
    device_path: String,
    size: u64,
    /// Source file modification time (seconds) when it was copied.
    mtime: u64,
}

const MANIFEST: &str = ".rbx/manifest.json";

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_manifest(stick: &Path) -> Manifest {
    std::fs::read(stick.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// FAT/exFAT-safe path component (also forbids characters macOS allows).
fn fat_name(s: &str) -> String {
    let s: String = sanitize(s)
        .chars()
        .map(|c| {
            if matches!(c, '?' | '*' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    s.trim_end_matches(['.', ' ']).to_string()
}

fn device_path_for(t: &SyncTrack) -> String {
    let file = t
        .audio
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let artist = if t.artist.trim().is_empty() {
        "UnknownArtist".to_string()
    } else {
        fat_name(&t.artist)
    };
    let album = if t.album.trim().is_empty() {
        "UnknownAlbum".to_string()
    } else {
        fat_name(&t.album)
    };
    let (stem, ext) = split_ext(&file);
    format!(
        "/Contents/{artist}/{album}/{}{}",
        fat_name(stem),
        ext.to_lowercase()
    )
}

fn anlz_dir_for(device_path: &str) -> String {
    let h = fnv(device_path);
    format!(
        "/PIONEER/USBANLZ/P{:03X}/{:08X}",
        (h >> 32) & 0xfff,
        h & 0xffff_ffff
    )
}

fn resolve(stick: &Path, device: &str) -> PathBuf {
    stick.join(device.trim_start_matches('/'))
}

fn free_space(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        if let Ok(c) = CString::new(path.as_os_str().as_bytes()) {
            // SAFETY: statfs fills the zeroed struct for a valid path.
            unsafe {
                let mut s: libc::statfs = std::mem::zeroed();
                if libc::statfs(c.as_ptr(), &mut s) == 0 {
                    return s.f_bavail * s.f_bsize as u64;
                }
            }
        }
    }
    0
}

/// Work out what has to be copied. Reads the stick, writes nothing.
pub fn plan(req: &SyncRequest) -> SyncPlan {
    let manifest = load_manifest(&req.stick);
    // Audio a previous rekordbox export left on the stick, by file name and size.
    let mut existing: HashMap<(String, u64), String> = HashMap::new();
    if let Ok(b) = std::fs::read(req.stick.join("PIONEER/rekordbox/export.pdb"))
        && let Ok(db) = Database::parse(&b)
    {
        for t in &db.tracks {
            let file = Path::new(&t.file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase());
            if let Some(f) = file {
                existing.insert((f, t.file_size as u64), t.file_path.clone());
            }
        }
    }

    let mut out = SyncPlan {
        free_bytes: free_space(&req.stick),
        ..Default::default()
    };
    let mut used: HashSet<String> = HashSet::new();
    for t in &req.tracks {
        let Ok(meta) = std::fs::metadata(&t.audio) else {
            out.missing_audio.push(format!(
                "{} – {} ({})",
                t.artist,
                t.title,
                t.audio.display()
            ));
            continue;
        };
        let size = meta.len();
        if t.anlz_dat.as_ref().is_none_or(|p| !p.is_file()) {
            out.no_analysis.push(format!("{} – {}", t.artist, t.title));
        }
        let on_stick = |device: &str| {
            std::fs::metadata(resolve(&req.stick, device)).is_ok_and(|m| m.len() == size)
        };
        // 1. Where this app put it before, if the source did not change.
        let from_manifest = manifest
            .tracks
            .get(&t.key)
            .filter(|m| m.size == size && m.mtime == mtime(&t.audio) && on_stick(&m.device_path))
            .map(|m| m.device_path.clone());
        // 2. A file a rekordbox export left there.
        let file = t
            .audio
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let from_rekordbox = existing.get(&(file, size)).filter(|d| on_stick(d)).cloned();
        let (device_path, copy) = match from_manifest.or(from_rekordbox) {
            Some(d) => (d, false),
            None => {
                let base = device_path_for(t);
                let mut d = base.clone();
                let mut n = 2;
                while used.contains(&d.to_lowercase()) {
                    let (stem, ext) = split_ext(&base);
                    d = format!("{stem} ({n}){ext}");
                    n += 1;
                }
                let copy = !on_stick(&d);
                (d, copy)
            }
        };
        used.insert(device_path.to_lowercase());
        if copy {
            out.copy_files += 1;
            out.copy_bytes += size;
        } else {
            out.reuse_files += 1;
        }
        out.tracks.push(PlannedTrack {
            key: t.key,
            device_path,
            size,
            copy,
        });
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct SyncProgress {
    /// `backup`, `copy`, `analysis`, `database`, `settings`, `engine`, `verify`.
    pub phase: &'static str,
    /// The whole sync, 0 to 1.
    pub fraction: f64,
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub tracks: usize,
    pub playlists: usize,
    pub copied: usize,
    pub reused: usize,
    pub backup_dir: PathBuf,
    pub moved_aside: Vec<String>,
    pub warnings: Vec<String>,
    /// Tracks left out because their file could not be copied.
    pub skipped: Vec<String>,
    pub cancelled: bool,
    pub settings_written: bool,
    pub engine: Option<crate::engine::EngineReport>,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Na flashce je málo místa: potřeba {need} MB, volných {free} MB.")]
    NoSpace { need: u64, free: u64 },
    #[error("{0}")]
    Io(String),
    #[error("Nová databáze neprošla kontrolou, flashka zůstala beze změny: {0}")]
    Verify(String),
}

fn io(context: impl std::fmt::Display) -> impl Fn(std::io::Error) -> SyncError {
    move |e| SyncError::Io(format!("{context}: {e}"))
}

fn full_sync(f: &File) {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: F_FULLFSYNC on a valid open descriptor.
        if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
            return;
        }
    }
    let _ = f.sync_all();
}

/// Copy through `<name>.rbx-part`, flushed to the device, then renamed.
fn copy_file(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> std::io::Result<bool> {
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let part = dst.with_file_name(format!(".{name}.rbx-part"));
    let mut input = File::open(src)?;
    let result = (|| {
        let mut out = File::create(&part)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            progress(n as u64);
        }
        full_sync(&out);
        drop(out);
        if std::fs::metadata(&part)?.len() != std::fs::metadata(src)?.len() {
            return Err(std::io::Error::other("copied size differs from the source"));
        }
        std::fs::rename(&part, dst)?;
        Ok(true)
    })();
    // Cancelled or failed half way (a bad sector): no partial file stays.
    if !matches!(result, Ok(true)) {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn file_type(path: &str) -> u16 {
    match Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("mp3") => pdb::file_type::MP3,
        Some("m4a" | "mp4" | "aac") => pdb::file_type::M4A,
        Some("flac") => pdb::file_type::FLAC,
        Some("wav") => pdb::file_type::WAV,
        Some("aif" | "aiff") => pdb::file_type::AIFF,
        _ => 0,
    }
}

/// Build the analysis files for a track: rekordbox's analysis with the path
/// pointing at the stick and the library's cues.
fn build_anlz(t: &SyncTrack, device_path: &str) -> Option<Vec<(&'static str, Vec<u8>)>> {
    let dat_path = t.anlz_dat.as_ref()?;
    let read = |p: &Path| std::fs::read(p).ok().and_then(|b| AnlzFile::parse(&b).ok());
    let mut dat = read(dat_path)?;
    let mut ext = read(&dat_path.with_extension("EXT"));
    let mut two = read(&dat_path.with_extension("2EX"));
    for f in [Some(&mut dat), ext.as_mut(), two.as_mut()]
        .into_iter()
        .flatten()
    {
        if f.section(tag::PATH).is_some() {
            f.set(build_path(device_path));
        } else {
            f.sections.insert(0, build_path(device_path));
        }
    }
    write_cue_points(&mut dat, ext.as_mut(), &t.cues);
    let mut out = vec![("DAT", dat.to_bytes())];
    if let Some(e) = ext {
        out.push(("EXT", e.to_bytes()));
    }
    if let Some(e) = two {
        out.push(("2EX", e.to_bytes()));
    }
    Some(out)
}

/// Turn the library selection into an `export.pdb` model.
fn build_database(
    req: &SyncRequest,
    plan: &SyncPlan,
    anlz_paths: &HashMap<i64, String>,
) -> Database {
    let planned: HashMap<i64, &PlannedTrack> = plan.tracks.iter().map(|p| (p.key, p)).collect();
    let mut db = Database::default();
    let mut ids: HashMap<i64, u32> = HashMap::new();
    let mut names: HashMap<(u8, String), u32> = HashMap::new();
    // One row per distinct name: 0 artists, 1 genres, 2 labels, 3 keys.
    let mut intern = |kind: u8, name: &str, db: &mut Database| -> u32 {
        if name.trim().is_empty() {
            return 0;
        }
        if let Some(&id) = names.get(&(kind, name.to_string())) {
            return id;
        }
        let name = name.to_string();
        let id = match kind {
            0 => {
                let id = db.artists.len() as u32 + 1;
                db.artists.push(Artist {
                    id,
                    name: name.clone(),
                });
                id
            }
            1 => {
                let id = db.genres.len() as u32 + 1;
                db.genres.push(Named {
                    id,
                    name: name.clone(),
                });
                id
            }
            2 => {
                let id = db.labels.len() as u32 + 1;
                db.labels.push(Named {
                    id,
                    name: name.clone(),
                });
                id
            }
            _ => {
                let id = db.keys.len() as u32 + 1;
                db.keys.push(Key {
                    id,
                    id2: id,
                    name: name.clone(),
                });
                id
            }
        };
        names.insert((kind, name), id);
        id
    };
    let mut albums: HashMap<(String, u32), u32> = HashMap::new();
    for t in &req.tracks {
        let Some(p) = planned.get(&t.key) else {
            continue;
        };
        let id = db.tracks.len() as u32 + 1;
        ids.insert(t.key, id);
        let artist_id = intern(0, &t.artist, &mut db);
        let album_id = if t.album.trim().is_empty() {
            0
        } else {
            let next = albums.len() as u32 + 1;
            *albums
                .entry((t.album.clone(), artist_id))
                .or_insert_with(|| {
                    db.albums.push(Album {
                        id: next,
                        artist_id,
                        name: t.album.clone(),
                    });
                    next
                })
        };
        let filename = Path::new(&p.device_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let genre_id = intern(1, &t.genre, &mut db);
        let label_id = intern(2, &t.label, &mut db);
        let key_id = intern(3, &t.key_name, &mut db);
        let remixer_id = intern(0, &t.remixer, &mut db);
        db.tracks.push(Track {
            title: t.title.clone(),
            artist_id,
            album_id,
            genre_id,
            label_id,
            key_id,
            remixer_id,
            tempo: (t.bpm * 100.0).round() as u32,
            duration: t.duration.round() as u16,
            bitrate: t.bitrate,
            sample_rate: t.sample_rate,
            sample_depth: t.sample_depth as u16,
            file_size: p.size as u32,
            file_type: file_type(&p.device_path),
            rating: t.rating.min(5),
            color_id: if t.color <= 8 { t.color } else { 0 },
            comment: t.comment.clone(),
            year: t.year as u16,
            track_number: t.track_number,
            date_added: t.date_added.clone(),
            analyze_path: anlz_paths.get(&t.key).cloned().unwrap_or_default(),
            analyze_date: if anlz_paths.contains_key(&t.key) {
                req.date.clone()
            } else {
                String::new()
            },
            filename,
            file_path: p.device_path.clone(),
            ..Track::new_export(id)
        });
    }

    // Playlists: new ids in tree order; folders keep their place.
    let mut pl_ids: HashMap<i64, u32> = HashMap::new();
    let mut sorted: Vec<&SyncPlaylist> = req.playlists.iter().collect();
    sorted.sort_by_key(|p| (p.parent, p.sort, p.key));
    for p in &sorted {
        pl_ids.insert(p.key, pl_ids.len() as u32 + 1);
    }
    let mut order: HashMap<u32, u32> = HashMap::new();
    for p in sorted {
        let id = pl_ids[&p.key];
        let parent = if p.parent == 0 {
            0
        } else {
            pl_ids.get(&p.parent).copied().unwrap_or(0)
        };
        let slot = order.entry(parent).or_insert(0);
        db.playlists.push(PlaylistNode {
            id,
            parent_id: parent,
            sort_order: *slot,
            is_folder: p.is_folder,
            name: p.name.clone(),
        });
        *slot += 1;
        if !p.is_folder {
            let mut n = 0;
            for k in &p.tracks {
                if let Some(&tid) = ids.get(k) {
                    n += 1;
                    db.playlist_entries.push(PlaylistEntry {
                        entry_index: n,
                        track_id: tid,
                        playlist_id: id,
                    });
                }
            }
        }
    }
    db
}

/// Rough share of the whole sync each phase takes, from the time it
/// usually needs: copying runs at stick speed, the rest is per track.
struct Weights {
    starts: Vec<(&'static str, f64, f64)>,
}

impl Weights {
    fn new(plan: &SyncPlan, settings: bool, engine: bool) -> Self {
        let n = plan.tracks.len() as f64;
        let phases = [
            ("backup", 0.5),
            ("copy", plan.copy_bytes as f64 / 20e6),
            ("analysis", 0.02 * n),
            ("database", 0.5 + 0.002 * n),
            ("settings", if settings { 0.3 } else { 0.0 }),
            ("engine", if engine { 0.5 + 0.01 * n } else { 0.0 }),
            ("verify", 0.3 + 0.002 * n),
        ];
        let total: f64 = phases.iter().map(|p| p.1).sum::<f64>().max(f64::EPSILON);
        let mut at = 0.0;
        let starts = phases
            .iter()
            .map(|&(name, secs)| {
                let start = at / total;
                at += secs;
                (name, start, secs / total)
            })
            .collect();
        Self { starts }
    }

    /// `within`: how far the phase is, 0 to 1.
    fn at(&self, phase: &str, within: f64) -> f64 {
        self.starts
            .iter()
            .find(|p| p.0 == phase)
            .map_or(0.0, |p| (p.1 + p.2 * within.clamp(0.0, 1.0)).min(1.0))
    }
}

/// Carry out a plan. See the module docs for the order of operations.
pub fn execute(
    req: &SyncRequest,
    plan: &SyncPlan,
    backup_dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&SyncProgress),
) -> Result<SyncReport, SyncError> {
    let stick = &req.stick;
    let mut report = SyncReport {
        backup_dir: backup_dir.to_path_buf(),
        ..Default::default()
    };
    let w = Weights::new(plan, req.settings.is_some(), req.engine.is_some());
    if plan.copy_bytes + 64 * 1024 * 1024 > plan.free_bytes && plan.free_bytes > 0 {
        return Err(SyncError::NoSpace {
            need: plan.copy_bytes / 1_000_000 + 64,
            free: plan.free_bytes / 1_000_000,
        });
    }

    // 1. Back up the stick's database files.
    progress(&SyncProgress {
        phase: "backup",
        total: 1,
        ..Default::default()
    });
    std::fs::create_dir_all(backup_dir).map_err(io("záloha"))?;
    for f in [
        "PIONEER/rekordbox/export.pdb",
        "PIONEER/rekordbox/exportExt.pdb",
        "PIONEER/rekordbox/exportLibrary.db",
        MANIFEST,
    ] {
        let src = stick.join(f);
        if src.is_file() {
            let dst = backup_dir.join(f);
            std::fs::create_dir_all(dst.parent().unwrap()).map_err(io("záloha"))?;
            std::fs::copy(&src, &dst).map_err(io(format!("záloha {f}")))?;
        }
    }

    // 2. Audio.
    let by_key: HashMap<i64, &SyncTrack> = req.tracks.iter().map(|t| (t.key, t)).collect();
    let mut p = SyncProgress {
        phase: "copy",
        total: plan.copy_files,
        bytes_total: plan.copy_bytes,
        ..Default::default()
    };
    let copy_fraction = |done: u64| w.at("copy", done as f64 / plan.copy_bytes.max(1) as f64);
    // A file that cannot be read (damaged, unplugged disk) is left out, as
    // rekordbox does: the rest of the stick still gets written.
    let mut skipped: HashSet<i64> = HashSet::new();
    for pt in plan.tracks.iter().filter(|t| t.copy) {
        let t = by_key[&pt.key];
        p.current = format!("{} – {}", t.artist, t.title);
        p.fraction = copy_fraction(p.bytes_done);
        progress(&p);
        let mut last = std::time::Instant::now();
        let mut copied = 0u64;
        let result = copy_file(&t.audio, &resolve(stick, &pt.device_path), cancel, |n| {
            copied += n;
            p.bytes_done += n;
            p.fraction = copy_fraction(p.bytes_done);
            if last.elapsed().as_millis() > 120 {
                last = std::time::Instant::now();
                progress(&p);
            }
        });
        p.done += 1;
        match result {
            Ok(true) => report.copied += 1,
            Ok(false) => {
                report.cancelled = true;
                return Ok(report);
            }
            Err(e) => {
                // Keep the bar honest: the rest of this file will not come.
                p.bytes_done += pt.size.saturating_sub(copied);
                skipped.insert(pt.key);
                report.skipped.push(format!("{} – {} ({e})", t.artist, t.title));
            }
        }
    }
    report.reused = plan.reuse_files;
    // From here on only the tracks that made it onto the stick.
    let kept;
    let plan = if skipped.is_empty() {
        plan
    } else {
        kept = SyncPlan {
            tracks: plan.tracks.iter().filter(|t| !skipped.contains(&t.key)).cloned().collect(),
            ..plan.clone()
        };
        &kept
    };

    // 3. Analysis files, in folders the current library does not use.
    let mut anlz_paths: HashMap<i64, String> = HashMap::new();
    let mut anlz_dirs: Vec<String> = Vec::new();
    p = SyncProgress {
        phase: "analysis",
        total: plan.tracks.len(),
        ..Default::default()
    };
    for pt in &plan.tracks {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let t = by_key[&pt.key];
        p.current = t.title.clone();
        p.fraction = w.at("analysis", p.done as f64 / p.total.max(1) as f64);
        p.done += 1;
        progress(&p);
        let Some(files) = build_anlz(t, &pt.device_path) else {
            continue;
        };
        let dir = anlz_dir_for(&pt.device_path);
        let abs = resolve(stick, &dir);
        // Without its analysis files a track still plays (no waveform, grid
        // or cues): a write error here costs that track, not the sync.
        let written = std::fs::create_dir_all(&abs).and_then(|()| {
            files
                .iter()
                .try_for_each(|(ext, bytes)| replace_file(&abs.join(format!("ANLZ0000.{ext}")), bytes))
        });
        if let Err(e) = written {
            report
                .warnings
                .push(format!("{} – {}: analýza se nezapsala ({e})", t.artist, t.title));
            continue;
        }
        anlz_paths.insert(pt.key, format!("{dir}/ANLZ0000.DAT"));
        anlz_dirs.push(dir);
    }

    // 4. The new database, checked before it goes live.
    p = SyncProgress {
        phase: "database",
        total: 1,
        fraction: w.at("database", 0.0),
        ..Default::default()
    };
    progress(&p);
    let db = build_database(req, plan, &anlz_paths);
    let bytes = pdb::write(
        &db,
        &WriteOptions {
            device_name: req.device_name.clone(),
            export_date: req.date.clone(),
        },
    );
    let check = Database::parse(&bytes).map_err(|e| SyncError::Verify(e.to_string()))?;
    if !check.problems.is_empty()
        || check.tracks.len() != db.tracks.len()
        || check.playlists.len() != db.playlists.len()
    {
        return Err(SyncError::Verify(format!("{:?}", check.problems)));
    }
    let rb = stick.join("PIONEER/rekordbox");
    std::fs::create_dir_all(&rb).map_err(io("PIONEER/rekordbox"))?;

    // 5. Stale companion databases would show the old library (OneLibrary
    //    players) or tag the wrong tracks; move them aside.
    let aside = stick.join(".rbx/old").join(&req.date);
    for f in ["exportExt.pdb", "exportLibrary.db"] {
        let src = rb.join(f);
        if src.is_file() {
            std::fs::create_dir_all(&aside).map_err(io(".rbx/old"))?;
            std::fs::rename(&src, aside.join(f)).map_err(io(f))?;
            report.moved_aside.push(f.to_string());
        }
    }

    // 6. Commit: replace export.pdb in one rename.
    replace_file(&rb.join("export.pdb"), &bytes).map_err(io("export.pdb"))?;

    // 7. Remember where everything went; drop our analysis folders no longer used.
    let old = load_manifest(stick);
    let mut manifest = Manifest {
        version: 1,
        ..Default::default()
    };
    for pt in &plan.tracks {
        manifest.tracks.insert(
            pt.key,
            ManifestTrack {
                device_path: pt.device_path.clone(),
                size: pt.size,
                mtime: mtime(&by_key[&pt.key].audio),
            },
        );
    }
    manifest.anlz_dirs = anlz_dirs.clone();
    let keep: HashSet<&String> = anlz_dirs.iter().collect();
    for d in old.anlz_dirs.iter().filter(|d| !keep.contains(d)) {
        let _ = std::fs::remove_dir_all(resolve(stick, d));
    }
    let mpath = stick.join(MANIFEST);
    std::fs::create_dir_all(mpath.parent().unwrap()).map_err(io(".rbx"))?;
    replace_file(
        &mpath,
        &serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(io(MANIFEST))?;

    // 8. Player settings. The library is already in place, so a failure
    //    here is only a warning.
    if let Some(settings) = &req.settings {
        progress(&SyncProgress {
            phase: "settings",
            total: 1,
            fraction: w.at("settings", 0.0),
            ..Default::default()
        });
        if let Err(e) = crate::settings::write(stick, settings, &backup_dir.join("PIONEER")) {
            report
                .warnings
                .push(format!("Nastavení přehrávače se nezapsalo: {e}"));
        } else {
            report.settings_written = true;
        }
    }

    // 9. The Denon library over the same files. The rekordbox library is in
    //    place already, so a failure here leaves a working Pioneer stick.
    if let Some(schema) = req.engine {
        progress(&SyncProgress {
            phase: "engine",
            total: 1,
            fraction: w.at("engine", 0.0),
            ..Default::default()
        });
        match crate::engine::install(req, plan, schema, backup_dir) {
            Ok(e) => {
                report.warnings.extend(
                    e.warnings
                        .iter()
                        .filter(|w| w.starts_with("Denon nenajde"))
                        .cloned(),
                );
                report.engine = Some(e);
            }
            Err(e) => report
                .warnings
                .push(format!("Denon knihovna se nezapsala: {e}")),
        }
    }

    // 10. Read back what the players will read.
    p = SyncProgress {
        phase: "verify",
        total: 1,
        fraction: w.at("verify", 0.0),
        ..Default::default()
    };
    progress(&p);
    let back = std::fs::read(rb.join("export.pdb")).map_err(io("export.pdb"))?;
    let back = Database::parse(&back).map_err(|e| SyncError::Verify(e.to_string()))?;
    for t in &back.tracks {
        if !resolve(stick, &t.file_path).is_file() {
            report
                .warnings
                .push(format!("Chybí audio: {}", t.file_path));
        }
        if !t.analyze_path.is_empty() && !resolve(stick, &t.analyze_path).is_file() {
            report
                .warnings
                .push(format!("Chybí analýza: {}", t.analyze_path));
        }
    }
    report.tracks = back.tracks.len();
    report.playlists = back.playlists.iter().filter(|p| !p.is_folder).count();
    Ok(report)
}
