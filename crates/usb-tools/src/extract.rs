//! Copy playlists from a stick into folders on disk:
//! `<dest>/<folder>/<playlist>/01 - file.mp3` plus `<playlist>.m3u8`.
//!
//! Re-running is safe: files that already exist with the right size are
//! skipped, and every copy goes to a `.part` file that is renamed when done.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pioneer_formats::pdb::Track;

use crate::Stick;
use crate::names::{sanitize, split_ext, unique};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Duplicates {
    /// APFS clone of the first copy: no extra disk space (falls back to copy).
    #[default]
    Clone,
    Copy,
    /// Symlink to the first copy.
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Naming {
    /// Keep the file name from the stick.
    #[default]
    Original,
    /// `Artist - Title.ext`.
    ArtistTitle,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    /// Playlist ids to extract; empty = all playlists.
    pub playlists: Vec<u32>,
    /// Also extract the players' HISTORY playlists into `HISTORY/`.
    pub history: bool,
    /// Prefix files with their position: `01 - `.
    pub numbered: bool,
    pub naming: Naming,
    pub duplicates: Duplicates,
    /// Write `<playlist>.m3u8` next to the files.
    pub m3u: bool,
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    pub track_id: u32,
    pub source: PathBuf,
    pub file_name: String,
    pub size: u64,
    /// `#EXTINF` data: seconds and display name.
    pub duration: u16,
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct PlannedPlaylist {
    /// Folder relative to the destination, e.g. `Gigy/Klub X`.
    pub folder: PathBuf,
    pub name: String,
    pub files: Vec<PlannedFile>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub playlists: Vec<PlannedPlaylist>,
    /// Tracks referenced by a playlist whose audio file is not on the stick.
    pub missing: Vec<String>,
    pub total_files: usize,
    pub total_bytes: u64,
}

fn display_name(stick: &Stick, t: &Track) -> String {
    match stick.db.artist_name(t.artist_id) {
        Some(a) if !a.is_empty() => format!("{a} - {}", t.title),
        _ => t.title.clone(),
    }
}

/// Work out what goes where without touching the disk (beyond `stat`).
pub fn plan(stick: &Stick, opts: &ExtractOptions) -> Plan {
    let db = &stick.db;
    let mut lists: Vec<(PathBuf, String, Vec<u32>)> = Vec::new();

    let mut ids: Vec<u32> = if opts.playlists.is_empty() {
        db.playlists
            .iter()
            .filter(|p| !p.is_folder)
            .map(|p| p.id)
            .collect()
    } else {
        opts.playlists.clone()
    };
    // Tree order, so playlists come out in the order the player shows them.
    // `order` doubles as the visited set, so a damaged tree cannot loop.
    let mut order = HashMap::new();
    fn walk(db: &pioneer_formats::pdb::Database, parent: u32, order: &mut HashMap<u32, usize>) {
        for n in db.playlist_children(parent) {
            if order.contains_key(&n.id) {
                continue;
            }
            order.insert(n.id, order.len());
            if n.is_folder {
                walk(db, n.id, order);
            }
        }
    }
    walk(db, 0, &mut order);
    ids.sort_by_key(|id| order.get(id).copied().unwrap_or(usize::MAX));

    for id in ids {
        let path = db.playlist_path(id);
        let Some(name) = path.last() else { continue };
        let folder: PathBuf = path.iter().map(|p| sanitize(p)).collect();
        lists.push((folder, name.to_string(), db.playlist_tracks(id)));
    }
    if opts.history {
        for h in &db.history_playlists {
            let mut entries: Vec<_> = db
                .history_entries
                .iter()
                .filter(|e| e.playlist_id == h.id)
                .collect();
            entries.sort_by_key(|e| e.entry_index);
            lists.push((
                Path::new("HISTORY").join(sanitize(&h.name)),
                h.name.clone(),
                entries.iter().map(|e| e.track_id).collect(),
            ));
        }
    }

    let mut plan = Plan::default();
    let mut used_folders = HashSet::new();
    let mut missing = HashSet::new();
    for (folder, name, track_ids) in lists {
        // Two playlists with the same (sanitized) path get distinct folders.
        let folder = {
            let parent = folder.parent().map(Path::to_path_buf).unwrap_or_default();
            let leaf = folder
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let leaf = unique(&leaf, "", |c| used_folders.contains(&parent.join(c)));
            parent.join(leaf)
        };
        used_folders.insert(folder.clone());

        let mut taken: HashSet<String> = HashSet::new();
        let width = track_ids.len().to_string().len().max(2);
        let mut files = Vec::new();
        for (i, tid) in track_ids.iter().enumerate() {
            let Some(t) = db.track(*tid) else {
                missing.insert(format!("{name}: track id {tid} not in library"));
                continue;
            };
            let source = stick.audio_path(t);
            let Ok(meta) = std::fs::metadata(&source) else {
                missing.insert(format!("{} ({})", display_name(stick, t), t.file_path));
                continue;
            };
            let original = Path::new(&t.file_path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.filename.clone());
            let (orig_stem, ext) = split_ext(&original);
            let stem = match opts.naming {
                Naming::Original => sanitize(orig_stem),
                Naming::ArtistTitle => sanitize(&display_name(stick, t)),
            };
            let stem = if opts.numbered {
                format!("{:0width$} - {stem}", i + 1)
            } else {
                stem
            };
            let ext = ext.to_string();
            // Case-insensitive like APFS/HFS+ defaults.
            let file_name = unique(&stem, &ext, |c| taken.contains(&c.to_lowercase()));
            taken.insert(file_name.to_lowercase());
            plan.total_bytes += meta.len();
            plan.total_files += 1;
            files.push(PlannedFile {
                track_id: *tid,
                source,
                file_name,
                size: meta.len(),
                duration: t.duration,
                display: display_name(stick, t),
            });
        }
        plan.playlists.push(PlannedPlaylist {
            folder,
            name,
            files,
        });
    }
    plan.missing = missing.into_iter().collect();
    plan.missing.sort();
    plan
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub copied: usize,
    pub cloned: usize,
    pub linked: usize,
    pub skipped_existing: usize,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("creating {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// APFS copy-on-write clone. Fails across volumes or on non-APFS disks.
#[cfg(target_os = "macos")]
fn clone_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let a = CString::new(from.as_os_str().as_bytes())?;
    let b = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both arguments are valid NUL-terminated paths.
    if unsafe { libc::clonefile(a.as_ptr(), b.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn clone_file(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("clonefile is macOS only"))
}

fn copy_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    let part = to.with_extension(format!(
        "{}.part",
        to.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::copy(from, &part)?;
    std::fs::rename(&part, to)
}

fn write_m3u(dir: &Path, pl: &PlannedPlaylist) -> std::io::Result<()> {
    let mut s = String::from("#EXTM3U\n");
    for f in &pl.files {
        s.push_str(&format!(
            "#EXTINF:{},{}\n{}\n",
            f.duration, f.display, f.file_name
        ));
    }
    let name = format!("{}.m3u8", sanitize(&pl.name));
    std::fs::write(dir.join(name), s)
}

/// Carry out a plan. Only reads from the stick.
pub fn execute(
    plan: &Plan,
    dest: &Path,
    opts: &ExtractOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&Progress),
) -> Result<Report, ExtractError> {
    let mut report = Report::default();
    let mut p = Progress {
        files_total: plan.total_files,
        bytes_total: plan.total_bytes,
        ..Default::default()
    };
    // First place each track was written, for clones / links.
    let mut first: HashMap<u32, PathBuf> = HashMap::new();

    for pl in &plan.playlists {
        let dir = dest.join(&pl.folder);
        std::fs::create_dir_all(&dir).map_err(|source| ExtractError::CreateDir {
            path: dir.clone(),
            source,
        })?;
        for f in &pl.files {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return Ok(report);
            }
            let to = dir.join(&f.file_name);
            p.current = f.display.clone();
            progress(&p);

            let result = if std::fs::metadata(&to).is_ok_and(|m| m.len() == f.size) {
                report.skipped_existing += 1;
                Ok(())
            } else {
                let _ = std::fs::remove_file(&to);
                match (opts.duplicates, first.get(&f.track_id)) {
                    (Duplicates::Clone, Some(prev)) => match clone_file(prev, &to) {
                        Ok(()) => {
                            report.cloned += 1;
                            Ok(())
                        }
                        Err(_) => copy_atomic(&f.source, &to).map(|_| report.copied += 1),
                    },
                    (Duplicates::Symlink, Some(prev)) => {
                        std::os::unix::fs::symlink(prev, &to).map(|_| report.linked += 1)
                    }
                    _ => copy_atomic(&f.source, &to).map(|_| report.copied += 1),
                }
            };
            match result {
                Ok(()) => {
                    first.entry(f.track_id).or_insert(to);
                }
                Err(e) => report.errors.push(format!("{}: {e}", to.display())),
            }
            p.files_done += 1;
            p.bytes_done += f.size;
            progress(&p);
        }
        if opts.m3u
            && let Err(e) = write_m3u(&dir, pl) {
                report.errors.push(format!("{}: m3u8: {e}", dir.display()));
            }
    }
    Ok(report)
}
