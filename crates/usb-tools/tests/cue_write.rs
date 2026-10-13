use std::path::Path;

use pioneer_formats::anlz::{AnlzFile, CuePoint, read_cue_points};
use usb_tools::Stick;
use usb_tools::cue_write::write_cues;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

#[test]
fn writes_cues_with_backup() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/rekordcrate/data/complete_export/demo_tracks");
    if !src.exists() {
        eprintln!("fixture missing, skipping");
        return;
    }
    let usb = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    copy_dir(&src, usb.path());
    let stick = Stick::open(usb.path()).unwrap();
    let track = stick.db.tracks[0].clone();
    let cues = vec![
        CuePoint {
            hot_color: 0x2a,
            comment: "Drop".into(),
            ..CuePoint::hot(1, 32_000)
        },
        CuePoint {
            loop_end_ms: Some(48_000),
            ..CuePoint::hot(4, 40_000)
        },
        CuePoint::memory(1_000),
    ];
    let report = write_cues(&stick, &[(track.id, cues.clone())], backup.path());
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.written, vec![track.id]);

    let dat_path = stick.resolve(&track.analyze_path);
    let dat = AnlzFile::parse(&std::fs::read(&dat_path).unwrap()).unwrap();
    let ext = AnlzFile::parse(&std::fs::read(dat_path.with_extension("EXT")).unwrap()).unwrap();
    let back = read_cue_points(&dat, Some(&ext)).unwrap();
    assert_eq!(back.len(), 3);
    assert!(
        back.iter()
            .any(|c| c.slot == 1 && c.comment == "Drop" && c.hot_color == 0x2a)
    );
    assert!(
        back.iter()
            .any(|c| c.slot == 4 && c.loop_end_ms == Some(48_000))
    );

    // Originals backed up byte for byte; waveforms and grid untouched.
    let orig = std::fs::read(src.join(track.analyze_path.trim_start_matches('/'))).unwrap();
    assert_eq!(
        std::fs::read(
            backup
                .path()
                .join(track.analyze_path.trim_start_matches('/'))
        )
        .unwrap(),
        orig
    );
    let orig = AnlzFile::parse(&orig).unwrap();
    assert_eq!(
        orig.beat_grid().unwrap().unwrap(),
        dat.beat_grid().unwrap().unwrap()
    );
    // No temporary files left behind.
    let leftovers = std::fs::read_dir(dat_path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("rbx-new"))
        .count();
    assert_eq!(leftovers, 0);
}
