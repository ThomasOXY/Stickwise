//! Import (and re-import) a rekordbox library dumped by
//! `sidecar/rekordbox_dump.py`.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Library, Result};

#[derive(Debug, Deserialize)]
pub struct DumpTrack {
    pub rb_id: String,
    pub path: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub remixer: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub bpm: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub bitrate: u32,
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub sample_depth: u32,
    #[serde(default)]
    pub rating: u8,
    #[serde(default)]
    pub color: u8,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub year: u32,
    #[serde(default)]
    pub track_number: u32,
    #[serde(default)]
    pub date_added: String,
    #[serde(default)]
    pub anlz: String,
    #[serde(default)]
    pub artwork: String,
}

#[derive(Debug, Deserialize)]
pub struct DumpCue {
    pub rb_id: String,
    pub slot: u8,
    pub time_ms: u32,
    pub loop_end_ms: Option<u32>,
    pub loop_beats: Option<(u16, u16)>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub hot_color: u8,
    #[serde(default)]
    pub memory_color: u8,
    #[serde(default)]
    pub active_loop: bool,
}

#[derive(Debug, Deserialize)]
pub struct DumpPlaylist {
    pub rb_id: String,
    pub parent_rb_id: String,
    pub name: String,
    pub is_folder: bool,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct DumpItem {
    pub playlist_rb_id: String,
    pub rb_id: String,
    pub position: i64,
}

#[derive(Debug, Deserialize)]
pub struct RekordboxDump {
    pub tracks: Vec<DumpTrack>,
    pub cues: Vec<DumpCue>,
    pub playlists: Vec<DumpPlaylist>,
    pub items: Vec<DumpItem>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub cues: usize,
    /// Tracks whose cues were edited in the app and were kept.
    pub cues_kept: usize,
    pub playlists: usize,
    /// rekordbox playlists changed in the app and left as they are.
    pub playlists_kept: usize,
}

impl Library {
    pub fn import_rekordbox(&mut self, dump: &RekordboxDump) -> Result<ImportReport> {
        let mut r = ImportReport::default();
        let tx = self.conn().transaction()?;

        // Tracks: match by rekordbox id, then by path.
        let mut ids: HashMap<&str, i64> = HashMap::new();
        for t in &dump.tracks {
            let Some(path) = t.path.as_deref().filter(|p| !p.is_empty()) else {
                continue;
            };
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE (rb_id = ?1 AND rb_id != '') OR path = ?2 ORDER BY rb_id = ?1 DESC LIMIT 1",
                    params![t.rb_id, path],
                    |r| r.get(0),
                )
                .optional()?;
            let values = params![
                path,
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
                t.rating,
                t.color,
                t.comment,
                t.year,
                t.track_number,
                t.date_added,
                t.anlz,
                t.artwork,
                t.rb_id
            ];
            let id = match existing {
                Some(id) => {
                    tx.execute(
                        "UPDATE tracks SET path=?1, title=?2, artist=?3, album=?4, genre=?5, label=?6, remixer=?7, key=?8, \
                         bpm=?9, duration=?10, bitrate=?11, sample_rate=?12, sample_depth=?13, rating=?14, color=?15, \
                         comment=?16, year=?17, track_number=?18, date_added=?19, anlz=?20, artwork=?21, rb_id=?22 \
                         WHERE id = ?23",
                        rusqlite::params_from_iter(values.iter().copied().chain(std::iter::once(&id as &dyn rusqlite::ToSql))),
                    )?;
                    r.tracks_updated += 1;
                    id
                }
                None => {
                    tx.execute(
                        "INSERT INTO tracks(path, title, artist, album, genre, label, remixer, key, bpm, duration, bitrate, \
                         sample_rate, sample_depth, rating, color, comment, year, track_number, date_added, anlz, artwork, rb_id) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
                        values,
                    )?;
                    r.tracks_added += 1;
                    tx.last_insert_rowid()
                }
            };
            ids.insert(t.rb_id.as_str(), id);
        }

        // Cues, unless edited here.
        let mut by_track: HashMap<i64, Vec<&DumpCue>> = HashMap::new();
        for c in &dump.cues {
            if let Some(&id) = ids.get(c.rb_id.as_str()) {
                by_track.entry(id).or_default().push(c);
            }
        }
        for &id in ids.values() {
            let source: String =
                tx.query_row("SELECT cues_source FROM tracks WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
            if source == "local" {
                if by_track.contains_key(&id) {
                    r.cues_kept += 1;
                }
                continue;
            }
            tx.execute("DELETE FROM cues WHERE track_id = ?1", [id])?;
            for c in by_track.get(&id).map(Vec::as_slice).unwrap_or_default() {
                tx.execute(
                    "INSERT INTO cues(track_id, slot, time_ms, loop_end_ms, loop_num, loop_den, comment, hot_color, memory_color, active_loop) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
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
                r.cues += 1;
            }
        }

        // Playlists: rebuild the imported ones, keep those changed here.
        let kept: HashMap<String, i64> = {
            let mut st = tx.prepare(
                "SELECT rb_id, id FROM playlists WHERE source = 'local' AND rb_id != ''",
            )?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        tx.execute("DELETE FROM playlists WHERE source = 'rekordbox'", [])?;
        let mut pl_ids: HashMap<String, i64> = kept.clone();
        pl_ids.insert("root".into(), 0);
        let mut pending: Vec<&DumpPlaylist> = dump.playlists.iter().collect();
        // Parents first; give up on anything whose parent never appears.
        for _ in 0..64 {
            let before = pending.len();
            pending.retain(|p| {
                if kept.contains_key(&p.rb_id) {
                    r.playlists_kept += 1;
                    return false;
                }
                let Some(&parent) = pl_ids.get(&p.parent_rb_id) else { return true };
                if tx
                    .execute(
                        "INSERT INTO playlists(parent_id, name, is_folder, is_set, sort_order, source, rb_id) \
                         VALUES (?1, ?2, ?3, 0, ?4, 'rekordbox', ?5)",
                        params![parent, p.name, p.is_folder, p.sort_order, p.rb_id],
                    )
                    .is_ok()
                {
                    pl_ids.insert(p.rb_id.clone(), tx.last_insert_rowid());
                    r.playlists += 1;
                }
                false
            });
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        let mut items: Vec<&DumpItem> = dump.items.iter().collect();
        items.sort_by_key(|i| (i.playlist_rb_id.clone(), i.position));
        for i in items {
            if kept.contains_key(&i.playlist_rb_id) {
                continue;
            }
            if let (Some(&pl), Some(&t)) =
                (pl_ids.get(&i.playlist_rb_id), ids.get(i.rb_id.as_str()))
            {
                tx.execute(
                    "INSERT INTO playlist_items(playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
                    params![pl, i.position, t],
                )?;
            }
        }
        tx.commit()?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> RekordboxDump {
        serde_json::from_value(serde_json::json!({
            "tracks": [
                {"rb_id": "10", "path": "/m/a.mp3", "title": "A", "bpm": 128.0},
                {"rb_id": "11", "path": "/m/b.mp3", "title": "B"},
                {"rb_id": "12", "path": null, "title": "missing path"}
            ],
            "cues": [
                {"rb_id": "10", "slot": 1, "time_ms": 1000, "loop_end_ms": null, "loop_beats": null, "hot_color": 42}
            ],
            "playlists": [
                {"rb_id": "p2", "parent_rb_id": "p1", "name": "Child", "is_folder": false, "sort_order": 0},
                {"rb_id": "p1", "parent_rb_id": "root", "name": "Folder", "is_folder": true, "sort_order": 0}
            ],
            "items": [
                {"playlist_rb_id": "p2", "rb_id": "11", "position": 2},
                {"playlist_rb_id": "p2", "rb_id": "10", "position": 1}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn import_then_reimport_keeps_local_changes() {
        let mut lib = Library::in_memory().unwrap();
        let r = lib.import_rekordbox(&dump()).unwrap();
        assert_eq!((r.tracks_added, r.cues, r.playlists), (2, 1, 2));
        let child = lib
            .playlists()
            .unwrap()
            .into_iter()
            .find(|p| p.name == "Child")
            .unwrap();
        let tracks = lib.tracks().unwrap();
        let a = tracks.iter().find(|t| t.title == "A").unwrap().id;
        let b = tracks.iter().find(|t| t.title == "B").unwrap().id;
        assert_eq!(child.track_ids, vec![a, b]);

        // Edit cues and the playlist in the app, then import again.
        lib.set_cues(a, &[pioneer_formats::anlz::CuePoint::memory(5)])
            .unwrap();
        lib.set_playlist_tracks(child.id, &[b]).unwrap();
        let mine = lib.create_playlist(0, "Můj set", false, true).unwrap();
        let r = lib.import_rekordbox(&dump()).unwrap();
        assert_eq!(
            (
                r.tracks_added,
                r.tracks_updated,
                r.cues_kept,
                r.playlists_kept
            ),
            (0, 2, 1, 1)
        );
        assert_eq!(lib.cues(a).unwrap().len(), 1);
        assert_eq!(lib.cues(a).unwrap()[0].slot, 0, "local cue edit survived");
        let lists = lib.playlists().unwrap();
        assert_eq!(
            lists.iter().find(|p| p.name == "Child").unwrap().track_ids,
            vec![b]
        );
        assert!(lists.iter().any(|p| p.id == mine));
        assert_eq!(
            lists.iter().filter(|p| p.name == "Child").count(),
            1,
            "no duplicate after re-import"
        );
    }
}
