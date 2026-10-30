//! Health check of a stick: database, files, analysis, formats each player
//! can play, filesystem, macOS junk. Read-only.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use pioneer_formats::anlz::AnlzFile;
use serde::{Deserialize, Serialize};

use crate::Stick;
use crate::compat::{Player, can_mount, can_play};
use crate::probe::{AudioInfo, probe_file};

#[derive(Debug, Clone)]
pub enum Problem {
    MissingAudio,
    /// Track has no or an unreadable `ANLZ0000.DAT`: no waveform, grid or cues.
    MissingAnalysis(String),
    /// File size differs from the database: the file changed after rekordbox
    /// analysed it (e.g. tags or artwork written), or it was replaced.
    SizeMismatch {
        database: u64,
        actual: u64,
    },
    /// MP3 whose ID3 tag declares a wrong size: stray bytes before the audio.
    BrokenTag {
        junk: usize,
    },
    /// File content does not match its extension.
    WrongExtension {
        extension: String,
        actual: String,
    },
    /// One or more players cannot play this file.
    Unsupported {
        players: Vec<Player>,
        reason: String,
    },
    Unreadable(String),
}

impl Problem {
    /// Warnings usually still play; everything else is likely to fail on a player.
    pub fn is_warning(&self) -> bool {
        matches!(self, Problem::SizeMismatch { .. })
    }
}

#[derive(Debug, Clone)]
pub struct TrackIssue {
    pub track_id: u32,
    pub display: String,
    pub file_path: String,
    pub playlists: Vec<String>,
    pub audio: Option<AudioInfo>,
    pub problem: Problem,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub tracks: usize,
    pub playlists: usize,
    pub db_problems: Vec<String>,
    /// `statfs` filesystem type, e.g. `msdos`, `exfat`.
    pub filesystem: Option<String>,
    pub filesystem_issues: Vec<(Player, String)>,
    /// macOS `._*` AppleDouble files on the stick.
    pub apple_double: Vec<String>,
    pub issues: Vec<TrackIssue>,
    /// Formats found: description → count.
    pub formats: BTreeMap<String, usize>,
}

/// Size and modification time of a file: when both match, its content is
/// taken as unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    size: u64,
    modified_ms: u64,
}

impl Stamp {
    fn of(meta: &std::fs::Metadata) -> Self {
        Stamp {
            size: meta.len(),
            modified_ms: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64),
        }
    }
}

/// What reading a track's files found. Read errors are not kept, so a file
/// that failed to read is read again next time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Checked {
    audio: Option<(Stamp, AudioInfo)>,
    /// The analysis file and its parse error, if any.
    anlz: Option<(Stamp, Option<String>)>,
}

/// Results of earlier checks, keyed by the audio file's full path, so a
/// check of a stick only reads the files that changed since.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VerifyCache {
    tracks: HashMap<String, Checked>,
}

#[cfg(target_os = "macos")]
pub fn filesystem_type(path: &Path) -> Option<String> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statfs fills the zeroed struct; f_fstypename is NUL-terminated.
    unsafe {
        let mut s: libc::statfs = std::mem::zeroed();
        if libc::statfs(c.as_ptr(), &mut s) != 0 {
            return None;
        }
        Some(
            CStr::from_ptr(s.f_fstypename.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(target_os = "macos"))]
pub fn filesystem_type(_: &Path) -> Option<String> {
    None
}

fn find_apple_double(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("._") {
            out.push(p.strip_prefix(root).unwrap_or(&p).display().to_string());
        } else if p.is_dir() && !name.starts_with('.') {
            find_apple_double(&p, root, out);
        }
    }
}

fn expected_container(ext: &str) -> Option<&'static [&'static str]> {
    Some(match ext {
        "mp3" => &["Mp3"],
        "m4a" | "mp4" | "aac" | "alac" => &["Mp4"],
        "wav" => &["Wav"],
        "aif" | "aiff" => &["Aiff"],
        "flac" => &["Flac"],
        _ => return None,
    })
}

pub fn verify(
    stick: &Stick,
    players: &[Player],
    cancel: &AtomicBool,
    progress: impl FnMut(usize, usize),
) -> VerifyReport {
    verify_cached(stick, players, &mut VerifyCache::default(), cancel, progress)
}

/// [`verify`], reading only the files that changed since `cache` saw them.
/// Entries of this stick's tracks that are gone are dropped from `cache`.
pub fn verify_cached(
    stick: &Stick,
    players: &[Player],
    cache: &mut VerifyCache,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> VerifyReport {
    let db = &stick.db;
    let mut fresh = HashMap::new();
    let mut r = VerifyReport {
        tracks: db.tracks.len(),
        playlists: db.playlists.iter().filter(|p| !p.is_folder).count(),
        db_problems: db.problems.clone(),
        filesystem: filesystem_type(&stick.root),
        ..Default::default()
    };
    if let Some(fs) = &r.filesystem {
        for &p in players {
            if let Err(why) = can_mount(p, fs) {
                r.filesystem_issues.push((p, why));
            }
        }
    }
    find_apple_double(&stick.root, &stick.root, &mut r.apple_double);

    let mut in_playlists: HashMap<u32, Vec<String>> = HashMap::new();
    for e in &db.playlist_entries {
        let name = db.playlist_path(e.playlist_id).join(" / ");
        let v = in_playlists.entry(e.track_id).or_default();
        if !v.contains(&name) {
            v.push(name);
        }
    }

    for (i, t) in db.tracks.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        progress(i, db.tracks.len());
        let display = match db.artist_name(t.artist_id) {
            Some(a) if !a.is_empty() => format!("{a} – {}", t.title),
            _ => t.title.clone(),
        };
        let mut issue = |audio: Option<AudioInfo>, problem: Problem| {
            r.issues.push(TrackIssue {
                track_id: t.id,
                display: display.clone(),
                file_path: t.file_path.clone(),
                playlists: in_playlists.get(&t.id).cloned().unwrap_or_default(),
                audio,
                problem,
            })
        };
        let path = stick.audio_path(t);
        let Ok(meta) = std::fs::metadata(&path) else {
            issue(None, Problem::MissingAudio);
            continue;
        };
        let key = path.display().to_string();
        let audio_stamp = Stamp::of(&meta);
        let prev = cache.tracks.get(&key).cloned().unwrap_or_default();
        let mut checked = Checked::default();
        if t.file_size != 0 && meta.len() != t.file_size as u64 {
            issue(
                None,
                Problem::SizeMismatch {
                    database: t.file_size as u64,
                    actual: meta.len(),
                },
            );
        }
        if t.analyze_path.is_empty() {
            issue(
                None,
                Problem::MissingAnalysis("no analysis path in database".into()),
            );
        } else {
            let anlz = stick.resolve(&t.analyze_path);
            let stamp = std::fs::metadata(&anlz).ok().map(|m| Stamp::of(&m));
            // None: unreadable; Some(err): read, with its parse error if any.
            let parsed = match (stamp, prev.anlz) {
                (Some(s), Some((was, err))) if s == was => Some(err),
                (Some(_), _) => std::fs::read(&anlz)
                    .ok()
                    .map(|b| AnlzFile::parse(&b).err().map(|e| e.to_string())),
                (None, _) => None,
            };
            checked.anlz = stamp.zip(parsed.clone());
            match parsed {
                None => issue(
                    None,
                    Problem::MissingAnalysis(format!("{} missing", t.analyze_path)),
                ),
                Some(Some(e)) => issue(
                    None,
                    Problem::MissingAnalysis(format!("{}: {e}", t.analyze_path)),
                ),
                Some(None) => {}
            }
        }
        let info = match prev.audio {
            Some((was, info)) if was == audio_stamp => info,
            _ => match probe_file(&path) {
                Ok(i) => i,
                Err(e) => {
                    issue(None, Problem::Unreadable(e.to_string()));
                    fresh.insert(key, checked);
                    continue;
                }
            },
        };
        checked.audio = Some((audio_stamp, info));
        fresh.insert(key, checked);
        *r.formats.entry(info.describe()).or_default() += 1;
        if info.junk_after_tag > 0 {
            issue(
                Some(info),
                Problem::BrokenTag {
                    junk: info.junk_after_tag,
                },
            );
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Some(expected) = expected_container(&ext) {
            let actual = format!("{:?}", info.container);
            if !expected.contains(&actual.as_str()) {
                issue(
                    Some(info),
                    Problem::WrongExtension {
                        extension: ext.clone(),
                        actual,
                    },
                );
            }
        }
        // One issue per distinct reason, listing the players it affects.
        let mut by_reason: BTreeMap<String, Vec<Player>> = BTreeMap::new();
        for &p in players {
            if let Err(why) = can_play(p, &info) {
                by_reason.entry(why).or_default().push(p);
            }
        }
        for (reason, players) in by_reason {
            issue(Some(info), Problem::Unsupported { players, reason });
        }
    }
    // A finished check knows every track of the stick: forget the others.
    if !cancel.load(Ordering::Relaxed) {
        cache
            .tracks
            .retain(|k, _| !Path::new(k).starts_with(&stick.root));
    }
    cache.tracks.extend(fresh);
    progress(db.tracks.len(), db.tracks.len());
    r
}
