use std::path::Path;
use std::sync::atomic::AtomicBool;

use pioneer_formats::anlz::{AnlzFile, CuePoint, read_cue_points};
use pioneer_formats::pdb::Database;
use pioneer_formats::setting::MySettings;
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
        engine: Some(engine_library::Schema::V3_0_1),
        settings: Some({
            let mut s = MySettings::default();
            s.set("eject_lock", 0x81).unwrap();
            s
        }),
    };

    let p = plan(&req);
    assert_eq!((p.copy_files, p.reuse_files), (3, 0));
    assert_eq!(p.no_analysis.len(), 1);
    let mut fractions = Vec::new();
    let r = execute(&req, &p, backup.path(), &AtomicBool::new(false), |e| {
        fractions.push(e.fraction)
    })
    .unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert!(
        fractions.windows(2).all(|w| w[0] <= w[1])
            && fractions.iter().all(|f| (0.0..=1.0).contains(f)),
        "progress only moves forward: {fractions:?}"
    );
    assert!(r.settings_written);
    let on_stick = usb_tools::settings::read(stick.path());
    assert_eq!(on_stick.found.len(), 4);
    assert_eq!(on_stick.settings.get("eject_lock"), Some(0x81));

    // The Denon library points at the very same audio files.
    let engine = r.engine.as_ref().expect("engine library written");
    assert_eq!(engine.tracks, 3);
    let edb = engine_library::read(&stick.path().join("Engine Library/Database2/m.db")).unwrap();
    assert_eq!(edb.schema, Some(engine_library::Schema::V3_0_1));
    let sety = edb.children(0)[0];
    assert_eq!(sety.title, "Sety");
    let klub = edb.children(sety.id)[0];
    let titles: Vec<String> = klub
        .tracks
        .iter()
        .map(|id| {
            edb.tracks
                .iter()
                .find(|t| t.id == *id)
                .unwrap()
                .title
                .clone()
        })
        .collect();
    assert_eq!(titles[2], "Bez analýzy");
    for t in &edb.tracks {
        let rel = t
            .path
            .strip_prefix("../")
            .expect("relative to Engine Library");
        assert!(stick.path().join(rel).is_file(), "{}", t.path);
    }
    let analysed = edb
        .tracks
        .iter()
        .find(|t| t.beats.is_set)
        .expect("beat grid carried over");
    assert_eq!(analysed.beats.default_grid[0].beat_index, -4);
    assert_eq!(analysed.cues.cues[0].label, "Drop");
    assert_eq!(
        analysed.cues.cues[0].sample_offset,
        30.0 * analysed.track_data.sample_rate
    );
    assert_eq!(analysed.key, Some(1));
    let uuid = edb.uuid.clone();
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
    let r2 = execute(&req, &p2, backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r2.warnings.is_empty(), "{:?}", r2.warnings);
    // Nor are the analysis files written again: they did not change.
    assert_eq!((r.anlz_written, r.anlz_unchanged), (2, 0));
    assert_eq!((r2.anlz_written, r2.anlz_unchanged), (0, 2));
    // Nor when the manifest has no fingerprints (written by an older version):
    // the files on the stick are compared.
    let mpath = stick.path().join(".rbx/manifest.json");
    let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(&mpath).unwrap()).unwrap();
    for t in m["tracks"].as_object_mut().unwrap().values_mut() {
        t.as_object_mut().unwrap().remove("anlz");
    }
    std::fs::write(&mpath, serde_json::to_vec(&m).unwrap()).unwrap();
    let r2b = execute(&req, &plan(&req), backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((r2b.anlz_written, r2b.anlz_unchanged), (0, 2));
    // The Denon library keeps its identity (play history stays linked).
    let edb = engine_library::read(&stick.path().join("Engine Library/Database2/m.db")).unwrap();
    assert_eq!(edb.uuid, uuid);
    assert!(
        backup
            .path()
            .join("Engine Library/Database2/m.db")
            .is_file()
    );

    // A cue edited in the library: only that track's analysis is rewritten.
    let mut req = req;
    req.tracks[0].cues[0].comment = "Break".into();
    let r3 = execute(&req, &plan(&req), backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((r3.anlz_written, r3.anlz_unchanged), (1, 1));
    let db =
        Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap())
            .unwrap();
    let t = db.tracks.iter().find(|t| t.title == req.tracks[0].title).unwrap();
    let anlz = stick.path().join(t.analyze_path.trim_start_matches('/'));
    let dat = AnlzFile::parse(&std::fs::read(&anlz).unwrap()).unwrap();
    let ext = AnlzFile::parse(&std::fs::read(anlz.with_extension("EXT")).unwrap()).unwrap();
    assert_eq!(read_cue_points(&dat, Some(&ext)).unwrap()[0].comment, "Break");

    // The same song in the library twice, from another folder: it shares the
    // file on the stick, and the Denon library still gets written.
    let dup = mac.path().join("Jinde").join(req.tracks[0].audio.file_name().unwrap());
    std::fs::create_dir_all(dup.parent().unwrap()).unwrap();
    std::fs::copy(&req.tracks[0].audio, &dup).unwrap();
    req.tracks.push(SyncTrack {
        key: 300,
        audio: dup,
        ..req.tracks[0].clone()
    });
    req.playlists[1].tracks.push(300);
    let p4 = plan(&req);
    assert_eq!(p4.copy_files, 0);
    let r4 = execute(&req, &p4, backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r4.warnings.is_empty(), "{:?}", r4.warnings);
    assert_eq!(r4.engine.as_ref().expect("engine library written").tracks, 3);
}

#[test]
fn analysis_damaged_on_the_stick_is_written_again() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/rekordcrate/data/complete_export/demo_tracks");
    if !demo.exists() {
        eprintln!("fixture missing, skipping");
        return;
    }
    let mac = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    let old =
        Database::parse(&std::fs::read(demo.join("PIONEER/rekordbox/export.pdb")).unwrap()).unwrap();
    let t = &old.tracks[0];
    let audio = mac.path().join("a.mp3");
    std::fs::write(&audio, vec![1u8; 4000]).unwrap();
    let req = SyncRequest {
        stick: stick.path().to_path_buf(),
        tracks: vec![SyncTrack {
            key: 1,
            audio,
            title: "A".into(),
            artist: "A".into(),
            anlz_dat: Some(demo.join(t.analyze_path.trim_start_matches('/'))),
            ..Default::default()
        }],
        playlists: vec![SyncPlaylist {
            key: 10,
            parent: 0,
            name: "Set".into(),
            is_folder: false,
            sort: 0,
            tracks: vec![1],
        }],
        device_name: "TEST".into(),
        date: "2026-10-31".into(),
        engine: None,
        settings: None,
    };
    execute(&req, &plan(&req), backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    let db =
        Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap())
            .unwrap();
    let ext = stick
        .path()
        .join(db.tracks[0].analyze_path.trim_start_matches('/'))
        .with_extension("EXT");
    let good = std::fs::read(&ext).unwrap();
    // Truncated by a stick pulled out mid-write elsewhere, or deleted.
    std::fs::write(&ext, &good[..good.len() / 2]).unwrap();
    let r = execute(&req, &plan(&req), backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.anlz_written, 1);
    assert_eq!(std::fs::read(&ext).unwrap(), good);
}

#[test]
fn unreadable_track_is_left_out_not_the_sync() {
    use std::os::unix::fs::PermissionsExt;
    let mac = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    let mut tracks = Vec::new();
    for (key, name) in [(1, "ok.mp3"), (2, "broken.mp3"), (3, "ok2.mp3")] {
        let audio = mac.path().join(name);
        std::fs::write(&audio, vec![key as u8; 3000]).unwrap();
        tracks.push(SyncTrack {
            key,
            audio,
            title: name.into(),
            artist: "A".into(),
            ..Default::default()
        });
    }
    // Planned while readable, unreadable by the time it is copied.
    let req = SyncRequest {
        stick: stick.path().to_path_buf(),
        tracks,
        playlists: vec![SyncPlaylist {
            key: 10,
            parent: 0,
            name: "Set".into(),
            is_folder: false,
            sort: 0,
            tracks: vec![1, 2, 3],
        }],
        device_name: "TEST".into(),
        date: "2026-10-31".into(),
        engine: None,
        settings: None,
    };
    let p = plan(&req);
    let broken = mac.path().join("broken.mp3");
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o000)).unwrap();
    let r = execute(&req, &p, backup.path(), &AtomicBool::new(false), |_| {});
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o644)).unwrap();
    let r = r.unwrap();
    assert_eq!(r.skipped.len(), 1, "{:?}", r.skipped);
    assert!(r.skipped[0].contains("broken.mp3"));
    assert_eq!((r.copied, r.tracks), (2, 2));
    let db = Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap()).unwrap();
    let titles: Vec<_> = db.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["ok.mp3", "ok2.mp3"]);
    assert_eq!(db.playlist_entries.len(), 2);
    // No half-copied file left behind.
    let leftovers: Vec<_> = walk(stick.path()).into_iter().filter(|p| p.contains("rbx-part")).collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p.display().to_string());
        }
    }
    out
}

#[test]
fn analysis_already_on_the_stick_is_used_for_an_unanalysed_track() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/rekordcrate/data/complete_export/demo_tracks");
    if !demo.exists() {
        eprintln!("fixture missing, skipping");
        return;
    }
    let mac = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    // A stick rekordbox exported to: the audio and its analysis are there.
    copy_dir(&demo.join("PIONEER"), &stick.path().join("PIONEER"));
    let old =
        Database::parse(&std::fs::read(demo.join("PIONEER/rekordbox/export.pdb")).unwrap()).unwrap();
    let t = &old.tracks[0];
    // rekordbox put a hot cue on it.
    let stick_dat = stick.path().join(t.analyze_path.trim_start_matches('/'));
    let mut dat = AnlzFile::parse(&std::fs::read(&stick_dat).unwrap()).unwrap();
    let mut ext = AnlzFile::parse(&std::fs::read(stick_dat.with_extension("EXT")).unwrap()).unwrap();
    pioneer_formats::anlz::write_cue_points(&mut dat, Some(&mut ext), &[CuePoint::hot(1, 12_000)]);
    std::fs::write(&stick_dat, dat.to_bytes()).unwrap();
    std::fs::write(stick_dat.with_extension("EXT"), ext.to_bytes()).unwrap();
    let on_stick = stick.path().join(t.file_path.trim_start_matches('/'));
    std::fs::create_dir_all(on_stick.parent().unwrap()).unwrap();
    std::fs::write(&on_stick, vec![3u8; t.file_size as usize]).unwrap();
    // The same file in the library, never analysed there.
    let audio = mac.path().join(Path::new(&t.file_path).file_name().unwrap());
    std::fs::write(&audio, vec![3u8; t.file_size as usize]).unwrap();
    let req = SyncRequest {
        stick: stick.path().to_path_buf(),
        tracks: vec![SyncTrack {
            key: 1,
            audio,
            title: t.title.clone(),
            artist: "A".into(),
            ..Default::default()
        }],
        playlists: vec![SyncPlaylist {
            key: 10,
            parent: 0,
            name: "Set".into(),
            is_folder: false,
            sort: 0,
            tracks: vec![1],
        }],
        device_name: "TEST".into(),
        date: "2026-10-31".into(),
        engine: None,
        settings: None,
    };
    let p = plan(&req);
    assert_eq!(p.reuse_files, 1);
    assert!(p.no_analysis.is_empty(), "{:?}", p.no_analysis);
    assert!(p.tracks[0].stick_anlz.is_some());

    let r = execute(&req, &p, backup.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let db =
        Database::parse(&std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap())
            .unwrap();
    let anlz = stick.path().join(db.tracks[0].analyze_path.trim_start_matches('/'));
    let dat = AnlzFile::parse(&std::fs::read(&anlz).unwrap()).unwrap();
    let ext = AnlzFile::parse(&std::fs::read(anlz.with_extension("EXT")).unwrap()).unwrap();
    assert!(matches!(dat.beat_grid(), Some(Ok(_))), "waveform and grid carried over");
    // The library has no cues of its own for it: the stick's stay.
    let cues = read_cue_points(&dat, Some(&ext)).unwrap();
    assert_eq!((cues.len(), cues[0].time_ms), (1, 12_000));
}
