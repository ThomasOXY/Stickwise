//! The local library: every track the DJ owns, playlists and sets, cue
//! points and where each track's analysis lives. Audio files stay where
//! they are; the library only points at them.
//!
//! Stored in one SQLite file. Tracks and playlists imported from rekordbox
//! carry its ids so a later import updates them instead of duplicating;
//! playlists made here and cues edited here are never overwritten by an
//! import.

pub mod folders;
mod import;
pub mod peer;

use std::path::Path;

use pioneer_formats::anlz::CuePoint;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub use folders::{FolderReport, Scan};
pub use import::{ImportReport, RekordboxDump};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    /// Absolute path of the audio file.
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub key: String,
    pub bpm: f64,
    /// Seconds.
    pub duration: f64,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub sample_depth: u32,
    pub rating: u8,
    /// rekordbox colour row 1..=8, 0 = none.
    pub color: u8,
    pub comment: String,
    pub year: u32,
    pub track_number: u32,
    pub date_added: String,
    /// `ANLZ0000.DAT` of this track (rekordbox's analysis), empty if none.
    pub anlz: String,
    pub artwork: String,
    pub rb_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    /// 0 at the top level.
    pub parent_id: i64,
    pub name: String,
    pub is_folder: bool,
    /// A DJ set (prepared in this app), shown apart from plain playlists.
    pub is_set: bool,
    pub sort_order: i64,
    /// `local` (made here) or `rekordbox` (imported).
    pub source: String,
    pub track_ids: Vec<i64>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tracks (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL DEFAULT '', artist TEXT NOT NULL DEFAULT '', album TEXT NOT NULL DEFAULT '',
  genre TEXT NOT NULL DEFAULT '', label TEXT NOT NULL DEFAULT '', remixer TEXT NOT NULL DEFAULT '',
  key TEXT NOT NULL DEFAULT '', bpm REAL NOT NULL DEFAULT 0, duration REAL NOT NULL DEFAULT 0,
  bitrate INTEGER NOT NULL DEFAULT 0, sample_rate INTEGER NOT NULL DEFAULT 0, sample_depth INTEGER NOT NULL DEFAULT 0,
  rating INTEGER NOT NULL DEFAULT 0, color INTEGER NOT NULL DEFAULT 0, comment TEXT NOT NULL DEFAULT '',
  year INTEGER NOT NULL DEFAULT 0, track_number INTEGER NOT NULL DEFAULT 0, date_added TEXT NOT NULL DEFAULT '',
  anlz TEXT NOT NULL DEFAULT '', artwork TEXT NOT NULL DEFAULT '',
  rb_id TEXT NOT NULL DEFAULT '',
  -- 'rekordbox' until the cues are edited here, then 'local' (imports keep them).
  cues_source TEXT NOT NULL DEFAULT 'rekordbox'
);
CREATE INDEX IF NOT EXISTS tracks_rb ON tracks(rb_id);
CREATE TABLE IF NOT EXISTS cues (
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  slot INTEGER NOT NULL, time_ms INTEGER NOT NULL, loop_end_ms INTEGER,
  loop_num INTEGER, loop_den INTEGER, comment TEXT NOT NULL DEFAULT '',
  hot_color INTEGER NOT NULL DEFAULT 0, memory_color INTEGER NOT NULL DEFAULT 0,
  active_loop INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS cues_track ON cues(track_id);
CREATE TABLE IF NOT EXISTS playlists (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER NOT NULL DEFAULT 0,
  name TEXT NOT NULL,
  is_folder INTEGER NOT NULL DEFAULT 0,
  is_set INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL DEFAULT 'local',
  rb_id TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS playlist_items (
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS items_playlist ON playlist_items(playlist_id, position);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- Beat grids edited by the DJ: tempo and one bar one (ms), over the analysis.
CREATE TABLE IF NOT EXISTS grids (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  bpm REAL NOT NULL,
  anchor_ms REAL NOT NULL
);
-- Tracks whose analysis failed, for the file as it was (mtime, seconds).
CREATE TABLE IF NOT EXISTS analysis_failures (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  mtime INTEGER NOT NULL
);
-- Cues a track had before a set plan replaced them (restorable).
CREATE TABLE IF NOT EXISTS cue_backups (track_id INTEGER PRIMARY KEY, cues TEXT NOT NULL, saved TEXT NOT NULL);
";

const TRACK_COLUMNS: &str = "id, path, title, artist, album, genre, label, remixer, key, bpm, duration, bitrate, \
    sample_rate, sample_depth, rating, color, comment, year, track_number, date_added, anlz, artwork, rb_id";

fn track_from_row(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        genre: r.get(5)?,
        label: r.get(6)?,
        remixer: r.get(7)?,
        key: r.get(8)?,
        bpm: r.get(9)?,
        duration: r.get(10)?,
        bitrate: r.get(11)?,
        sample_rate: r.get(12)?,
        sample_depth: r.get(13)?,
        rating: r.get(14)?,
        color: r.get(15)?,
        comment: r.get(16)?,
        year: r.get(17)?,
        track_number: r.get(18)?,
        date_added: r.get(19)?,
        anlz: r.get(20)?,
        artwork: r.get(21)?,
        rb_id: r.get(22)?,
    })
}

/// Track id → hot cues as (slot, colour code, is loop).
pub type HotCueCodes = std::collections::HashMap<i64, Vec<(u8, u8, bool)>>;

pub struct Library {
    conn: Connection,
}

impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        peer::migrate(&mut conn)?;
        Ok(Library { conn })
    }

    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        peer::migrate(&mut conn)?;
        Ok(Library { conn })
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn clear_meta(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM meta WHERE key = ?1", [key])?;
        Ok(())
    }

    pub fn tracks(&self) -> Result<Vec<Track>> {
        let mut st = self
            .conn
            .prepare(&format!("SELECT {TRACK_COLUMNS} FROM tracks ORDER BY id"))?;
        let rows = st
            .query_map([], track_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn track(&self, id: i64) -> Result<Option<Track>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                [id],
                track_from_row,
            )
            .optional()?)
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut st = self.conn.prepare(
            "SELECT id, parent_id, name, is_folder, is_set, sort_order, source FROM playlists ORDER BY parent_id, sort_order, id",
        )?;
        let mut lists: Vec<Playlist> = st
            .query_map([], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    parent_id: r.get(1)?,
                    name: r.get(2)?,
                    is_folder: r.get::<_, i64>(3)? != 0,
                    is_set: r.get::<_, i64>(4)? != 0,
                    sort_order: r.get(5)?,
                    source: r.get(6)?,
                    track_ids: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut st = self.conn.prepare(
            "SELECT playlist_id, track_id FROM playlist_items ORDER BY playlist_id, position",
        )?;
        let items: Vec<(i64, i64)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let index: std::collections::HashMap<i64, usize> =
            lists.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        for (pl, t) in items {
            if let Some(&i) = index.get(&pl) {
                lists[i].track_ids.push(t);
            }
        }
        Ok(lists)
    }

    fn next_sort(&self, parent: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM playlists WHERE parent_id = ?1",
            [parent],
            |r| r.get(0),
        )?)
    }

    pub fn create_playlist(
        &self,
        parent_id: i64,
        name: &str,
        is_folder: bool,
        is_set: bool,
    ) -> Result<i64> {
        let sort = self.next_sort(parent_id)?;
        self.conn.execute(
            "INSERT INTO playlists(parent_id, name, is_folder, is_set, sort_order, source) VALUES (?1, ?2, ?3, ?4, ?5, 'local')",
            params![parent_id, name, is_folder, is_set, sort],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        // Renaming makes it ours: a later rekordbox import must not undo it.
        self.conn.execute(
            "UPDATE playlists SET name = ?2, source = 'local' WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Delete a playlist or a folder with everything inside it.
    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        let children: Vec<i64> = {
            let mut st = self
                .conn
                .prepare("SELECT id FROM playlists WHERE parent_id = ?1")?;
            st.query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for c in children {
            self.delete_playlist(c)?;
        }
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Move a playlist under `parent_id` at position `index` among its siblings.
    pub fn move_playlist(&mut self, id: i64, parent_id: i64, index: usize) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut siblings: Vec<i64> = {
            let mut st = tx.prepare("SELECT id FROM playlists WHERE parent_id = ?1 AND id != ?2 ORDER BY sort_order, id")?;
            st.query_map(params![parent_id, id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        siblings.insert(index.min(siblings.len()), id);
        tx.execute(
            "UPDATE playlists SET parent_id = ?2, source = 'local' WHERE id = ?1",
            params![id, parent_id],
        )?;
        for (i, s) in siblings.iter().enumerate() {
            tx.execute(
                "UPDATE playlists SET sort_order = ?2 WHERE id = ?1",
                params![s, i as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The DJ's edited beat grid of a track: (bpm, anchor ms of a bar one).
    pub fn grid(&self, track_id: i64) -> Result<Option<(f64, f64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT bpm, anchor_ms FROM grids WHERE track_id = ?1",
                [track_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Keep an edited grid; the track's tempo follows it.
    pub fn set_grid(&mut self, track_id: i64, bpm: f64, anchor_ms: f64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO grids(track_id, bpm, anchor_ms) VALUES (?1, ?2, ?3) \
             ON CONFLICT(track_id) DO UPDATE SET bpm = ?2, anchor_ms = ?3",
            params![track_id, bpm, anchor_ms],
        )?;
        tx.execute("UPDATE tracks SET bpm = ?2 WHERE id = ?1", params![track_id, (bpm * 100.0).round() / 100.0])?;
        tx.commit()?;
        Ok(())
    }

    /// Back to the analysed grid, with its tempo.
    pub fn clear_grid(&mut self, track_id: i64, bpm: Option<f64>) -> Result<()> {
        self.conn.execute("DELETE FROM grids WHERE track_id = ?1", [track_id])?;
        if let Some(b) = bpm {
            self.conn.execute("UPDATE tracks SET bpm = ?2 WHERE id = ?1", params![track_id, b])?;
        }
        Ok(())
    }

    /// Our own analysis of a track: its `ANLZ0000.DAT`, the grid's tempo,
    /// the key (only where the tags gave none) and the length if unknown.
    pub fn set_analysis(&mut self, id: i64, anlz: &str, bpm: Option<f64>, key: Option<&str>, duration: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET anlz = ?2, bpm = COALESCE(?3, bpm), \
             key = CASE WHEN key = '' THEN COALESCE(?4, '') ELSE key END, \
             duration = CASE WHEN duration = 0 THEN ?5 ELSE duration END WHERE id = ?1",
            params![id, anlz, bpm, key, duration],
        )?;
        // A new analysis is asked for: its grid replaces an edited one.
        self.conn.execute("DELETE FROM grids WHERE track_id = ?1", [id])?;
        self.conn.execute("DELETE FROM analysis_failures WHERE track_id = ?1", [id])?;
        Ok(())
    }

    /// Remember that analysing a track failed, for its file as it is now
    /// (modification time, seconds).
    pub fn set_analysis_failed(&self, id: i64, mtime: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO analysis_failures(track_id, mtime) VALUES (?1, ?2) \
             ON CONFLICT(track_id) DO UPDATE SET mtime = excluded.mtime",
            params![id, mtime as i64],
        )?;
        Ok(())
    }

    /// Analysing this track failed before and its file has not changed.
    pub fn analysis_failed(&self, id: i64, mtime: u64) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM analysis_failures WHERE track_id = ?1 AND mtime = ?2",
                params![id, mtime as i64],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Give siblings their order: `ids[0]` first. Others under the same
    /// parent keep their relative order after them.
    pub fn order_playlists(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE playlists SET sort_order = ?2, source = 'local' WHERE id = ?1",
                params![id, i as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Replace the tracks of a playlist (add, remove and reorder in one go).
    pub fn set_playlist_tracks(&mut self, id: i64, track_ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [id])?;
        for (i, t) in track_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_items(playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
                params![id, i as i64, t],
            )?;
        }
        tx.execute("UPDATE playlists SET source = 'local' WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn cues(&self, track_id: i64) -> Result<Vec<CuePoint>> {
        let mut st = self.conn.prepare(
            "SELECT slot, time_ms, loop_end_ms, loop_num, loop_den, comment, hot_color, memory_color, active_loop \
             FROM cues WHERE track_id = ?1 ORDER BY slot = 0, slot, time_ms",
        )?;
        let rows = st
            .query_map([track_id], |r| {
                let num: Option<u16> = r.get(3)?;
                let den: Option<u16> = r.get(4)?;
                Ok(CuePoint {
                    slot: r.get(0)?,
                    time_ms: r.get(1)?,
                    loop_end_ms: r.get(2)?,
                    loop_beats: num.zip(den),
                    comment: r.get(5)?,
                    hot_color: r.get(6)?,
                    hot_led: None,
                    memory_color: r.get(7)?,
                    active_loop: r.get::<_, i64>(8)? != 0,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    fn write_cues(tx: &rusqlite::Transaction, track_id: i64, cues: &[CuePoint]) -> Result<()> {
        tx.execute("DELETE FROM cues WHERE track_id = ?1", [track_id])?;
        for c in cues {
            tx.execute(
                "INSERT INTO cues(track_id, slot, time_ms, loop_end_ms, loop_num, loop_den, comment, hot_color, memory_color, active_loop) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    track_id,
                    c.slot,
                    c.time_ms,
                    c.loop_end_ms,
                    c.loop_beats.map(|b| b.0),
                    c.loop_beats.map(|b| b.1),
                    c.comment,
                    c.hot_color,
                    c.memory_color,
                    c.active_loop
                ],
            )?;
        }
        Ok(())
    }

    /// Cues edited in the app; from now on imports leave this track's cues alone.
    pub fn set_cues(&mut self, track_id: i64, cues: &[CuePoint]) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::write_cues(&tx, track_id, cues)?;
        tx.execute(
            "UPDATE tracks SET cues_source = 'local' WHERE id = ?1",
            [track_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Keep the track's current cues so they can be restored; an existing
    /// backup is kept (it holds the cues from before the first replacement).
    pub fn backup_cues(&self, track_id: i64) -> Result<()> {
        let cues: Vec<serde_json::Value> = self
            .cues(track_id)?
            .iter()
            .map(|c| {
                serde_json::json!({
                    "slot": c.slot, "time_ms": c.time_ms, "loop_end_ms": c.loop_end_ms,
                    "loop_beats": c.loop_beats, "comment": c.comment, "hot_color": c.hot_color,
                    "memory_color": c.memory_color, "active_loop": c.active_loop
                })
            })
            .collect();
        self.conn.execute(
            "INSERT OR IGNORE INTO cue_backups(track_id, cues, saved) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![track_id, serde_json::to_string(&cues)?],
        )?;
        Ok(())
    }

    pub fn has_cue_backup(&self, track_id: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM cue_backups WHERE track_id = ?1",
            [track_id],
            |r| r.get::<_, i64>(0),
        )? > 0)
    }

    /// Put back the cues saved by [`Library::backup_cues`]; returns false if there was none.
    pub fn restore_cues(&mut self, track_id: i64) -> Result<bool> {
        let saved: Option<String> = self
            .conn
            .query_row(
                "SELECT cues FROM cue_backups WHERE track_id = ?1",
                [track_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(saved) = saved else { return Ok(false) };
        let values: Vec<serde_json::Value> = serde_json::from_str(&saved)?;
        let cues: Vec<CuePoint> = values
            .iter()
            .map(|v| CuePoint {
                slot: v["slot"].as_u64().unwrap_or(0) as u8,
                time_ms: v["time_ms"].as_u64().unwrap_or(0) as u32,
                loop_end_ms: v["loop_end_ms"].as_u64().map(|x| x as u32),
                loop_beats: v["loop_beats"]
                    .as_array()
                    .and_then(|a| Some((a.first()?.as_u64()? as u16, a.get(1)?.as_u64()? as u16))),
                comment: v["comment"].as_str().unwrap_or_default().to_string(),
                hot_color: v["hot_color"].as_u64().unwrap_or(0) as u8,
                hot_led: None,
                memory_color: v["memory_color"].as_u64().unwrap_or(0) as u8,
                active_loop: v["active_loop"].as_bool().unwrap_or(false),
            })
            .collect();
        self.set_cues(track_id, &cues)?;
        self.conn
            .execute("DELETE FROM cue_backups WHERE track_id = ?1", [track_id])?;
        Ok(true)
    }

    /// Hot cue colours of all tracks (for the table): track → (slot, colour code, is loop).
    pub fn hot_cue_codes(&self) -> Result<HotCueCodes> {
        let mut st = self.conn.prepare(
            "SELECT track_id, slot, hot_color, loop_end_ms IS NOT NULL FROM cues WHERE slot > 0",
        )?;
        let mut out = HotCueCodes::default();
        for row in st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, u8>(1)?,
                r.get::<_, u8>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })? {
            let (t, slot, code, is_loop) = row?;
            out.entry(t).or_default().push((slot, code, is_loop));
        }
        Ok(out)
    }

    pub(crate) fn conn(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlists_and_sets() {
        let mut lib = Library::in_memory().unwrap();
        let tx = lib.conn().transaction().unwrap();
        for i in 1..=3 {
            tx.execute(
                "INSERT INTO tracks(path, title) VALUES (?1, ?2)",
                params![format!("/m/{i}.mp3"), format!("T{i}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let folder = lib.create_playlist(0, "Gigy", true, false).unwrap();
        let set = lib
            .create_playlist(folder, "Klub X — sobota", false, true)
            .unwrap();
        lib.set_playlist_tracks(set, &[3, 1, 2]).unwrap();
        let p = lib.playlists().unwrap();
        let s = p.iter().find(|p| p.id == set).unwrap();
        assert!(s.is_set && !s.is_folder);
        assert_eq!(s.track_ids, vec![3, 1, 2]);
        lib.rename_playlist(set, "Klub X").unwrap();
        lib.move_playlist(set, 0, 0).unwrap();
        let p = lib.playlists().unwrap();
        assert_eq!(p.iter().find(|p| p.id == set).unwrap().parent_id, 0);
        lib.delete_playlist(folder).unwrap();
        assert_eq!(lib.playlists().unwrap().len(), 1);

        let cues = vec![
            CuePoint {
                comment: "Drop".into(),
                hot_color: 0x2a,
                ..CuePoint::hot(1, 30_000)
            },
            CuePoint::memory(500),
        ];
        lib.set_cues(1, &cues).unwrap();
        lib.backup_cues(1).unwrap();
        lib.set_cues(1, &[CuePoint::hot(1, 9)]).unwrap();
        lib.backup_cues(1).unwrap(); // keeps the first backup
        assert!(lib.has_cue_backup(1).unwrap());
        assert!(lib.restore_cues(1).unwrap());
        assert!(!lib.has_cue_backup(1).unwrap());
        let back = lib.cues(1).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].comment, "Drop");
        assert_eq!(lib.hot_cue_codes().unwrap()[&1], vec![(1, 0x2a, false)]);
    }

    #[test]
    fn failed_analysis_is_remembered_until_the_file_changes() {
        let mut lib = Library::in_memory().unwrap();
        lib.conn().execute("INSERT INTO tracks(path, title) VALUES ('/m/x.mp3', 'X')", []).unwrap();
        assert!(!lib.analysis_failed(1, 100).unwrap());
        lib.set_analysis_failed(1, 100).unwrap();
        assert!(lib.analysis_failed(1, 100).unwrap());
        // The file changed: worth another try.
        assert!(!lib.analysis_failed(1, 200).unwrap());
        // Analysed after all: forgotten.
        lib.set_analysis(1, "/a/ANLZ0000.DAT", None, None, 0.0).unwrap();
        assert!(!lib.analysis_failed(1, 100).unwrap());
    }
}
