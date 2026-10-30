//! Build a fresh `m.db` in one transaction, then check it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, params};

use crate::{Error, Library, Result, Schema, read};

/// What [`build`] wrote.
#[derive(Debug, Clone, Default)]
pub struct BuildReport {
    pub uuid: String,
    pub tracks: usize,
    pub playlists: usize,
    /// Playlists renamed because Engine wants unique names per folder.
    pub renamed: Vec<(String, String)>,
    /// Entries dropped because Engine allows a track once per playlist.
    pub duplicate_entries: usize,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `YYYY-MM-DD HH:MM:SS` (UTC), the way Engine stamps playlists.
fn datetime(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn file_type(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn opt(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

/// Write `lib` as a new Engine database at `path` (which must not exist).
/// `uuid` keeps the identity of the library the stick had, so Engine's
/// play history (`hm.db`) still matches. The result is read back and
/// compared before this returns.
pub fn build(
    path: &Path,
    lib: &Library,
    schema: Schema,
    uuid: Option<&str>,
) -> Result<BuildReport> {
    if path.exists() {
        return Err(Error::Invalid(format!("{} already exists", path.display())));
    }
    let uuid = uuid.map(String::from).unwrap_or_else(crate::new_uuid);
    let mut conn = Connection::open(path)?;
    let mode: String = conn.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(Error::Invalid(format!("journal_mode {mode}")));
    }
    conn.execute_batch(schema.ddl())?;
    let (major, minor, patch) = schema.version();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO Information (uuid, schemaVersionMajor, schemaVersionMinor, schemaVersionPatch, \
         currentPlayedIndiciator, lastRekordBoxLibraryImportReadCounter) VALUES (?1, ?2, ?3, ?4, 0, NULL)",
        params![uuid, major, minor, patch],
    )?;
    // Row 1 is Engine's "no artwork".
    tx.execute(
        "INSERT OR IGNORE INTO AlbumArt (id, hash, albumArt) VALUES (1, NULL, NULL)",
        [],
    )?;

    let stamp = now();
    let mut ids: Vec<i64> = Vec::with_capacity(lib.tracks.len());
    {
        let blob_cols = if schema.performance_table() {
            ""
        } else {
            ", trackData, overviewWaveFormData, beatData, quickCues, loops, thirdPartySourceId, activeOnLoadLoops"
        };
        let blob_vals = if schema.performance_table() {
            ""
        } else {
            ", ?42, ?43, ?44, ?45, ?46, NULL, NULL"
        };
        let sql = format!(
            "INSERT INTO Track (playOrder, length, bpm, year, path, filename, bitrate, bpmAnalyzed, \
             albumArtId, fileBytes, title, artist, album, genre, comment, label, composer, remixer, key, \
             rating, albumArt, timeLastPlayed, isPlayed, fileType, isAnalyzed, dateCreated, dateAdded, \
             isAvailable, isMetadataOfPackedTrackChanged, isPerfomanceDataOfPackedTrackChanged, \
             playedIndicator, isMetadataImported, pdbImportKey, streamingSource, uri, isBeatGridLocked, \
             originDatabaseUuid, originTrackId, streamingFlags, explicitLyrics, lastEditTime{blob_cols}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, \
             ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, \
             ?39, ?40, ?41{blob_vals})"
        );
        let mut ins = tx.prepare(&sql)?;
        let mut perf = if schema.performance_table() {
            Some(tx.prepare(
                "UPDATE PerformanceData SET trackData = ?1, overviewWaveFormData = ?2, beatData = ?3, \
                 quickCues = ?4, loops = ?5 WHERE trackId = ?6",
            )?)
        } else {
            None
        };
        let mut seen: HashSet<&str> = HashSet::new();
        for t in &lib.tracks {
            if !seen.insert(&t.path) {
                return Err(Error::Invalid(format!("track listed twice: {}", t.path)));
            }
            let filename = Path::new(&t.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let track_data = t.track_data.encode();
            let overview = t.overview.as_ref().map(|o| o.encode());
            let beats = t.beats.encode();
            let cues = t.cues.encode();
            let loops = t.loops.encode();
            let date = if t.date_added > 0 {
                t.date_added
            } else {
                stamp
            };
            let common: [&dyn rusqlite::ToSql; 41] = [
                &Option::<i64>::None,
                &t.length,
                &(t.bpm.round() as i64),
                &(t.year > 0).then_some(t.year),
                &t.path,
                &filename,
                &(t.bitrate > 0).then_some(t.bitrate),
                &t.bpm,
                &1i64,
                &(t.file_bytes as i64),
                &t.title,
                &opt(&t.artist),
                &opt(&t.album),
                &opt(&t.genre),
                &opt(&t.comment),
                &opt(&t.label),
                &opt(&t.composer),
                &opt(&t.remixer),
                &t.key,
                &(t.rating.min(5) as i64 * 20),
                &"image://planck/0",
                &Option::<i64>::None,
                &0i64,
                &file_type(&t.path),
                &1i64,
                &date,
                &date,
                &1i64,
                &0i64,
                &0i64,
                &Option::<i64>::None,
                &1i64,
                &0i64,
                &Option::<i64>::None,
                &Option::<String>::None,
                &0i64,
                &Option::<String>::None,
                &Option::<i64>::None,
                &0i64,
                &0i64,
                &stamp,
            ];
            if schema.performance_table() {
                ins.execute(&common[..])?;
            } else {
                let mut all: Vec<&dyn rusqlite::ToSql> = common.to_vec();
                all.extend([
                    &track_data as &dyn rusqlite::ToSql,
                    &overview,
                    &beats,
                    &cues,
                    &loops,
                ]);
                ins.execute(&all[..])?;
            }
            let id = tx.last_insert_rowid();
            if let Some(perf) = perf.as_mut() {
                let n = perf.execute(params![track_data, overview, beats, cues, loops, id])?;
                if n != 1 {
                    return Err(Error::Invalid(format!(
                        "no PerformanceData row for track {id}: schema triggers missing"
                    )));
                }
            }
            ids.push(id);
        }
    }

    // Playlists: ids allocated parents first; siblings chained by nextListId.
    let mut report = BuildReport {
        uuid: uuid.clone(),
        tracks: ids.len(),
        ..Default::default()
    };
    let keys: HashSet<i64> = lib.playlists.iter().map(|p| p.key).collect();
    let parent_of = |p: &crate::Playlist| {
        if keys.contains(&p.parent) {
            p.parent
        } else {
            0
        }
    };
    let mut order: Vec<&crate::Playlist> = Vec::new();
    let mut placed: HashSet<i64> = HashSet::new();
    while order.len() < lib.playlists.len() {
        let before = order.len();
        let mut ready: Vec<&crate::Playlist> = lib
            .playlists
            .iter()
            .filter(|p| {
                !placed.contains(&p.key) && (parent_of(p) == 0 || placed.contains(&parent_of(p)))
            })
            .collect();
        ready.sort_by_key(|p| (p.sort, p.key));
        for p in ready {
            placed.insert(p.key);
            order.push(p);
        }
        if order.len() == before {
            return Err(Error::Invalid("playlist folders form a cycle".into()));
        }
    }
    let engine_id: HashMap<i64, i64> = order
        .iter()
        .enumerate()
        .map(|(i, p)| (p.key, i as i64 + 1))
        .collect();
    let mut siblings: HashMap<i64, Vec<&crate::Playlist>> = HashMap::new();
    for p in &order {
        siblings.entry(parent_of(p)).or_default().push(p);
    }
    let mut next: HashMap<i64, i64> = HashMap::new();
    let mut title: HashMap<i64, String> = HashMap::new();
    for group in siblings.values_mut() {
        group.sort_by_key(|p| (p.sort, p.key));
        let mut used: HashSet<String> = HashSet::new();
        for (i, p) in group.iter().enumerate() {
            next.insert(p.key, group.get(i + 1).map_or(0, |n| engine_id[&n.key]));
            let base = if p.title.trim().is_empty() {
                "Playlist".to_string()
            } else {
                p.title.clone()
            };
            let mut t = base.clone();
            let mut n = 2;
            while !used.insert(t.to_lowercase()) {
                t = format!("{base} ({n})");
                n += 1;
            }
            if t != p.title {
                report.renamed.push((p.title.clone(), t.clone()));
            }
            title.insert(p.key, t);
        }
    }
    let edited = datetime(stamp);
    let mut intended: HashMap<i64, Vec<i64>> = HashMap::new();
    {
        let mut ins = tx.prepare(
            "INSERT INTO Playlist (id, title, parentListId, isPersisted, nextListId, lastEditTime, \
             isExplicitlyExported) VALUES (?1, ?2, ?3, 1, ?4, ?5, 1)",
        )?;
        for p in &order {
            let parent = parent_of(p);
            ins.execute(params![
                engine_id[&p.key],
                title[&p.key],
                if parent == 0 { 0 } else { engine_id[&parent] },
                next[&p.key],
                edited
            ])?;
        }
        let mut ent = tx.prepare(
            "INSERT INTO PlaylistEntity (id, listId, trackId, databaseUuid, nextEntityId, \
             membershipReference) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        )?;
        let mut next_id = 1i64;
        for p in &order {
            let mut seen = HashSet::new();
            let list: Vec<i64> = p
                .tracks
                .iter()
                .filter_map(|&i| ids.get(i).copied())
                .filter(|id| {
                    let fresh = seen.insert(*id);
                    if !fresh {
                        report.duplicate_entries += 1;
                    }
                    fresh
                })
                .collect();
            for (i, tid) in list.iter().enumerate() {
                let nxt = if i + 1 < list.len() { next_id + 1 } else { 0 };
                ent.execute(params![next_id, engine_id[&p.key], tid, uuid, nxt])?;
                next_id += 1;
            }
            if !list.is_empty() {
                intended.insert(engine_id[&p.key], list);
            }
        }
    }
    tx.commit()?;
    report.playlists = order.len();

    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(Error::Invalid(format!("integrity_check: {check}")));
    }
    let fk: Option<String> = conn
        .query_row("PRAGMA foreign_key_check", [], |r| r.get::<_, String>(0))
        .ok();
    if let Some(table) = fk {
        return Err(Error::Invalid(format!("foreign_key_check: {table}")));
    }
    drop(conn);

    // Read back what Engine will read.
    let back = read(path)?;
    if back.schema != Some(schema) || back.uuid != uuid || back.tracks.len() != lib.tracks.len() {
        return Err(Error::Invalid(
            "read-back differs from what was written".into(),
        ));
    }
    for (t, b) in lib.tracks.iter().zip(&back.tracks) {
        if t.path != b.path || t.cues != b.cues || t.loops != b.loops || t.beats != b.beats {
            return Err(Error::Invalid(format!("read-back differs for {}", t.path)));
        }
    }
    for p in &back.playlists {
        let want = intended.get(&p.id).cloned().unwrap_or_default();
        if p.tracks != want {
            return Err(Error::Invalid(format!(
                "playlist {} reads back in another order",
                p.title
            )));
        }
    }
    let mut want_order: HashMap<i64, Vec<i64>> = HashMap::new();
    for (parent, group) in &siblings {
        let parent = if *parent == 0 { 0 } else { engine_id[parent] };
        want_order.insert(parent, group.iter().map(|p| engine_id[&p.key]).collect());
    }
    for (parent, want) in want_order {
        let got: Vec<i64> = back.children(parent).iter().map(|p| p.id).collect();
        if got != want {
            return Err(Error::Invalid(
                "playlist order reads back differently".into(),
            ));
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::datetime;

    #[test]
    fn dates() {
        assert_eq!(datetime(0), "1970-01-01 00:00:00");
        assert_eq!(datetime(1_762_352_326), "2025-11-05 14:18:46");
    }
}
