//! Read an Engine database (read-only) for checks and inspection.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::blobs::{BeatData, Loops, Overview, QuickCues, TrackData};
use crate::{Result, Schema};

#[derive(Debug, Clone)]
pub struct ReadTrack {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub bpm: f64,
    pub key: Option<i32>,
    pub track_data: TrackData,
    pub beats: BeatData,
    pub cues: QuickCues,
    pub loops: Loops,
    pub overview: Option<Overview>,
}

#[derive(Debug, Clone)]
pub struct ReadPlaylist {
    pub id: i64,
    pub title: String,
    pub parent: i64,
    pub next: i64,
    /// Track ids in playlist order.
    pub tracks: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct ReadLibrary {
    /// `None` for a schema this crate does not write.
    pub schema: Option<Schema>,
    pub version: (i64, i64, i64),
    pub uuid: String,
    pub tracks: Vec<ReadTrack>,
    pub playlists: Vec<ReadPlaylist>,
}

impl ReadLibrary {
    /// Children of `parent` (0 = top) in their linked-list order.
    pub fn children(&self, parent: i64) -> Vec<&ReadPlaylist> {
        let group: Vec<&ReadPlaylist> = self
            .playlists
            .iter()
            .filter(|p| p.parent == parent)
            .collect();
        let pointed: HashSet<i64> = group.iter().map(|p| p.next).collect();
        let by_id: HashMap<i64, &ReadPlaylist> = group.iter().map(|p| (p.id, *p)).collect();
        let mut out = Vec::new();
        let mut at = group
            .iter()
            .find(|p| !pointed.contains(&p.id))
            .map(|p| p.id);
        while let Some(id) = at {
            let Some(p) = by_id.get(&id) else { break };
            if out.len() > group.len() {
                break;
            }
            out.push(*p);
            at = (p.next != 0).then_some(p.next);
        }
        out
    }
}

/// Only the version and identity, cheaply.
pub fn info(path: &Path) -> Result<((i64, i64, i64), String)> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(c.query_row(
        "SELECT schemaVersionMajor, schemaVersionMinor, schemaVersionPatch, uuid FROM Information LIMIT 1",
        [],
        |r| Ok(((r.get(0)?, r.get(1)?, r.get(2)?), r.get::<_, Option<String>>(3)?.unwrap_or_default())),
    )?)
}

pub fn read(path: &Path) -> Result<ReadLibrary> {
    let (version, uuid) = info(path)?;
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let blob = |r: &rusqlite::Row, i: usize| -> rusqlite::Result<Vec<u8>> {
        Ok(r.get::<_, Option<Vec<u8>>>(i)?.unwrap_or_default())
    };
    let mut st = c.prepare(
        "SELECT t.id, t.path, t.title, t.artist, t.bpmAnalyzed, t.key, p.trackData, p.beatData, \
         p.quickCues, p.loops, p.overviewWaveFormData FROM Track t \
         LEFT JOIN PerformanceData p ON p.trackId = t.id ORDER BY t.id",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<f64>>(4)?.unwrap_or_default(),
                r.get::<_, Option<i32>>(5)?,
                [
                    blob(r, 6)?,
                    blob(r, 7)?,
                    blob(r, 8)?,
                    blob(r, 9)?,
                    blob(r, 10)?,
                ],
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut tracks = Vec::new();
    for (id, path, title, artist, bpm, key, [td, bd, qc, lp, ov]) in rows {
        tracks.push(ReadTrack {
            id,
            path,
            title,
            artist,
            bpm,
            key,
            track_data: TrackData::decode(&td)?,
            beats: BeatData::decode(&bd)?,
            cues: QuickCues::decode(&qc)?,
            loops: Loops::decode(&lp)?,
            overview: if ov.is_empty() {
                None
            } else {
                Some(Overview::decode(&ov)?)
            },
        });
    }

    let mut entities: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut st = c.prepare("SELECT listId, id, trackId, nextEntityId FROM PlaylistEntity")?;
    for row in st.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })? {
        let (list, id, track, next) = row?;
        entities.entry(list).or_default().push((id, track, next));
    }
    let mut st =
        c.prepare("SELECT id, title, parentListId, nextListId FROM Playlist ORDER BY id")?;
    let playlists = st
        .query_map([], |r| {
            Ok(ReadPlaylist {
                id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                parent: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                next: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                tracks: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|mut p| {
            p.tracks = chain(entities.get(&p.id).map(Vec::as_slice).unwrap_or(&[]));
            p
        })
        .collect();
    Ok(ReadLibrary {
        schema: Schema::from_version(version),
        version,
        uuid,
        tracks,
        playlists,
    })
}

/// Walk `nextEntityId` from the head (the entry nobody points at).
fn chain(rows: &[(i64, i64, i64)]) -> Vec<i64> {
    let pointed: HashSet<i64> = rows.iter().map(|r| r.2).collect();
    let by_id: HashMap<i64, &(i64, i64, i64)> = rows.iter().map(|r| (r.0, r)).collect();
    let mut out = Vec::new();
    let mut at = rows.iter().find(|r| !pointed.contains(&r.0)).map(|r| r.0);
    while let Some(id) = at {
        let Some(r) = by_id.get(&id) else { break };
        if out.len() > rows.len() {
            break;
        }
        out.push(r.1);
        at = (r.2 != 0).then_some(r.2);
    }
    out
}
