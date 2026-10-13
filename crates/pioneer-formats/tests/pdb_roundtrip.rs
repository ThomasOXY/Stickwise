//! export.pdb: read real rekordbox exports, write them back with our writer,
//! read again — the library must survive unchanged, and every row we encode
//! must be byte-identical to rekordbox's where the layout is known.

use std::path::{Path, PathBuf};

use pioneer_formats::pdb::{Database, Header, Track, WriteOptions, read_table_rows, table, write};

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/rekordcrate/data");
    let mut out = vec![
        root.join("complete_export/demo_tracks/PIONEER/rekordbox/export.pdb"),
        root.join("complete_export/empty/PIONEER/rekordbox/export.pdb"),
        root.join("pdb/num_rows/export.pdb"),
    ];
    if let Ok(extra) = std::env::var("RBX_PDB_CORPUS") {
        out.extend(extra.split(':').map(PathBuf::from));
    }
    out.retain(|p| p.exists());
    out
}

fn assert_same_library(a: &Database, b: &Database) {
    assert_eq!(a.tracks, b.tracks);
    assert_eq!(a.artists, b.artists);
    assert_eq!(a.albums, b.albums);
    assert_eq!(a.genres, b.genres);
    assert_eq!(a.labels, b.labels);
    assert_eq!(a.keys, b.keys);
    assert_eq!(a.colors, b.colors);
    assert_eq!(a.playlists, b.playlists);
    assert_eq!(a.playlist_entries, b.playlist_entries);
    assert_eq!(a.artwork, b.artwork);
}

#[test]
fn rewrite_preserves_library() {
    let files = fixtures();
    if files.is_empty() {
        eprintln!("no PDB fixtures, skipping");
        return;
    }
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let db = Database::parse(&bytes).unwrap();
        assert!(db.problems.is_empty(), "{}: {:?}", f.display(), db.problems);
        let out = write(
            &db,
            &WriteOptions {
                device_name: "TEST".into(),
                export_date: "2026-10-13".into(),
            },
        );
        let back = Database::parse(&out).unwrap();
        assert!(
            back.problems.is_empty(),
            "{}: {:?}",
            f.display(),
            back.problems
        );
        assert_same_library(&db, &back);
        eprintln!(
            "{}: {} tracks, {} playlists, {} entries — ok ({} → {} bytes)",
            f.display(),
            db.tracks.len(),
            db.playlists.len(),
            db.playlist_entries.len(),
            bytes.len(),
            out.len()
        );
    }
}

/// Our track encoder must reproduce rekordbox's own row bytes.
#[test]
fn track_rows_are_byte_identical() {
    for f in fixtures() {
        let bytes = std::fs::read(&f).unwrap();
        let header = Header::parse(&bytes).unwrap();
        let mut problems = Vec::new();
        for (i, raw) in read_table_rows(&bytes, &header, table::TRACKS, &mut problems)
            .iter()
            .enumerate()
        {
            let t = Track::parse(raw.data).unwrap();
            let ours = t.encode(0);
            // Compare the fixed fields and every string; padding between and
            // after strings may hold stale bytes in older rekordbox exports.
            let mut used = vec![false; ours.len()];
            used[..0x88].fill(true);
            used[2..4].fill(false); // index_shift depends on the row's page position
            for k in 0..21 {
                let at = u16::from_le_bytes([ours[0x5e + 2 * k], ours[0x5f + 2 * k]]) as usize;
                let len = pioneer_formats::devicesql::decode(&ours, at).unwrap().1;
                used[at..at + len].fill(true);
            }
            let pick = |b: &[u8]| -> Vec<u8> {
                b.iter()
                    .zip(&used)
                    .filter(|(_, u)| **u)
                    .map(|(b, _)| *b)
                    .collect()
            };
            let (ours, theirs) = (pick(&ours), pick(&raw.data[..used.len()]));
            assert_eq!(ours, theirs, "{} track #{i} differs", f.display());
        }
    }
}

#[test]
fn many_tracks_span_pages() {
    let mut db = Database::default();
    for i in 1..=3000u32 {
        db.tracks.push(Track {
            title: format!("Track {i} — Příliš žluťoučký kůň"),
            file_path: format!("/Contents/Artist {}/Album/{i}.mp3", i % 50),
            tempo: 12000 + i,
            ..Track::new_export(i)
        });
    }
    db.playlists.push(pioneer_formats::pdb::PlaylistNode {
        id: 1,
        parent_id: 0,
        sort_order: 0,
        is_folder: false,
        name: "Všechno".into(),
    });
    db.playlist_entries = (1..=3000)
        .map(|i| pioneer_formats::pdb::PlaylistEntry {
            entry_index: i,
            track_id: i,
            playlist_id: 1,
        })
        .collect();
    let out = write(&db, &WriteOptions::default());
    let back = Database::parse(&out).unwrap();
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert_eq!(back.tracks, db.tracks);
    assert_eq!(back.playlist_tracks(1), (1..=3000).collect::<Vec<_>>());
}
