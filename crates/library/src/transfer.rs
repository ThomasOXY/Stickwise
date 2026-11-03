//! Part of a library sent to another Mac: the chosen playlists, sets and
//! tracks with their tags, cues and grids ([`Offer`]), what the receiving
//! library lacks of it ([`Want`]), and taking it in ([`Library::apply_offer`]).
//!
//! Taking it in follows the sync rules: a track the receiver already has
//! (same fingerprint) keeps its file; its tags, cues, grid and analysis are
//! replaced only by a newer edit, and replaced cues go to the cue backup.
//! Playlists are matched by uid, likewise newer wins.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use pioneer_formats::anlz::CuePoint;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::peer::{PlaylistEntry, TrackEntry};
use crate::{Error, Library, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CueData {
    pub slot: u8,
    pub time_ms: u32,
    pub loop_end_ms: Option<u32>,
    pub loop_beats: Option<(u16, u16)>,
    pub comment: String,
    pub hot_color: u8,
    pub memory_color: u8,
    pub active_loop: bool,
}

impl From<&CuePoint> for CueData {
    fn from(c: &CuePoint) -> Self {
        CueData {
            slot: c.slot,
            time_ms: c.time_ms,
            loop_end_ms: c.loop_end_ms,
            loop_beats: c.loop_beats,
            comment: c.comment.clone(),
            hot_color: c.hot_color,
            memory_color: c.memory_color,
            active_loop: c.active_loop,
        }
    }
}

impl From<&CueData> for CuePoint {
    fn from(c: &CueData) -> Self {
        CuePoint {
            slot: c.slot,
            time_ms: c.time_ms,
            loop_end_ms: c.loop_end_ms,
            loop_beats: c.loop_beats,
            comment: c.comment.clone(),
            hot_color: c.hot_color,
            hot_led: None,
            memory_color: c.memory_color,
            active_loop: c.active_loop,
        }
    }
}

/// Tags beyond those in [`TrackEntry`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tags {
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub key: String,
    pub bpm: f64,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub sample_depth: u32,
    pub rating: u8,
    pub color: u8,
    pub comment: String,
    pub year: u32,
    pub track_number: u32,
    pub date_added: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackData {
    pub entry: TrackEntry,
    pub tags: Tags,
    /// Name of the audio file on the sending Mac.
    pub file_name: String,
    pub cues: Vec<CueData>,
    /// The DJ's edited grid: (bpm, anchor ms).
    pub grid: Option<(f64, f64)>,
    /// Analysis files the sender has: `DAT`, `EXT`, `2EX`.
    pub anlz: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub library_uid: String,
    /// The chosen playlists, what is inside chosen folders, and the folders
    /// above them (so they land in the same place).
    pub playlists: Vec<PlaylistEntry>,
    pub tracks: Vec<TrackData>,
}

impl Offer {
    pub fn bytes(&self) -> u64 {
        self.tracks.iter().map(|t| t.entry.size).sum()
    }
}

/// What the receiver asks to be sent of one track.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Want {
    pub fingerprint: String,
    pub audio: bool,
    pub anlz: bool,
}

/// A track's files as they arrived.
#[derive(Debug, Clone, Default)]
pub struct Received {
    /// The audio file, `None` when only the analysis came.
    pub audio: Option<(String, u64, i64)>,
    /// `ANLZ0000.DAT` (its siblings next to it).
    pub anlz: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub cues_updated: usize,
    pub playlists_added: usize,
    pub playlists_updated: usize,
    /// Tracks that did not arrive: "Artist – Title".
    pub skipped: Vec<String>,
}

const TAG_COLUMNS: &str = "title = ?2, artist = ?3, album = ?4, genre = ?5, label = ?6, remixer = ?7, key = ?8, \
    bpm = ?9, rating = ?10, color = ?11, comment = ?12, year = ?13, track_number = ?14";

fn label(t: &TrackData) -> String {
    match (t.entry.artist.as_str(), t.entry.title.as_str()) {
        ("", title) => title.to_string(),
        (artist, title) => format!("{artist} – {title}"),
    }
}

/// Analysis files next to `dat` (itself included) that exist.
fn anlz_files(dat: &str) -> Vec<String> {
    if dat.is_empty() || !Path::new(dat).is_file() {
        return Vec::new();
    }
    ["DAT", "EXT", "2EX"]
        .into_iter()
        .filter(|e| Path::new(dat).with_extension(e).is_file())
        .map(String::from)
        .collect()
}

struct LocalStamps {
    id: i64,
    meta: i64,
    cues: i64,
    grid: i64,
    anlz: i64,
    anlz_path: String,
}

fn local_by_fingerprint(tx: &rusqlite::Connection, fp: &str) -> Result<Option<LocalStamps>> {
    Ok(tx
        .query_row(
            "SELECT id, meta_modified, cues_modified, grid_modified, anlz_modified, anlz FROM tracks \
             WHERE fingerprint = ?1 ORDER BY id LIMIT 1",
            [fp],
            |r| {
                Ok(LocalStamps {
                    id: r.get(0)?,
                    meta: r.get(1)?,
                    cues: r.get(2)?,
                    grid: r.get(3)?,
                    anlz: r.get(4)?,
                    anlz_path: r.get(5)?,
                })
            },
        )
        .optional()?)
}

fn set_stamp(tx: &Transaction, id: i64, column: &str, value: i64) -> Result<()> {
    tx.execute(&format!("UPDATE tracks SET {column} = ?2 WHERE id = ?1"), params![id, value])?;
    Ok(())
}

fn write_grid(tx: &Transaction, id: i64, grid: Option<(f64, f64)>) -> Result<()> {
    match grid {
        Some((bpm, anchor)) => tx.execute(
            "INSERT INTO grids(track_id, bpm, anchor_ms) VALUES (?1, ?2, ?3) \
             ON CONFLICT(track_id) DO UPDATE SET bpm = ?2, anchor_ms = ?3",
            params![id, bpm, anchor],
        )?,
        None => tx.execute("DELETE FROM grids WHERE track_id = ?1", [id])?,
    };
    Ok(())
}

impl Library {
    /// What sending these playlists (folders with all inside) and tracks
    /// covers: the playlists with the folders above them, and every track.
    pub fn selection(&self, playlist_ids: &[i64], track_ids: &[i64]) -> Result<(Vec<i64>, Vec<i64>)> {
        let all = self.playlists()?;
        let by_id: HashMap<i64, &crate::Playlist> = all.iter().map(|p| (p.id, p)).collect();
        let mut chosen: Vec<i64> = Vec::new();
        let mut seen = HashSet::new();
        let mut stack: Vec<i64> = playlist_ids.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if !by_id.contains_key(&id) || !seen.insert(id) {
                continue;
            }
            chosen.push(id);
            stack.extend(all.iter().filter(|p| p.parent_id == id).map(|p| p.id).rev());
        }
        let mut lists = Vec::new();
        for &id in &chosen {
            let mut up = by_id[&id].parent_id;
            while let Some(p) = by_id.get(&up) {
                if seen.insert(p.id) {
                    lists.push(p.id);
                }
                up = p.parent_id;
            }
        }
        lists.extend(chosen.iter().copied());

        let mut tracks = Vec::new();
        let mut have = HashSet::new();
        for &id in &chosen {
            for &t in &by_id[&id].track_ids {
                if have.insert(t) {
                    tracks.push(t);
                }
            }
        }
        for &t in track_ids {
            if have.insert(t) {
                tracks.push(t);
            }
        }
        Ok((lists, tracks))
    }

    /// The offer for [`Library::selection`] of these. Every track must be
    /// fingerprinted.
    pub fn offer(&self, playlist_ids: &[i64], track_ids: &[i64]) -> Result<Offer> {
        let (lists, tracks) = self.selection(playlist_ids, track_ids)?;
        let mut fingerprints: HashMap<i64, String> = HashMap::new();
        let mut out_tracks = Vec::new();
        let mut sent = HashSet::new();
        for id in tracks {
            let Some(t) = self.track(id)? else { continue };
            let (fp, size, meta, cues, grid, anlz): (String, i64, i64, i64, i64, i64) = self.conn.query_row(
                "SELECT fingerprint, file_size, meta_modified, cues_modified, grid_modified, anlz_modified FROM tracks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?;
            if fp.is_empty() {
                return Err(Error::Invalid(format!("Skladba {} ještě nemá otisk.", t.title)));
            }
            fingerprints.insert(id, fp.clone());
            if !sent.insert(fp.clone()) {
                continue;
            }
            out_tracks.push(TrackData {
                entry: TrackEntry {
                    fingerprint: fp,
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    duration: t.duration,
                    size: size as u64,
                    meta_modified: meta,
                    cues_modified: cues,
                    grid_modified: grid,
                    anlz_modified: anlz,
                },
                tags: Tags {
                    genre: t.genre,
                    label: t.label,
                    remixer: t.remixer,
                    key: t.key,
                    bpm: t.bpm,
                    bitrate: t.bitrate,
                    sample_rate: t.sample_rate,
                    sample_depth: t.sample_depth,
                    rating: t.rating,
                    color: t.color,
                    comment: t.comment,
                    year: t.year,
                    track_number: t.track_number,
                    date_added: t.date_added,
                },
                file_name: Path::new(&t.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                cues: self.cues(id)?.iter().map(CueData::from).collect(),
                grid: self.grid(id)?,
                anlz: anlz_files(&t.anlz),
            });
        }

        let all = self.playlists()?;
        let mut st = self.conn.prepare("SELECT id, uid, modified FROM playlists")?;
        let stamps: HashMap<i64, (String, i64)> = st
            .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<rusqlite::Result<_>>()?;
        let playlists = lists
            .iter()
            .filter_map(|id| all.iter().find(|p| p.id == *id))
            .map(|p| {
                let (uid, modified) = stamps.get(&p.id).cloned().unwrap_or_default();
                PlaylistEntry {
                    uid,
                    parent: stamps.get(&p.parent_id).map(|s| s.0.clone()),
                    name: p.name.clone(),
                    is_folder: p.is_folder,
                    is_set: p.is_set,
                    sort_order: p.sort_order,
                    modified,
                    tracks: p.track_ids.iter().filter_map(|t| fingerprints.get(t).cloned()).collect(),
                }
            })
            .collect();
        Ok(Offer { library_uid: self.library_uid()?, playlists, tracks: out_tracks })
    }

    /// The files this library needs of an offer: tracks it lacks, and the
    /// analysis of those it has without one (or with an older one).
    pub fn wants(&self, offer: &Offer) -> Result<Vec<Want>> {
        let mut out = Vec::new();
        for t in &offer.tracks {
            let sender_anlz = t.anlz.iter().any(|e| e == "DAT");
            match local_by_fingerprint(&self.conn, &t.entry.fingerprint)? {
                None => out.push(Want { fingerprint: t.entry.fingerprint.clone(), audio: true, anlz: sender_anlz }),
                Some(l) => {
                    let lacking = l.anlz_path.is_empty() || !Path::new(&l.anlz_path).is_file();
                    if sender_anlz && (lacking || t.entry.anlz_modified > l.anlz) {
                        out.push(Want { fingerprint: t.entry.fingerprint.clone(), audio: false, anlz: true });
                    }
                }
            }
        }
        Ok(out)
    }

    /// Take in an offer whose files arrived as `received` (by fingerprint).
    pub fn apply_offer(&mut self, offer: &Offer, received: &HashMap<String, Received>) -> Result<ApplyReport> {
        let mut report = ApplyReport::default();
        let tx = self.conn.transaction()?;
        let mut ids: HashMap<&str, i64> = HashMap::new();

        for t in &offer.tracks {
            let e = &t.entry;
            let got = received.get(&e.fingerprint);
            let id = match local_by_fingerprint(&tx, &e.fingerprint)? {
                None => {
                    let Some((path, size, mtime)) = got.and_then(|r| r.audio.clone()) else {
                        report.skipped.push(label(t));
                        continue;
                    };
                    let g = &t.tags;
                    tx.execute(
                        "INSERT INTO tracks(path, title, artist, album, genre, label, remixer, key, bpm, duration, bitrate, \
                         sample_rate, sample_depth, rating, color, comment, year, track_number, date_added, \
                         fingerprint, file_size, file_mtime, cues_source) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
                         CASE WHEN ?19 = '' THEN date('now', 'localtime') ELSE ?19 END, ?20, ?21, ?22, 'local')",
                        params![
                            path, e.title, e.artist, e.album, g.genre, g.label, g.remixer, g.key, g.bpm, e.duration,
                            g.bitrate, g.sample_rate, g.sample_depth, g.rating, g.color, g.comment, g.year,
                            g.track_number, g.date_added, e.fingerprint, size as i64, mtime
                        ],
                    )?;
                    let id = tx.last_insert_rowid();
                    Library::write_cues(&tx, id, &t.cues.iter().map(CuePoint::from).collect::<Vec<_>>())?;
                    write_grid(&tx, id, t.grid)?;
                    if let Some(dat) = got.and_then(|r| r.anlz.as_ref()) {
                        tx.execute("UPDATE tracks SET anlz = ?2 WHERE id = ?1", params![id, dat])?;
                    }
                    report.tracks_added += 1;
                    for (c, v) in [
                        ("meta_modified", e.meta_modified),
                        ("cues_modified", e.cues_modified),
                        ("grid_modified", e.grid_modified),
                        ("anlz_modified", e.anlz_modified),
                    ] {
                        set_stamp(&tx, id, c, v)?;
                    }
                    id
                }
                Some(l) => {
                    let mut updated = false;
                    if e.meta_modified > l.meta {
                        let g = &t.tags;
                        tx.execute(
                            &format!("UPDATE tracks SET {TAG_COLUMNS} WHERE id = ?1"),
                            params![
                                l.id, e.title, e.artist, e.album, g.genre, g.label, g.remixer, g.key, g.bpm, g.rating,
                                g.color, g.comment, g.year, g.track_number
                            ],
                        )?;
                        set_stamp(&tx, l.id, "meta_modified", e.meta_modified)?;
                        updated = true;
                    }
                    if e.cues_modified > l.cues {
                        Library::backup_cues_on(&tx, l.id)?;
                        Library::write_cues(&tx, l.id, &t.cues.iter().map(CuePoint::from).collect::<Vec<_>>())?;
                        tx.execute("UPDATE tracks SET cues_source = 'local' WHERE id = ?1", [l.id])?;
                        set_stamp(&tx, l.id, "cues_modified", e.cues_modified)?;
                        report.cues_updated += 1;
                        updated = true;
                    }
                    if e.grid_modified > l.grid {
                        write_grid(&tx, l.id, t.grid)?;
                        set_stamp(&tx, l.id, "grid_modified", e.grid_modified)?;
                        updated = true;
                    }
                    if let Some(dat) = got.and_then(|r| r.anlz.as_ref()) {
                        tx.execute("UPDATE tracks SET anlz = ?2 WHERE id = ?1", params![l.id, dat])?;
                        set_stamp(&tx, l.id, "anlz_modified", e.anlz_modified.max(l.anlz))?;
                        updated = true;
                    }
                    if updated {
                        report.tracks_updated += 1;
                    }
                    l.id
                }
            };
            ids.insert(&e.fingerprint, id);
        }

        // Parents before children.
        let mut done: HashSet<&str> = HashSet::new();
        let in_offer: HashSet<&str> = offer.playlists.iter().map(|p| p.uid.as_str()).collect();
        let mut left: Vec<&PlaylistEntry> = offer.playlists.iter().collect();
        while !left.is_empty() {
            let (ready, wait): (Vec<&PlaylistEntry>, Vec<&PlaylistEntry>) = left.into_iter().partition(|p| {
                p.parent.as_deref().is_none_or(|u| done.contains(u) || !in_offer.contains(u))
            });
            // A cycle cannot come from a real library; take the rest as they are.
            let (ready, wait) = if ready.is_empty() { (wait, Vec::new()) } else { (ready, wait) };
            for p in ready {
                Self::apply_playlist(&tx, p, &ids, &mut report)?;
                done.insert(&p.uid);
            }
            left = wait;
        }
        tx.commit()?;
        Ok(report)
    }

    fn apply_playlist(tx: &Transaction, p: &PlaylistEntry, ids: &HashMap<&str, i64>, report: &mut ApplyReport) -> Result<()> {
        let uid_id = |uid: &str| -> Result<Option<i64>> {
            Ok(tx.query_row("SELECT id FROM playlists WHERE uid = ?1", [uid], |r| r.get(0)).optional()?)
        };
        let parent = match p.parent.as_deref() {
            Some(u) => uid_id(u)?.unwrap_or(0),
            None => 0,
        };
        let local: Option<(i64, i64)> = tx
            .query_row("SELECT id, modified FROM playlists WHERE uid = ?1", [&p.uid], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let id = match local {
            None => {
                let sort: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM playlists WHERE parent_id = ?1",
                    [parent],
                    |r| r.get(0),
                )?;
                tx.execute(
                    "INSERT INTO playlists(parent_id, name, is_folder, is_set, sort_order, source, uid) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'local', ?6)",
                    params![parent, p.name, p.is_folder, p.is_set, sort, p.uid],
                )?;
                report.playlists_added += 1;
                tx.last_insert_rowid()
            }
            Some((id, modified)) if p.modified > modified => {
                tx.execute(
                    "UPDATE playlists SET parent_id = ?2, name = ?3, is_folder = ?4, is_set = ?5, source = 'local' WHERE id = ?1",
                    params![id, parent, p.name, p.is_folder, p.is_set],
                )?;
                report.playlists_updated += 1;
                id
            }
            Some(_) => return Ok(()),
        };
        tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [id])?;
        for (i, t) in p.tracks.iter().filter_map(|f| ids.get(f.as_str())).enumerate() {
            tx.execute(
                "INSERT INTO playlist_items(playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
                params![id, i as i64, t],
            )?;
        }
        tx.execute("UPDATE playlists SET modified = ?2 WHERE id = ?1", params![id, p.modified])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library with tracks `/m/{i}.mp3` fingerprinted `f{i}`.
    fn library(n: i64) -> Library {
        let mut lib = Library::in_memory().unwrap();
        for i in 1..=n {
            lib.conn()
                .execute(
                    "INSERT INTO tracks(path, title, artist, duration, fingerprint, file_size) VALUES (?1, ?2, 'DJ', 300, ?3, 1000)",
                    params![format!("/m/{i}.mp3"), format!("T{i}"), format!("f{i}")],
                )
                .unwrap();
        }
        lib
    }

    fn arrived(fps: &[&str]) -> HashMap<String, Received> {
        fps.iter()
            .map(|f| (f.to_string(), Received { audio: Some((format!("/new/{f}.mp3"), 1000, 5)), anlz: None }))
            .collect()
    }

    #[test]
    fn selection_takes_folders_whole_and_their_parents() {
        let mut lib = library(4);
        let gigs = lib.create_playlist(0, "Gigy", true, false).unwrap();
        let club = lib.create_playlist(gigs, "Klub", true, false).unwrap();
        let set = lib.create_playlist(club, "Sobota", false, true).unwrap();
        let other = lib.create_playlist(gigs, "Jiný", false, false).unwrap();
        lib.set_playlist_tracks(set, &[2, 1]).unwrap();
        lib.set_playlist_tracks(other, &[3]).unwrap();

        let (lists, tracks) = lib.selection(&[set], &[4, 1]).unwrap();
        assert_eq!(lists, vec![club, gigs, set]);
        assert_eq!(tracks, vec![2, 1, 4]);
        let (lists, tracks) = lib.selection(&[gigs], &[]).unwrap();
        assert_eq!(lists.len(), 4);
        assert_eq!(tracks, vec![2, 1, 3]);
    }

    #[test]
    fn new_tracks_and_playlists_arrive_with_cues_and_grid() {
        let mut a = library(3);
        let folder = a.create_playlist(0, "Gigy", true, false).unwrap();
        let set = a.create_playlist(folder, "Sobota", false, true).unwrap();
        a.set_playlist_tracks(set, &[3, 1]).unwrap();
        a.set_cues(3, &[CuePoint { comment: "Drop".into(), ..CuePoint::hot(1, 64_000) }]).unwrap();
        a.set_grid(3, 124.0, 80.0).unwrap();
        let offer = a.offer(&[set], &[]).unwrap();
        assert_eq!(offer.tracks.len(), 2);
        assert_eq!(offer.playlists.len(), 2);

        let mut b = library(1); // has f1 already
        let wants = b.wants(&offer).unwrap();
        assert_eq!(wants, vec![Want { fingerprint: "f3".into(), audio: true, anlz: false }]);
        let r = b.apply_offer(&offer, &arrived(&["f3"])).unwrap();
        assert_eq!((r.tracks_added, r.playlists_added), (1, 2));
        assert!(r.skipped.is_empty());

        let new = b.track_by_fingerprint("f3").unwrap().unwrap();
        let t = b.track(new).unwrap().unwrap();
        assert_eq!((t.path.as_str(), t.title.as_str(), t.bpm), ("/new/f3.mp3", "T3", 124.0));
        assert_eq!(b.cues(new).unwrap()[0].comment, "Drop");
        assert_eq!(b.grid(new).unwrap(), Some((124.0, 80.0)));
        let lists = b.playlists().unwrap();
        let s = lists.iter().find(|p| p.is_set).unwrap();
        let f = lists.iter().find(|p| p.is_folder).unwrap();
        assert_eq!(s.parent_id, f.id);
        assert_eq!(s.track_ids, vec![new, 1]);
        // Same uid on both sides: the next send updates, not duplicates.
        let m = b.manifest().unwrap();
        assert!(m.playlists.iter().any(|p| p.uid == offer.playlists[1].uid));
        // The sender's stamps came along, so b does not look newer.
        let stamps = |l: &mut Library, id: i64| -> (i64, i64) {
            l.conn()
                .query_row("SELECT cues_modified, grid_modified FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
        };
        assert_eq!(stamps(&mut b, new), stamps(&mut a, 3));
        let again = b.apply_offer(&offer, &HashMap::new()).unwrap();
        assert_eq!(again, ApplyReport::default());
    }

    #[test]
    fn newer_edits_win_and_replaced_cues_are_kept() {
        let mut a = library(1);
        let mut b = library(1);
        b.set_cues(1, &[CuePoint::hot(1, 1000)]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        a.set_cues(1, &[CuePoint::hot(2, 2000)]).unwrap();
        a.conn().execute("UPDATE tracks SET rating = 5 WHERE id = 1", []).unwrap();

        let r = b.apply_offer(&a.offer(&[], &[1]).unwrap(), &HashMap::new()).unwrap();
        assert_eq!((r.tracks_updated, r.cues_updated), (1, 1));
        assert_eq!(b.cues(1).unwrap()[0].slot, 2);
        assert!(b.has_cue_backup(1).unwrap());
        assert_eq!(b.track(1).unwrap().unwrap().path, "/m/1.mp3", "the file stays");

        // Now b edits later: sending a's older state changes nothing.
        std::thread::sleep(std::time::Duration::from_millis(5));
        b.set_cues(1, &[CuePoint::hot(3, 3000)]).unwrap();
        let r = b.apply_offer(&a.offer(&[], &[1]).unwrap(), &HashMap::new()).unwrap();
        assert_eq!(r.cues_updated, 0);
        assert_eq!(b.cues(1).unwrap()[0].slot, 3);
    }

    #[test]
    fn tracks_that_did_not_arrive_are_skipped() {
        let mut a = library(2);
        let p = a.create_playlist(0, "P", false, false).unwrap();
        a.set_playlist_tracks(p, &[1, 2]).unwrap();
        let mut b = library(0);
        let r = b.apply_offer(&a.offer(&[p], &[]).unwrap(), &arrived(&["f2"])).unwrap();
        assert_eq!(r.skipped, vec!["DJ – T1"]);
        assert_eq!(b.playlists().unwrap()[0].track_ids.len(), 1);
    }

    #[test]
    fn unfingerprinted_tracks_cannot_be_offered() {
        let mut lib = library(1);
        lib.conn().execute("INSERT INTO tracks(path, title) VALUES ('/m/x.mp3', 'X')", []).unwrap();
        assert!(lib.offer(&[], &[2]).is_err());
    }
}
