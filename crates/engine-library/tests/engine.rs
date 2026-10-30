//! Checks against libraries Engine DJ itself wrote.
//!
//! Fixtures `engine_ref_3_0_1.db` and `engine_stick_3_0_2.db` come from
//! rb2engine (MIT, (c) 2026 J.R. Gutierrez), captured from Engine DJ 4.3.
//! Set `RBX_ENGINE_DB` to a copy of another Engine `m.db` to check it too.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use engine_library::blobs::{
    BeatData, Loops, Overview, QuickCue, QuickCues, TrackData, decompress,
};
use engine_library::{Library, Playlist, Schema, Track, build, read};
use rusqlite::{Connection, OpenFlags};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn engine_dbs() -> Vec<PathBuf> {
    let mut v = vec![
        fixture("engine_ref_3_0_1.db"),
        fixture("engine_stick_3_0_2.db"),
    ];
    if let Some(p) = std::env::var_os("RBX_ENGINE_DB") {
        v.push(PathBuf::from(p));
    }
    v
}

fn raw_blobs(path: &Path) -> Vec<[Vec<u8>; 5]> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut st = c
        .prepare(
            "SELECT trackData, beatData, quickCues, loops, overviewWaveFormData FROM PerformanceData",
        )
        .unwrap();
    st.query_map([], |r| {
        let b = |i| -> rusqlite::Result<Vec<u8>> {
            Ok(r.get::<_, Option<Vec<u8>>>(i)?.unwrap_or_default())
        };
        Ok([b(0)?, b(1)?, b(2)?, b(3)?, b(4)?])
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// Decoding and encoding again gives Engine's bytes back (compressed blobs:
/// the same payload).
#[test]
fn blobs_reencode_byte_identical() {
    let mut checked = 0;
    for db in engine_dbs() {
        for [td, bd, qc, lp, ov] in raw_blobs(&db) {
            if !td.is_empty() {
                let x = TrackData::decode(&td).unwrap();
                assert_eq!(decompress(&x.encode()).unwrap(), decompress(&td).unwrap());
            }
            if !bd.is_empty() {
                let x = BeatData::decode(&bd).unwrap();
                assert_eq!(decompress(&x.encode()).unwrap(), decompress(&bd).unwrap());
            }
            if !qc.is_empty() {
                let x = QuickCues::decode(&qc).unwrap();
                assert_eq!(decompress(&x.encode()).unwrap(), decompress(&qc).unwrap());
            }
            if !lp.is_empty() {
                assert_eq!(Loops::decode(&lp).unwrap().encode(), lp);
            }
            if !ov.is_empty() {
                let x = Overview::decode(&ov).unwrap();
                assert_eq!(decompress(&x.encode()).unwrap(), decompress(&ov).unwrap());
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
}

fn schema_objects(path: &Path) -> BTreeSet<String> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut st = c
        .prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    st.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|s| s.unwrap().split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn sample() -> Library {
    let track = |i: usize, title: &str| {
        let mut cues = QuickCues::default();
        cues.cues[0] = QuickCue {
            label: "Drop".into(),
            sample_offset: 44100.0 * i as f64,
            color: [255, 40, 226, 20],
        };
        Track {
            path: format!("../Contents/Umělec/Album/{title}.mp3"),
            title: title.into(),
            artist: "Umělec".into(),
            bpm: 128.0,
            length: 300,
            file_bytes: 10_000_000,
            rating: 4,
            key: Some(1),
            track_data: TrackData {
                sample_rate: 44100.0,
                samples: 44100 * 300,
                key: 1,
                loudness: [0.0; 3],
            },
            beats: BeatData {
                sample_rate: 44100.0,
                samples: 44100.0 * 300.0,
                is_set: true,
                ..Default::default()
            },
            cues,
            loops: Loops::default(),
            overview: Some(Overview {
                samples_per_point: 44100.0 * 300.0 / 1024.0,
                points: vec![[200, 100, 40]; 1024],
            }),
            ..Default::default()
        }
    };
    Library {
        tracks: vec![track(1, "Jedna"), track(2, "Dvě"), track(3, "Tři")],
        playlists: vec![
            Playlist {
                key: 10,
                parent: 0,
                title: "Sety".into(),
                sort: 0,
                tracks: vec![],
            },
            Playlist {
                key: 11,
                parent: 10,
                title: "Klub".into(),
                sort: 1,
                tracks: vec![2, 0, 1],
            },
            Playlist {
                key: 12,
                parent: 10,
                title: "Klub".into(),
                sort: 0,
                tracks: vec![1, 1, 2],
            },
            Playlist {
                key: 13,
                parent: 0,
                title: "Techno".into(),
                sort: 1,
                tracks: vec![0],
            },
        ],
    }
}

#[test]
fn builds_every_schema_and_reads_back() {
    for schema in Schema::ALL {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.db");
        let r = build(
            &path,
            &sample(),
            schema,
            Some("11111111-2222-4333-8444-555555555555"),
        )
        .unwrap();
        assert_eq!((r.tracks, r.playlists), (3, 4));
        assert_eq!(r.duplicate_entries, 1);
        assert_eq!(r.renamed.len(), 1, "two 'Klub' in one folder");
        let back = read(&path).unwrap();
        assert_eq!(back.schema, Some(schema));
        assert_eq!(back.uuid, "11111111-2222-4333-8444-555555555555");
        let top: Vec<&str> = back.children(0).iter().map(|p| p.title.as_str()).collect();
        assert_eq!(top, ["Sety", "Techno"]);
        let sety = back.children(0)[0].id;
        let sets: Vec<(&str, &[i64])> = back
            .children(sety)
            .iter()
            .map(|p| (p.title.as_str(), p.tracks.as_slice()))
            .collect();
        assert_eq!(sets[0], ("Klub", &[2, 3][..]));
        assert_eq!(sets[1], ("Klub (2)", &[3, 1, 2][..]));
        assert_eq!(back.tracks[0].cues.cues[0].label, "Drop");
        assert_eq!(back.tracks[1].path, "../Contents/Umělec/Album/Dvě.mp3");
        assert!(back.tracks[0].overview.is_some());
    }
}

/// A database we build carries exactly the schema Engine wrote.
#[test]
fn schema_matches_engine() {
    for (schema, golden) in [
        (Schema::V3_0_1, "engine_ref_3_0_1.db"),
        (Schema::V3_0_2, "engine_stick_3_0_2.db"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.db");
        build(&path, &Library::default(), schema, None).unwrap();
        assert_eq!(
            schema_objects(&path),
            schema_objects(&fixture(golden)),
            "{schema:?}"
        );
    }
}
