#![allow(clippy::field_reassign_with_default)]

use std::path::Path;
use std::sync::atomic::AtomicBool;

use pioneer_formats::pdb::{
    Artist, Database, PlaylistEntry, PlaylistNode, Track, WriteOptions, write,
};
use usb_tools::Stick;
use usb_tools::extract::{Duplicates, ExtractOptions, Naming, execute, plan};

/// A fake stick: 3 tracks, folder "Gigy" with two playlists sharing track 2.
fn fake_stick(root: &Path) {
    let mut db = Database::default();
    db.artists = vec![Artist {
        id: 1,
        name: "Umělec".into(),
    }];
    for (i, name) in [(1u32, "Jedna.mp3"), (2, "Dvě: remix.mp3"), (3, "Tři.flac")] {
        let path = format!("/Contents/Umělec/{name}");
        let file = root.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, vec![i as u8; 1000 * i as usize]).unwrap();
        db.tracks.push(Track {
            title: format!("Song {i}"),
            artist_id: 1,
            duration: 60 * i as u16,
            file_path: path,
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
            name: "Warm/up".into(),
        },
    ];
    let entries = [(2, 1, 1), (2, 2, 2), (3, 1, 2), (3, 2, 3), (3, 3, 99)];
    db.playlist_entries = entries
        .iter()
        .map(|&(pl, idx, t)| PlaylistEntry {
            playlist_id: pl,
            entry_index: idx,
            track_id: t,
        })
        .collect();
    let pdb = root.join("PIONEER/rekordbox/export.pdb");
    std::fs::create_dir_all(pdb.parent().unwrap()).unwrap();
    std::fs::write(pdb, write(&db, &WriteOptions::default())).unwrap();
}

#[test]
fn extracts_playlists_into_folders() {
    let usb = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    fake_stick(usb.path());
    let stick = Stick::open(usb.path()).unwrap();
    let opts = ExtractOptions {
        numbered: true,
        m3u: true,
        duplicates: Duplicates::Clone,
        ..Default::default()
    };
    let plan = plan(&stick, &opts);
    assert_eq!(plan.playlists.len(), 2);
    assert_eq!(plan.total_files, 4);
    assert_eq!(
        plan.missing,
        vec!["Warm/up: track id 99 not in library".to_string()]
    );

    let cancel = AtomicBool::new(false);
    let report = execute(&plan, out.path(), &opts, &cancel, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.copied + report.cloned, 4);
    assert_eq!(
        report.cloned, 1,
        "track 2 appears twice and is cloned the second time"
    );

    let klub = out.path().join("Gigy/Klub X");
    assert_eq!(
        std::fs::read(klub.join("01 - Jedna.mp3")).unwrap(),
        vec![1u8; 1000]
    );
    assert_eq!(
        std::fs::read(klub.join("02 - Dvě- remix.mp3")).unwrap(),
        vec![2u8; 2000]
    );
    let warm = out.path().join("Gigy/Warm-up");
    assert!(warm.join("01 - Dvě- remix.mp3").is_file());
    let m3u = std::fs::read_to_string(klub.join("Klub X.m3u8")).unwrap();
    assert_eq!(
        m3u,
        "#EXTM3U\n#EXTINF:60,Umělec - Song 1\n01 - Jedna.mp3\n#EXTINF:120,Umělec - Song 2\n02 - Dvě- remix.mp3\n"
    );

    // Second run is a no-op.
    let again = execute(&plan, out.path(), &opts, &cancel, |_| {}).unwrap();
    assert_eq!(again.skipped_existing, 4);
    assert_eq!(again.copied + again.cloned, 0);
}

#[test]
fn artist_title_naming_and_single_playlist() {
    let usb = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    fake_stick(usb.path());
    let stick = Stick::open(usb.path()).unwrap();
    let opts = ExtractOptions {
        playlists: vec![3],
        naming: Naming::ArtistTitle,
        ..Default::default()
    };
    let plan = plan(&stick, &opts);
    assert_eq!(plan.playlists.len(), 1);
    execute(&plan, out.path(), &opts, &AtomicBool::new(false), |_| {}).unwrap();
    let warm = out.path().join("Gigy/Warm-up");
    assert!(warm.join("Umělec - Song 2.mp3").is_file());
    assert!(warm.join("Umělec - Song 3.flac").is_file());
}
