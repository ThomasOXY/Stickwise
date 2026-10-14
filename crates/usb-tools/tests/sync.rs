use std::path::Path;
use std::sync::atomic::AtomicBool;

use pioneer_formats::anlz::{AnlzFile, CuePoint, read_cue_points};
use pioneer_formats::pdb::Database;
use usb_tools::sync::{SyncPlaylist, SyncRequest, SyncTrack, execute, plan};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, d) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &d)
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}

#[test]
fn syncs_library_selection_to_a_stick() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/rekordcrate/data/complete_export/demo_tracks");
    if !demo.exists() {
        eprintln!("fixture missing, skipping");
        return;
    }
    let mac = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    // The stick already has an older library (the demo export) and a stale OneLibrary db.
    copy_dir(&demo.join("PIONEER"), &stick.path().join("PIONEER"));
    std::fs::write(
        stick.path().join("PIONEER/rekordbox/exportLibrary.db"),
        b"old",
    )
    .unwrap();
    let old =
        Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap())
            .unwrap();

    // Library on the Mac: two tracks using the demo analysis, one without.
    let mut tracks = Vec::new();
    for (i, t) in old.tracks.iter().enumerate() {
        let audio = mac.path().join(format!("Hudba/Track {i} – Příliš.mp3"));
        std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
        std::fs::write(&audio, vec![i as u8; 5000 + i]).unwrap();
        tracks.push(SyncTrack {
            key: 100 + i as i64,
            audio,
            title: t.title.clone(),
            artist: "Umělec: X?".into(),
            album: String::new(),
            bpm: t.bpm(),
            duration: t.duration as f64,
            key_name: "8A".into(),
            anlz_dat: Some(demo.join(t.analyze_path.trim_start_matches('/'))),
            cues: vec![CuePoint {
                comment: "Drop".into(),
                hot_color: 0x2a,
                ..CuePoint::hot(1, 30_000)
            }],
            ..Default::default()
        });
    }
    let bare = mac.path().join("Hudba/bare.wav");
    std::fs::write(&bare, vec![7u8; 1234]).unwrap();
    tracks.push(SyncTrack {
        key: 200,
        audio: bare,
        title: "Bez analýzy".into(),
        ..Default::default()
    });
    let playlists = vec![
        SyncPlaylist {
            key: 1,
            parent: 0,
            name: "Sety".into(),
            is_folder: true,
            sort: 0,
            tracks: vec![],
        },
        SyncPlaylist {
            key: 2,
            parent: 1,
            name: "Klub X".into(),
            is_folder: false,
            sort: 0,
            tracks: vec![101, 100, 200],
        },
    ];
    let req = SyncRequest {
        stick: stick.path().to_path_buf(),
        tracks,
        playlists,
        device_name: "TEST".into(),
        date: "2026-10-14".into(),
    };

    let p = plan(&req);
    assert_eq!((p.copy_files, p.reuse_files), (3, 0));
    assert_eq!(p.no_analysis.len(), 1);
    let r = execute(&req, &p, backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!((r.tracks, r.playlists, r.copied), (3, 1, 3));
    assert_eq!(
        r.moved_aside,
        vec!["exportExt.pdb".to_string(), "exportLibrary.db".to_string()]
    );
    assert!(
        backup.path().join("PIONEER/rekordbox/export.pdb").is_file(),
        "old database backed up"
    );

    let db =
        Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap())
            .unwrap();
    assert!(db.problems.is_empty());
    let set = db.playlists.iter().find(|p| p.name == "Klub X").unwrap();
    let folder = db.playlists.iter().find(|p| p.name == "Sety").unwrap();
    assert_eq!(set.parent_id, folder.id);
    let order: Vec<String> = db
        .playlist_tracks(set.id)
        .iter()
        .map(|id| db.track(*id).unwrap().title.clone())
        .collect();
    assert_eq!(order[2], "Bez analýzy");
    for t in &db.tracks {
        assert!(
            stick
                .path()
                .join(t.file_path.trim_start_matches('/'))
                .is_file(),
            "{}",
            t.file_path
        );
        assert!(
            !t.file_path.contains('?') && !t.file_path.contains(':'),
            "{}",
            t.file_path
        );
    }
    // Analysis points at the stick path and carries the library's cues.
    let t = db
        .tracks
        .iter()
        .find(|t| !t.analyze_path.is_empty())
        .unwrap();
    let dat = AnlzFile::parse(
        &std::fs::read(stick.path().join(t.analyze_path.trim_start_matches('/'))).unwrap(),
    )
    .unwrap();
    assert_eq!(dat.path().unwrap(), t.file_path);
    let ext = AnlzFile::parse(
        &std::fs::read(
            stick
                .path()
                .join(t.analyze_path.trim_start_matches('/'))
                .with_extension("EXT"),
        )
        .unwrap(),
    )
    .unwrap();
    let cues = read_cue_points(&dat, Some(&ext)).unwrap();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].comment, "Drop");

    // Second sync: nothing to copy.
    let p2 = plan(&req);
    assert_eq!((p2.copy_files, p2.reuse_files), (0, 3));
    execute(&req, &p2, backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
}
