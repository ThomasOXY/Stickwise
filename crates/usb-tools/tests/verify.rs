#![allow(clippy::field_reassign_with_default)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use pioneer_formats::pdb::{Database, Track, WriteOptions, write};
use usb_tools::Stick;
use usb_tools::compat::Player;
use usb_tools::verify::{Problem, VerifyCache, verify_cached};

/// A stick with one WAV-looking track.
fn fake_stick(root: &Path) -> std::path::PathBuf {
    let mut db = Database::default();
    let path = "/Contents/a.wav";
    let file = root.join(path.trim_start_matches('/'));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    wav.resize(4000, 0);
    std::fs::write(&file, &wav).unwrap();
    db.tracks.push(Track {
        title: "A".into(),
        file_path: path.into(),
        ..Track::new_export(1)
    });
    let pdb = root.join("PIONEER/rekordbox/export.pdb");
    std::fs::create_dir_all(pdb.parent().unwrap()).unwrap();
    std::fs::write(pdb, write(&db, &WriteOptions::default())).unwrap();
    file
}

fn unreadable(r: &usb_tools::verify::VerifyReport) -> bool {
    r.issues
        .iter()
        .any(|i| matches!(i.problem, Problem::Unreadable(_)))
}

#[test]
fn unchanged_files_are_not_read_again() {
    let usb = tempfile::tempdir().unwrap();
    let audio = fake_stick(usb.path());
    let stick = Stick::open(usb.path()).unwrap();
    let cancel = AtomicBool::new(false);
    let mut cache = VerifyCache::default();

    let first = verify_cached(&stick, &Player::ALL, &mut cache, &cancel, |_, _| {});
    assert!(!unreadable(&first));

    // Survives a restart of the app.
    let mut cache: VerifyCache =
        serde_json::from_slice(&serde_json::to_vec(&cache).unwrap()).unwrap();

    // Same size and time: the cached result is used, the file is not opened.
    std::fs::set_permissions(&audio, std::fs::Permissions::from_mode(0o000)).unwrap();
    let second = verify_cached(&stick, &Player::ALL, &mut cache, &cancel, |_, _| {});
    assert!(!unreadable(&second));
    assert_eq!(first.formats, second.formats);
    assert_eq!(first.issues.len(), second.issues.len());

    // A changed file is read again.
    std::fs::set_permissions(&audio, std::fs::Permissions::from_mode(0o644)).unwrap();
    let f = std::fs::OpenOptions::new().append(true).open(&audio).unwrap();
    f.set_len(5000).unwrap();
    drop(f);
    std::fs::set_permissions(&audio, std::fs::Permissions::from_mode(0o000)).unwrap();
    let third = verify_cached(&stick, &Player::ALL, &mut cache, &cancel, |_, _| {});
    std::fs::set_permissions(&audio, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(unreadable(&third));
}
