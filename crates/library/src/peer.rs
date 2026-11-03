//! What sync between two Stickwise libraries (Mac ⇄ Mac) builds on: who
//! a track is regardless of where its file lies, when what changed, and
//! what was deleted.
//!
//! A track is its fingerprint, the BLAKE3 hash of the audio file; a
//! playlist is its uid. Triggers stamp the changes (unix ms), so every
//! write is caught: edits, rekordbox imports and folder scans alike.
//! Deleted playlists leave a tombstone so the other side can be asked.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Library, Result};

/// Now in unix milliseconds, as SQLite computes it.
const NOW_MS: &str = "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)";

/// Tag columns whose change is an edit of the track.
const META_COLUMNS: [&str; 13] = [
    "title", "artist", "album", "genre", "label", "remixer", "key", "bpm", "rating", "color", "comment",
    "year", "track_number",
];

/// Schema changes in order; `PRAGMA user_version` says how many are applied.
fn migrations() -> Vec<String> {
    let changed = META_COLUMNS
        .iter()
        .map(|c| format!("OLD.{c} IS NOT NEW.{c}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let stamp = |table: &str, column: &str, row: &str| {
        format!("UPDATE {table} SET {column} = {NOW_MS} WHERE id = {row};")
    };
    vec![format!(
        "
ALTER TABLE tracks ADD COLUMN fingerprint TEXT NOT NULL DEFAULT '';
-- The file as it was when fingerprinted (bytes, mtime seconds).
ALTER TABLE tracks ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN file_mtime INTEGER NOT NULL DEFAULT 0;
-- Last change (unix ms), 0 = never changed here.
ALTER TABLE tracks ADD COLUMN meta_modified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN cues_modified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN grid_modified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN anlz_modified INTEGER NOT NULL DEFAULT 0;
CREATE INDEX tracks_fingerprint ON tracks(fingerprint);
ALTER TABLE playlists ADD COLUMN uid TEXT NOT NULL DEFAULT '';
ALTER TABLE playlists ADD COLUMN modified INTEGER NOT NULL DEFAULT 0;
UPDATE playlists SET uid = lower(hex(randomblob(16)));
CREATE INDEX playlists_uid ON playlists(uid);
CREATE TABLE sync_tombstones (kind TEXT NOT NULL, uid TEXT NOT NULL, deleted INTEGER NOT NULL, PRIMARY KEY (kind, uid));
INSERT OR IGNORE INTO meta(key, value) VALUES ('library_uid', lower(hex(randomblob(16))));

CREATE TRIGGER tracks_meta_stamp AFTER UPDATE OF {cols} ON tracks WHEN {changed}
BEGIN {meta} END;
CREATE TRIGGER tracks_anlz_stamp AFTER UPDATE OF anlz ON tracks WHEN OLD.anlz IS NOT NEW.anlz
BEGIN {anlz} END;
CREATE TRIGGER cues_insert_stamp AFTER INSERT ON cues BEGIN {cues_new} END;
CREATE TRIGGER cues_delete_stamp AFTER DELETE ON cues BEGIN {cues_old} END;
CREATE TRIGGER cues_update_stamp AFTER UPDATE ON cues BEGIN {cues_new} END;
CREATE TRIGGER grids_insert_stamp AFTER INSERT ON grids BEGIN {grid_new} END;
CREATE TRIGGER grids_delete_stamp AFTER DELETE ON grids BEGIN {grid_old} END;
CREATE TRIGGER grids_update_stamp AFTER UPDATE ON grids BEGIN {grid_new} END;

CREATE TRIGGER playlists_uid AFTER INSERT ON playlists WHEN NEW.uid = ''
BEGIN UPDATE playlists SET uid = lower(hex(randomblob(16))) WHERE id = NEW.id; END;
-- A playlist back from the other side is no longer deleted.
CREATE TRIGGER playlists_revived AFTER INSERT ON playlists WHEN NEW.uid != ''
BEGIN DELETE FROM sync_tombstones WHERE kind = 'playlist' AND uid = NEW.uid; END;
CREATE TRIGGER playlists_stamp AFTER UPDATE OF name, parent_id, is_folder, is_set, sort_order ON playlists
WHEN OLD.name IS NOT NEW.name OR OLD.parent_id IS NOT NEW.parent_id OR OLD.is_folder IS NOT NEW.is_folder
  OR OLD.is_set IS NOT NEW.is_set OR OLD.sort_order IS NOT NEW.sort_order
BEGIN {playlist} END;
CREATE TRIGGER playlists_tombstone AFTER DELETE ON playlists WHEN OLD.uid != ''
BEGIN INSERT OR REPLACE INTO sync_tombstones(kind, uid, deleted) VALUES ('playlist', OLD.uid, {NOW_MS}); END;
CREATE TRIGGER items_insert_stamp AFTER INSERT ON playlist_items BEGIN {items_new} END;
CREATE TRIGGER items_delete_stamp AFTER DELETE ON playlist_items BEGIN {items_old} END;
",
        cols = META_COLUMNS.join(", "),
        meta = stamp("tracks", "meta_modified", "NEW.id"),
        anlz = stamp("tracks", "anlz_modified", "NEW.id"),
        cues_new = stamp("tracks", "cues_modified", "NEW.track_id"),
        cues_old = stamp("tracks", "cues_modified", "OLD.track_id"),
        grid_new = stamp("tracks", "grid_modified", "NEW.track_id"),
        grid_old = stamp("tracks", "grid_modified", "OLD.track_id"),
        playlist = stamp("playlists", "modified", "NEW.id"),
        items_new = stamp("playlists", "modified", "NEW.playlist_id"),
        items_old = stamp("playlists", "modified", "OLD.playlist_id"),
    )]
}

/// Bring an opened library up to the current schema.
pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let have: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in migrations().iter().enumerate().skip(have) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

/// BLAKE3 of a whole file, hex.
pub fn fingerprint_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

/// A track's file as last fingerprinted.
#[derive(Debug, Clone, PartialEq)]
pub struct FileStamp {
    pub id: i64,
    pub path: String,
    pub fingerprint: String,
    pub size: u64,
    /// Modification time, unix seconds.
    pub mtime: i64,
}

impl FileStamp {
    /// The file on disk is not the one fingerprinted (or none was).
    pub fn stale(&self, size: u64, mtime: i64) -> bool {
        self.fingerprint.is_empty() || self.size != size || self.mtime != mtime
    }
}

/// One track in a [`Manifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackEntry {
    pub fingerprint: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds.
    pub duration: f64,
    pub size: u64,
    pub meta_modified: i64,
    pub cues_modified: i64,
    pub grid_modified: i64,
    pub anlz_modified: i64,
}

/// One playlist, folder or set in a [`Manifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub uid: String,
    /// `None` at the top level.
    pub parent: Option<String>,
    pub name: String,
    pub is_folder: bool,
    pub is_set: bool,
    pub sort_order: i64,
    pub modified: i64,
    /// Fingerprints in playlist order.
    pub tracks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tombstone {
    pub kind: String,
    pub uid: String,
    pub deleted: i64,
}

/// Everything one library tells another to compare them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub library_uid: String,
    pub tracks: Vec<TrackEntry>,
    pub playlists: Vec<PlaylistEntry>,
    pub tombstones: Vec<Tombstone>,
    /// Tracks not fingerprinted yet, so left out of `tracks` and the
    /// playlists: a comparison now would be incomplete.
    pub pending: usize,
}

impl Library {
    /// This library's own uid; the library on another Mac has its own.
    pub fn library_uid(&self) -> Result<String> {
        Ok(self.meta("library_uid")?.unwrap_or_default())
    }

    /// Every track's file as last fingerprinted.
    pub fn file_stamps(&self) -> Result<Vec<FileStamp>> {
        let mut st = self
            .conn
            .prepare("SELECT id, path, fingerprint, file_size, file_mtime FROM tracks ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                Ok(FileStamp {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    fingerprint: r.get(2)?,
                    size: r.get::<_, i64>(3)? as u64,
                    mtime: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn set_fingerprint(&self, id: i64, fingerprint: &str, size: u64, mtime: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET fingerprint = ?2, file_size = ?3, file_mtime = ?4 WHERE id = ?1",
            params![id, fingerprint, size as i64, mtime],
        )?;
        Ok(())
    }

    /// (fingerprinted, all) tracks.
    pub fn fingerprint_progress(&self) -> Result<(usize, usize)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FILTER (WHERE fingerprint != ''), COUNT(*) FROM tracks",
            [],
            |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, i64>(1)? as usize)),
        )?)
    }

    pub fn tombstones(&self) -> Result<Vec<Tombstone>> {
        let mut st = self
            .conn
            .prepare("SELECT kind, uid, deleted FROM sync_tombstones ORDER BY deleted")?;
        let rows = st
            .query_map([], |r| {
                Ok(Tombstone {
                    kind: r.get(0)?,
                    uid: r.get(1)?,
                    deleted: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn track_by_fingerprint(&self, fingerprint: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM tracks WHERE fingerprint = ?1 ORDER BY id LIMIT 1",
                [fingerprint],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn manifest(&self) -> Result<Manifest> {
        let mut fingerprints: HashMap<i64, String> = HashMap::new();
        let mut tracks = Vec::new();
        let mut pending = 0;
        let mut st = self.conn.prepare(
            "SELECT id, fingerprint, title, artist, album, duration, file_size, \
             meta_modified, cues_modified, grid_modified, anlz_modified FROM tracks ORDER BY id",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let fingerprint: String = r.get(1)?;
            if fingerprint.is_empty() {
                pending += 1;
                continue;
            }
            // The same file twice in the library is one track to the other side.
            if fingerprints.values().any(|f| *f == fingerprint) {
                fingerprints.insert(r.get(0)?, fingerprint);
                continue;
            }
            fingerprints.insert(r.get(0)?, fingerprint.clone());
            tracks.push(TrackEntry {
                fingerprint,
                title: r.get(2)?,
                artist: r.get(3)?,
                album: r.get(4)?,
                duration: r.get(5)?,
                size: r.get::<_, i64>(6)? as u64,
                meta_modified: r.get(7)?,
                cues_modified: r.get(8)?,
                grid_modified: r.get(9)?,
                anlz_modified: r.get(10)?,
            });
        }

        let mut st = self.conn.prepare("SELECT id, uid, modified FROM playlists")?;
        let stamps: HashMap<i64, (String, i64)> = st
            .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<rusqlite::Result<_>>()?;
        let playlists = self
            .playlists()?
            .into_iter()
            .map(|p| {
                let (uid, modified) = stamps.get(&p.id).cloned().unwrap_or_default();
                PlaylistEntry {
                    uid,
                    parent: stamps.get(&p.parent_id).map(|s| s.0.clone()),
                    name: p.name,
                    is_folder: p.is_folder,
                    is_set: p.is_set,
                    sort_order: p.sort_order,
                    modified,
                    tracks: p.track_ids.iter().filter_map(|t| fingerprints.get(t).cloned()).collect(),
                }
            })
            .collect();

        let tombstones = self.tombstones()?;

        Ok(Manifest {
            library_uid: self.library_uid()?,
            tracks,
            playlists,
            tombstones,
            pending,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pioneer_formats::anlz::CuePoint;

    fn stamps(lib: &mut Library, id: i64) -> (i64, i64, i64, i64) {
        lib.conn()
            .query_row(
                "SELECT meta_modified, cues_modified, grid_modified, anlz_modified FROM tracks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }

    fn library() -> Library {
        let mut lib = Library::in_memory().unwrap();
        for i in 1..=3 {
            lib.conn()
                .execute(
                    "INSERT INTO tracks(path, title, duration) VALUES (?1, ?2, 300)",
                    params![format!("/m/{i}.mp3"), format!("T{i}")],
                )
                .unwrap();
        }
        lib
    }

    #[test]
    fn migrates_once_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.sqlite");
        let uid = {
            let lib = Library::open(&path).unwrap();
            lib.create_playlist(0, "Gigy", true, false).unwrap();
            lib.library_uid().unwrap()
        };
        let lib = Library::open(&path).unwrap();
        assert_eq!(uid.len(), 32);
        assert_eq!(lib.library_uid().unwrap(), uid);
        assert_eq!(lib.manifest().unwrap().playlists.len(), 1);
    }

    #[test]
    fn changes_are_stamped() {
        let mut lib = library();
        assert_eq!(stamps(&mut lib, 1), (0, 0, 0, 0));

        // Writing the same values is no edit.
        lib.conn().execute("UPDATE tracks SET title = title, rating = rating", []).unwrap();
        assert_eq!(stamps(&mut lib, 1).0, 0);

        lib.conn().execute("UPDATE tracks SET rating = 5 WHERE id = 1", []).unwrap();
        lib.set_cues(2, &[CuePoint::hot(1, 1000)]).unwrap();
        lib.set_grid(3, 128.0, 50.0).unwrap();
        lib.set_analysis(1, "/a/ANLZ0000.DAT", None, None, 0.0).unwrap();
        let (meta, cues, grid, anlz) = stamps(&mut lib, 1);
        assert!(meta > 1_700_000_000_000 && anlz > 0 && cues == 0 && grid == 0);
        assert!(stamps(&mut lib, 2).1 > 0);
        // The grid's tempo is the track's: both changed.
        let (meta, _, grid, _) = stamps(&mut lib, 3);
        assert!(grid > 0 && meta > 0);
        // A fingerprint is no edit.
        lib.set_fingerprint(2, "ab", 10, 20).unwrap();
        assert_eq!(stamps(&mut lib, 2).0, 0);
    }

    #[test]
    fn playlists_get_uids_and_leave_tombstones() {
        let mut lib = library();
        let folder = lib.create_playlist(0, "Gigy", true, false).unwrap();
        let set = lib.create_playlist(folder, "Klub X", false, true).unwrap();
        lib.set_playlist_tracks(set, &[2, 1]).unwrap();
        for (id, fp) in [(1, "f1"), (2, "f2"), (3, "f3")] {
            lib.set_fingerprint(id, fp, 1, 1).unwrap();
        }

        let m = lib.manifest().unwrap();
        assert_eq!(m.pending, 0);
        assert_eq!(m.tracks.len(), 3);
        let f = m.playlists.iter().find(|p| p.is_folder).unwrap();
        let s = m.playlists.iter().find(|p| p.is_set).unwrap();
        assert_eq!(f.uid.len(), 32);
        assert_ne!(f.uid, s.uid);
        assert_eq!(f.parent, None);
        assert_eq!(s.parent.as_deref(), Some(f.uid.as_str()));
        assert_eq!(s.tracks, ["f2", "f1"]);
        assert!(s.modified > 0);

        lib.delete_playlist(folder).unwrap();
        let m = lib.manifest().unwrap();
        assert!(m.playlists.is_empty());
        let mut gone: Vec<&str> = m.tombstones.iter().map(|t| t.uid.as_str()).collect();
        gone.sort();
        let mut want = [f.uid.as_str(), s.uid.as_str()];
        want.sort();
        assert_eq!(gone, want);

        // Coming back from the other Mac undoes the tombstone.
        lib.conn()
            .execute(
                "INSERT INTO playlists(name, uid) VALUES ('Gigy', ?1)",
                [&f.uid],
            )
            .unwrap();
        assert_eq!(lib.manifest().unwrap().tombstones.len(), 1);
    }

    #[test]
    fn unfingerprinted_tracks_are_pending() {
        let lib = library();
        lib.set_fingerprint(1, "f1", 1, 1).unwrap();
        // The same file added twice is one track.
        lib.set_fingerprint(2, "f1", 1, 1).unwrap();
        let m = lib.manifest().unwrap();
        assert_eq!((m.tracks.len(), m.pending), (1, 1));
        assert_eq!(lib.track_by_fingerprint("f1").unwrap(), Some(1));
        assert_eq!(lib.fingerprint_progress().unwrap(), (2, 3));
    }

    #[test]
    fn fingerprints_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        std::fs::write(&a, b"abc").unwrap();
        assert_eq!(
            fingerprint_file(&a).unwrap(),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
        let s = FileStamp { id: 1, path: String::new(), fingerprint: "x".into(), size: 3, mtime: 9 };
        assert!(!s.stale(3, 9));
        assert!(s.stale(4, 9));
    }
}
