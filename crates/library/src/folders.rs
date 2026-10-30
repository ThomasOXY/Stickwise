//! Folders of audio files dropped into the library: each folder with music
//! becomes a playlist of the same name, folders of folders become playlist
//! folders. The files stay where they are.
//!
//! [`scan`] walks the disk and reads the tags without touching the
//! library, so the slow part runs without holding it; [`Library::add_scan`]
//! then writes the result in one transaction.

use std::path::{Path, PathBuf};

use lofty::prelude::*;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{Library, Result};

/// Formats the Pioneer players read.
const AUDIO: &[&str] = &["mp3", "wav", "aif", "aiff", "flac", "m4a", "mp4", "aac", "alac"];

/// One audio file, with what its tags say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub key: String,
    pub bpm: f64,
    pub duration: f64,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub sample_depth: u32,
    pub comment: String,
    pub year: u32,
    pub track_number: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScannedFolder {
    pub name: String,
    /// Audio files directly in this folder, by file name.
    pub tracks: Vec<NewTrack>,
    /// Subfolders that hold music somewhere inside.
    pub children: Vec<ScannedFolder>,
}

/// What was dropped: folders, and loose audio files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    pub folders: Vec<ScannedFolder>,
    pub files: Vec<NewTrack>,
    /// Dropped items with no music in them.
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderReport {
    pub playlists_created: usize,
    /// Playlists of the same name that were already there and got the new files.
    pub playlists_updated: usize,
    pub tracks_added: usize,
    /// Files already in the library (they are only put into the playlist).
    pub tracks_known: usize,
    /// Library ids of the tracks added now (they need analysing).
    pub added: Vec<i64>,
    /// First playlist made or updated, to show it.
    pub first_playlist: Option<i64>,
    /// Library ids of the loose files, in drop order.
    pub loose: Vec<i64>,
    pub ignored: Vec<String>,
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| AUDIO.contains(&e.as_str()))
}

fn hidden(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Read a file's tags. Untagged files get "Artist - Title" from the file
/// name when it looks like that, else the file name as title.
pub fn read_track(path: &Path) -> NewTrack {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut t = NewTrack {
        path: path.display().to_string(),
        ..Default::default()
    };
    if let Ok(f) = lofty::read_from_path(path) {
        let p = f.properties();
        t.duration = p.duration().as_secs_f64();
        t.bitrate = p.audio_bitrate().or(p.overall_bitrate()).unwrap_or(0);
        t.sample_rate = p.sample_rate().unwrap_or(0);
        t.sample_depth = p.bit_depth().unwrap_or(0) as u32;
        if let Some(tag) = f.primary_tag().or_else(|| f.first_tag()) {
            let s = |v: Option<std::borrow::Cow<'_, str>>| v.map(|v| v.trim().to_string()).unwrap_or_default();
            t.title = s(tag.title());
            t.artist = s(tag.artist());
            t.album = s(tag.album());
            t.genre = s(tag.genre());
            t.comment = s(tag.comment());
            t.year = tag.date().map_or(0, |d| d.year as u32);
            t.track_number = tag.track().unwrap_or(0);
            let get = |k: ItemKey| tag.get_string(k).map(str::trim).unwrap_or_default().to_string();
            t.key = camelot_from_open_key(&get(ItemKey::InitialKey));
            t.label = get(ItemKey::Label);
            t.remixer = get(ItemKey::Remixer);
            t.bpm = [ItemKey::Bpm, ItemKey::IntegerBpm]
                .into_iter()
                .find_map(|k| tag.get_string(k)?.trim().replace(',', ".").parse::<f64>().ok())
                .filter(|b| (20.0..400.0).contains(b))
                .unwrap_or(0.0);
        }
    }
    if t.title.is_empty() {
        // "01 - Artist - Title", "01. Title": the leading track number goes.
        let bare = stem.trim_start_matches(|c: char| c.is_ascii_digit());
        let bare = if bare.len() < stem.len() && bare.starts_with([' ', '.', '-', '_']) {
            bare.trim_start_matches([' ', '.', '-', '_'])
        } else {
            &stem
        };
        let stem = bare.to_string();
        match stem.split_once(" - ") {
            Some((a, b)) if t.artist.is_empty() => {
                t.artist = a.trim().to_string();
                t.title = b.trim().to_string();
            }
            _ => t.title = stem,
        }
    }
    t
}

/// Traktor's Open Key ("11m", "4d") as Camelot ("6A", "11B"), which the
/// rest of the app reads; other notations are kept as they are.
fn camelot_from_open_key(key: &str) -> String {
    let k = key.trim();
    let Some((i, mode)) = k.char_indices().last() else {
        return String::new();
    };
    match (k[..i].parse::<u8>(), mode) {
        (Ok(n @ 1..=12), 'm' | 'd') => format!("{}{}", (n + 6) % 12 + 1, if mode == 'm' { 'A' } else { 'B' }),
        _ => k.to_string(),
    }
}

fn scan_dir(dir: &Path) -> Option<ScannedFolder> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !hidden(p))
        .collect();
    entries.sort_by_key(|p| name_of(p).to_lowercase());
    let mut out = ScannedFolder {
        name: name_of(dir),
        ..Default::default()
    };
    for p in entries {
        // Not through symlinks: they can loop.
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.is_dir() {
            out.children.extend(scan_dir(&p));
        } else if meta.is_file() && is_audio(&p) {
            out.tracks.push(read_track(&p));
        }
    }
    (!out.tracks.is_empty() || !out.children.is_empty()).then_some(out)
}

/// Walk what was dropped and read every audio file's tags.
pub fn scan(paths: &[PathBuf]) -> Scan {
    let mut out = Scan::default();
    for p in paths {
        if p.is_dir() {
            match scan_dir(p) {
                Some(f) => out.folders.push(f),
                None => out.ignored.push(name_of(p)),
            }
        } else if p.is_file() && is_audio(p) {
            out.files.push(read_track(p));
        } else {
            out.ignored.push(name_of(p));
        }
    }
    out
}

impl Library {
    /// Library id of the file at `t.path`, adding it if it is new.
    fn add_track(tx: &rusqlite::Transaction, t: &NewTrack, r: &mut FolderReport) -> Result<i64> {
        let known: Option<i64> = tx
            .query_row("SELECT id FROM tracks WHERE path = ?1", [&t.path], |r| r.get(0))
            .optional()?;
        if let Some(id) = known {
            r.tracks_known += 1;
            return Ok(id);
        }
        tx.execute(
            "INSERT INTO tracks(path, title, artist, album, genre, label, remixer, key, bpm, duration, bitrate, \
             sample_rate, sample_depth, comment, year, track_number, date_added) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, date('now', 'localtime'))",
            params![
                t.path,
                t.title,
                t.artist,
                t.album,
                t.genre,
                t.label,
                t.remixer,
                t.key,
                t.bpm,
                t.duration,
                t.bitrate,
                t.sample_rate,
                t.sample_depth,
                t.comment,
                t.year,
                t.track_number
            ],
        )?;
        r.tracks_added += 1;
        let id = tx.last_insert_rowid();
        r.added.push(id);
        Ok(id)
    }

    /// The child of `parent` called `name` of this kind, made if missing.
    /// Returns its id and whether it was already there.
    fn child(tx: &rusqlite::Transaction, parent: i64, name: &str, folder: bool) -> Result<(i64, bool)> {
        let found: Option<i64> = tx
            .query_row(
                "SELECT id FROM playlists WHERE parent_id = ?1 AND name = ?2 AND is_folder = ?3 AND is_set = 0 \
                 ORDER BY sort_order LIMIT 1",
                params![parent, name, folder],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = found {
            return Ok((id, true));
        }
        tx.execute(
            "INSERT INTO playlists(parent_id, name, is_folder, sort_order, source) \
             VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM playlists WHERE parent_id = ?1), 'local')",
            params![parent, name, folder],
        )?;
        Ok((tx.last_insert_rowid(), false))
    }

    /// Append tracks a playlist does not have yet.
    fn append(tx: &rusqlite::Transaction, playlist: i64, ids: &[i64]) -> Result<()> {
        let mut have: Vec<i64> = {
            let mut st = tx.prepare("SELECT track_id FROM playlist_items WHERE playlist_id = ?1 ORDER BY position")?;
            st.query_map([playlist], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for &id in ids {
            if !have.contains(&id) {
                tx.execute(
                    "INSERT INTO playlist_items(playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
                    params![playlist, have.len() as i64, id],
                )?;
                have.push(id);
            }
        }
        tx.execute("UPDATE playlists SET source = 'local' WHERE id = ?1", [playlist])?;
        Ok(())
    }

    fn add_folder(
        tx: &rusqlite::Transaction,
        f: &ScannedFolder,
        parent: i64,
        r: &mut FolderReport,
    ) -> Result<()> {
        let ids = f
            .tracks
            .iter()
            .map(|t| Self::add_track(tx, t, r))
            .collect::<Result<Vec<_>>>()?;
        // Music only directly inside: a playlist. Subfolders too: a folder
        // with a playlist per subfolder (and one for the loose files).
        let (list_parent, list) = if f.children.is_empty() {
            (parent, true)
        } else {
            let (folder, _) = Self::child(tx, parent, &f.name, true)?;
            for c in &f.children {
                Self::add_folder(tx, c, folder, r)?;
            }
            (folder, !ids.is_empty())
        };
        if list {
            let (id, existed) = Self::child(tx, list_parent, &f.name, false)?;
            Self::append(tx, id, &ids)?;
            if existed {
                r.playlists_updated += 1;
            } else {
                r.playlists_created += 1;
            }
            r.first_playlist.get_or_insert(id);
        }
        Ok(())
    }

    /// Append tracks to a playlist, skipping ones it already has.
    pub fn add_to_playlist(&mut self, playlist: i64, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::append(&tx, playlist, ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Add a [`scan`] to the library: folders under `parent` (0 = top
    /// level), loose files to the library only.
    pub fn add_scan(&mut self, scan: &Scan, parent: i64) -> Result<FolderReport> {
        let mut r = FolderReport {
            ignored: scan.ignored.clone(),
            ..Default::default()
        };
        let tx = self.conn.transaction()?;
        for f in &scan.folders {
            Self::add_folder(&tx, f, parent, &mut r)?;
        }
        for t in &scan.files {
            let id = Self::add_track(&tx, t, &mut r)?;
            r.loose.push(id);
        }
        tx.commit()?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str) -> NewTrack {
        NewTrack {
            path: path.into(),
            title: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn open_key_becomes_camelot() {
        assert_eq!(camelot_from_open_key("1m"), "8A");
        assert_eq!(camelot_from_open_key("11m"), "6A");
        assert_eq!(camelot_from_open_key("6d"), "1B");
        assert_eq!(camelot_from_open_key("Am"), "Am");
        assert_eq!(camelot_from_open_key("8A"), "8A");
        assert_eq!(camelot_from_open_key(""), "");
        assert_eq!(camelot_from_open_key("C♯"), "C♯");
    }

    #[test]
    fn scans_folders_into_a_tree() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Techno");
        std::fs::create_dir_all(root.join("Peak")).unwrap();
        std::fs::create_dir_all(root.join("Empty")).unwrap();
        std::fs::write(root.join("02 Second.mp3"), b"x").unwrap();
        std::fs::write(root.join("01 Artist - First.mp3"), b"x").unwrap();
        std::fs::write(root.join("cover.jpg"), b"x").unwrap();
        std::fs::write(root.join("._01 Artist - First.mp3"), b"x").unwrap();
        std::fs::write(root.join("Peak/a.flac"), b"x").unwrap();
        let s = scan(&[root, d.path().join("nothing")]);
        assert_eq!(s.ignored, vec!["nothing"]);
        let f = &s.folders[0];
        assert_eq!(f.name, "Techno");
        let titles: Vec<_> = f.tracks.iter().map(|t| (t.artist.as_str(), t.title.as_str())).collect();
        assert_eq!(titles, vec![("Artist", "First"), ("", "Second")]);
        assert_eq!(f.children.len(), 1);
        assert_eq!(f.children[0].name, "Peak");
        assert_eq!(f.children[0].tracks[0].title, "a");
    }

    #[test]
    fn folders_become_playlists_and_dropping_again_adds_only_new() {
        let mut lib = Library::in_memory().unwrap();
        let s = Scan {
            folders: vec![ScannedFolder {
                name: "Techno".into(),
                tracks: vec![track("/m/t1.mp3")],
                children: vec![ScannedFolder {
                    name: "Peak".into(),
                    tracks: vec![track("/m/p1.mp3"), track("/m/p2.mp3")],
                    children: vec![],
                }],
            }],
            files: vec![track("/m/loose.mp3")],
            ignored: vec![],
        };
        let r = lib.add_scan(&s, 0).unwrap();
        assert_eq!((r.playlists_created, r.tracks_added, r.loose.len(), r.added.len()), (2, 4, 1, 4));

        // Analysis fills tempo, and the key only where the tags had none.
        let id = r.loose[0];
        lib.conn.execute("UPDATE tracks SET key = '8A' WHERE id = ?1", [id]).unwrap();
        lib.set_analysis(id, "/a/ANLZ0000.DAT", Some(126.0), Some("Fm"), 200.0).unwrap();
        let t = lib.track(id).unwrap().unwrap();
        assert_eq!((t.anlz.as_str(), t.bpm, t.key.as_str(), t.duration), ("/a/ANLZ0000.DAT", 126.0, "8A", 200.0));
        let ps = lib.playlists().unwrap();
        let folder = ps.iter().find(|p| p.is_folder && p.name == "Techno").unwrap();
        let peak = ps.iter().find(|p| p.name == "Peak").unwrap();
        let own = ps.iter().find(|p| !p.is_folder && p.name == "Techno").unwrap();
        assert_eq!((peak.parent_id, own.parent_id), (folder.id, folder.id));
        assert_eq!(peak.track_ids.len(), 2);

        // The same folder again, with one more file: nothing doubles.
        let mut s2 = s.clone();
        s2.folders[0].children[0].tracks.push(track("/m/p3.mp3"));
        let r2 = lib.add_scan(&s2, 0).unwrap();
        assert_eq!((r2.playlists_created, r2.playlists_updated, r2.tracks_added), (0, 2, 1));
        let ps = lib.playlists().unwrap();
        assert_eq!(ps.len(), 3);
        assert_eq!(ps.iter().find(|p| p.name == "Peak").unwrap().track_ids.len(), 3);
    }
}
