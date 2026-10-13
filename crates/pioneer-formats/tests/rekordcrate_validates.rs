#![allow(clippy::field_reassign_with_default)]

//! Our writer's output must be readable by an independent parser
//! (rekordcrate) with the same rows we put in.

use std::collections::HashMap;
use std::io::Cursor;

use fallible_iterator::FallibleIterator;
use pioneer_formats::pdb::{
    Album, Artist, Database, Key, Named, PlaylistEntry, PlaylistNode, Track, WriteOptions, write,
};
use rekordcrate::pdb::io::Database as RcDatabase;
use rekordcrate::pdb::{DatabaseType, PageContent, PageType, PlainRow, Row};
use rekordcrate::util::TableIndex;

fn sample() -> Database {
    let mut db = Database::default();
    db.artists = (1..=40)
        .map(|i| Artist {
            id: i,
            name: format!("Umělec {i}"),
        })
        .collect();
    db.albums = (1..=20)
        .map(|i| Album {
            id: i,
            artist_id: i,
            name: format!("Album {i}"),
        })
        .collect();
    db.genres = vec![
        Named {
            id: 1,
            name: "Techno".into(),
        },
        Named {
            id: 2,
            name: "Drum & Bass".into(),
        },
    ];
    db.keys = vec![Key {
        id: 1,
        id2: 1,
        name: "8A".into(),
    }];
    for i in 1..=600u32 {
        db.tracks.push(Track {
            title: format!("Skladba č. {i}"),
            artist_id: i % 40 + 1,
            album_id: i % 20 + 1,
            genre_id: i % 2 + 1,
            key_id: 1,
            tempo: 12800,
            duration: 300,
            file_path: format!("/Contents/Umělec {}/Album/{i:04}.mp3", i % 40 + 1),
            filename: format!("{i:04}.mp3"),
            analyze_path: format!("/PIONEER/USBANLZ/P000/{i:08X}/ANLZ0000.DAT"),
            ..Track::new_export(i)
        });
    }
    db.playlists = vec![
        PlaylistNode {
            id: 1,
            parent_id: 0,
            sort_order: 0,
            is_folder: true,
            name: "Gigy".into(),
        },
        PlaylistNode {
            id: 2,
            parent_id: 1,
            sort_order: 0,
            is_folder: false,
            name: "Klub X".into(),
        },
        PlaylistNode {
            id: 3,
            parent_id: 1,
            sort_order: 1,
            is_folder: false,
            name: "Warm-up".into(),
        },
    ];
    for (pl, range) in [(2u32, 1..=400u32), (3, 401..=600)] {
        for (n, t) in range.enumerate() {
            db.playlist_entries.push(PlaylistEntry {
                entry_index: n as u32 + 1,
                track_id: t,
                playlist_id: pl,
            });
        }
    }
    db
}

#[test]
fn rekordcrate_reads_our_export() {
    let db = sample();
    let bytes = write(
        &db,
        &WriteOptions {
            device_name: "RBX".into(),
            export_date: "2026-10-13".into(),
        },
    );
    let mut rc = RcDatabase::open_non_persistent(Cursor::new(bytes), DatabaseType::Plain).unwrap();
    let tables = rc.get_header().tables.clone();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut titles = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        if matches!(t.page_type, PageType::Unknown(_)) {
            continue;
        }
        let mut pages = rc.iter_pages_for_table(TableIndex::from(i)).unwrap();
        while let Some(page) = pages.next().unwrap() {
            if let PageContent::Data(data) = &page.content {
                for row in data.rows.values() {
                    *counts.entry(format!("{:?}", t.page_type)).or_default() += 1;
                    if let Row::Plain(PlainRow::Track(track)) = row {
                        titles.push(track.offsets.inner.title.clone().into_string().unwrap());
                    }
                }
            }
        }
    }
    let count = |needle: &str| {
        counts
            .iter()
            .filter(|(k, _)| k.contains(needle))
            .map(|(_, v)| *v)
            .sum::<usize>()
    };
    assert_eq!(count("Tracks"), 600, "{counts:?}");
    assert_eq!(count("Artists"), 40, "{counts:?}");
    assert_eq!(count("Albums"), 20, "{counts:?}");
    assert_eq!(count("PlaylistTree"), 3, "{counts:?}");
    assert_eq!(count("PlaylistEntries"), 600, "{counts:?}");
    let mut expected: Vec<String> = db.tracks.iter().map(|t| t.title.clone()).collect();
    titles.sort();
    expected.sort();
    assert_eq!(titles, expected);
}
