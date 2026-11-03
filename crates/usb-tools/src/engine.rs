//! The Denon side of a hybrid stick: an Engine library
//! (`Engine Library/Database2/m.db`) over the same audio files the
//! rekordbox library uses, so the music is on the stick once.
//!
//! The database is built and checked on the Mac, then swapped in with one
//! rename. The previous one goes to the backups; Engine's other files
//! (play history `hm.db`, …) stay, and the library keeps its identity so
//! the history still matches.

use std::path::{Component, Path, PathBuf};

use engine_library::blobs::{BeatData, TrackData};
use engine_library::{Library, Playlist, Schema, Track, convert};
use pioneer_formats::anlz::{AnlzFile, Waveform, tag};

use crate::cue_write::replace_file;
use crate::sync::{SyncPlan, SyncRequest};

pub const DB_DIR: &str = "Engine Library/Database2";

#[derive(Debug, Default)]
pub struct EngineReport {
    pub schema: Option<Schema>,
    pub tracks: usize,
    pub playlists: usize,
    /// Schema of the library the stick had before, when it was one.
    pub replaced: Option<(i64, i64, i64)>,
    /// Hot and memory cues that did not fit Engine's eight pads / loops.
    pub dropped_cues: usize,
    pub warnings: Vec<String>,
}

/// Days since 1970-01-01 for a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DD…` → Unix seconds.
fn unix_date(s: &str) -> i64 {
    let p: Vec<i64> = s
        .get(..10)
        .unwrap_or("")
        .split('-')
        .filter_map(|x| x.parse().ok())
        .collect();
    match p[..] {
        [y, m, d] if (1..=12).contains(&m) && (1..=31).contains(&d) => {
            days_from_civil(y, m, d) * 86_400
        }
        _ => 0,
    }
}

fn read_anlz(p: &Path) -> Option<AnlzFile> {
    std::fs::read(p).ok().and_then(|b| AnlzFile::parse(&b).ok())
}

/// Turn the sync request into an Engine library, reading beat grids and
/// waveforms from the tracks' analysis.
pub fn library(req: &SyncRequest, plan: &SyncPlan) -> (Library, usize) {
    let by_key: std::collections::HashMap<i64, &crate::sync::SyncTrack> =
        req.tracks.iter().map(|t| (t.key, t)).collect();
    let mut lib = Library::default();
    let mut index = std::collections::HashMap::new();
    let mut by_path = std::collections::HashMap::new();
    let mut dropped = 0;
    for pt in &plan.tracks {
        // The same song twice in the library (from two folders) shares one
        // file on the stick; Engine takes a file once, so both are it.
        if let Some(&i) = by_path.get(&pt.device_path.to_lowercase()) {
            index.insert(pt.key, i);
            continue;
        }
        by_path.insert(pt.device_path.to_lowercase(), lib.tracks.len());
        let t = by_key[&pt.key];
        let sr = if t.sample_rate > 0 {
            t.sample_rate as f64
        } else {
            44100.0
        };
        let samples = (t.duration.max(0.0) * sr).round() as i64;
        let src = crate::sync::anlz_source(t, pt, &req.stick);
        let dat = src.as_deref().and_then(read_anlz);
        let two = src.as_ref().and_then(|p| read_anlz(&p.with_extension("2EX")));
        let grid = match &t.grid {
            Some(beats) => convert::beat_grid(beats, sr, samples),
            None => dat
                .as_ref()
                .and_then(|d| d.beat_grid())
                .and_then(Result::ok)
                .map(|g| convert::beat_grid(&g.beats, sr, samples))
                .unwrap_or_default(),
        };
        let overview = two
            .as_ref()
            .and_then(|f| f.section(tag::WAVE_3BAND_PREVIEW))
            .or_else(|| dat.as_ref().and_then(|f| f.section(tag::WAVE_PREVIEW)))
            .and_then(|s| Waveform::parse(s).ok())
            .and_then(|w| convert::overview(&w, samples));
        let cues = convert::cues(&t.cues, sr);
        dropped += cues.dropped;
        let key = convert::key_ordinal(&t.key_name);
        index.insert(pt.key, lib.tracks.len());
        lib.tracks.push(Track {
            path: format!("..{}", pt.device_path),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            genre: t.genre.clone(),
            comment: t.comment.clone(),
            label: t.label.clone(),
            remixer: t.remixer.clone(),
            year: t.year,
            bpm: t.bpm,
            length: t.duration.round().max(0.0) as u32,
            bitrate: t.bitrate,
            file_bytes: pt.size,
            rating: t.rating,
            key,
            date_added: unix_date(&t.date_added),
            track_data: TrackData {
                sample_rate: sr,
                samples,
                key: key.unwrap_or(0),
                loudness: [0.0; 3],
            },
            beats: BeatData {
                sample_rate: sr,
                samples: samples as f64,
                is_set: !grid.is_empty(),
                default_grid: grid.clone(),
                adjusted_grid: grid,
                extra: Vec::new(),
            },
            cues: cues.quick_cues,
            loops: cues.loops,
            overview,
            ..Default::default()
        });
    }
    lib.playlists = req
        .playlists
        .iter()
        .map(|p| Playlist {
            key: p.key,
            parent: p.parent,
            title: p.name.clone(),
            sort: p.sort,
            tracks: {
                let mut seen = std::collections::HashSet::new();
                p.tracks
                    .iter()
                    .filter_map(|k| index.get(k).copied())
                    .filter(|&i| seen.insert(i))
                    .collect()
            },
        })
        .collect();
    (lib, dropped)
}

/// `Engine Library/../Contents/x.mp3` → `<stick>/Contents/x.mp3`.
fn resolve(stick: &Path, rel: &str) -> PathBuf {
    let mut out = stick.join("Engine Library");
    for c in Path::new(rel).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            _ => {}
        }
    }
    out
}

/// Build the library, check it, and swap it onto the stick.
pub fn install(
    req: &SyncRequest,
    plan: &SyncPlan,
    schema: Schema,
    backup_dir: &Path,
) -> Result<EngineReport, String> {
    let stick = &req.stick;
    let dir = stick.join(DB_DIR);
    let live = dir.join("m.db");
    let mut report = EngineReport {
        schema: Some(schema),
        ..Default::default()
    };
    // Keep the library's identity, so Engine's history still lines up.
    let old = live
        .is_file()
        .then(|| engine_library::info(&live).ok())
        .flatten();
    report.replaced = old.as_ref().map(|o| o.0);
    let uuid = old.map(|o| o.1).filter(|u| !u.is_empty());

    let (lib, dropped) = library(req, plan);
    report.dropped_cues = dropped;
    let tmp = std::env::temp_dir().join(format!(
        "rbx-engine-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let built = tmp.join("m.db");
    let result = (|| {
        let r = engine_library::build(&built, &lib, schema, uuid.as_deref())
            .map_err(|e| e.to_string())?;
        report.tracks = r.tracks;
        report.playlists = r.playlists;
        for (from, to) in r.renamed {
            report
                .warnings
                .push(format!("Denon: playlist „{from}“ přejmenován na „{to}“"));
        }
        let bytes = std::fs::read(&built).map_err(|e| e.to_string())?;

        // Back up the old library, then move journals aside: a leftover
        // journal would be applied to the new database.
        std::fs::create_dir_all(&dir).map_err(|e| format!("{DB_DIR}: {e}"))?;
        let saved = backup_dir.join(DB_DIR);
        let aside = stick.join(".rbx/old").join(&req.date).join(DB_DIR);
        for name in ["m.db", "m.db-journal", "m.db-wal", "m.db-shm"] {
            let f = dir.join(name);
            if f.is_file() {
                std::fs::create_dir_all(&saved).map_err(|e| e.to_string())?;
                std::fs::copy(&f, saved.join(name)).map_err(|e| format!("záloha {name}: {e}"))?;
                if name != "m.db" {
                    std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
                    std::fs::rename(&f, aside.join(name)).map_err(|e| format!("{name}: {e}"))?;
                }
            }
        }
        replace_file(&live, &bytes).map_err(|e| format!("{DB_DIR}/m.db: {e}"))?;
        crate::format::remove_apple_double(&stick.join("Engine Library"));

        // Read back what the Denon will read.
        let back = engine_library::read(&live).map_err(|e| format!("kontrola m.db: {e}"))?;
        if back.tracks.len() != lib.tracks.len() {
            return Err("Denon knihovna po zápisu nesedí.".to_string());
        }
        for t in &back.tracks {
            if !resolve(stick, &t.path).is_file() {
                report
                    .warnings
                    .push(format!("Denon nenajde audio: {}", t.path));
            }
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result.map(|_| report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths() {
        assert_eq!(unix_date("2025-11-05"), 1_762_300_800);
        assert_eq!(unix_date("1970-01-01"), 0);
        assert_eq!(unix_date(""), 0);
        assert_eq!(
            resolve(Path::new("/Volumes/X"), "../Contents/A/b.mp3"),
            PathBuf::from("/Volumes/X/Contents/A/b.mp3")
        );
    }
}
