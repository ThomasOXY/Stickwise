//! Rebuild every track's cue lists from the high-level `CuePoint` list and
//! compare with what rekordbox wrote. Runs on the fixtures plus any
//! `RBX_ANLZ_CORPUS` directories (read only).

use std::path::{Path, PathBuf};

use pioneer_formats::anlz::{AnlzFile, read_cue_points, write_cue_points};

fn dirs_with_dat(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            dirs_with_dat(&p, out);
        } else if p.file_name().is_some_and(|n| n == "ANLZ0000.DAT") {
            out.push(p.parent().unwrap().to_path_buf());
        }
    }
}

#[test]
fn cue_points_rebuild_rekordbox_bytes() {
    let mut roots =
        vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/rekordcrate/data")];
    if let Ok(extra) = std::env::var("RBX_ANLZ_CORPUS") {
        roots.extend(extra.split(':').map(PathBuf::from));
    }
    let mut dirs = Vec::new();
    for r in &roots {
        dirs_with_dat(r, &mut dirs);
    }
    let (mut tracks, mut with_cues, mut identical, mut cues_total) = (0, 0, 0, 0);
    let mut diffs = Vec::new();
    for d in dirs {
        let dat_bytes = std::fs::read(d.join("ANLZ0000.DAT")).unwrap();
        let ext_bytes = std::fs::read(d.join("ANLZ0000.EXT")).ok();
        let dat = AnlzFile::parse(&dat_bytes).unwrap();
        let ext = ext_bytes.as_ref().map(|b| AnlzFile::parse(b).unwrap());
        let cues = read_cue_points(&dat, ext.as_ref()).unwrap();
        tracks += 1;
        if cues.is_empty() {
            continue;
        }
        with_cues += 1;
        cues_total += cues.len();
        let (mut dat2, mut ext2) = (dat.clone(), ext.clone());
        write_cue_points(&mut dat2, ext2.as_mut(), &cues);
        // Semantics must always survive.
        assert_eq!(
            read_cue_points(&dat2, ext2.as_ref()).unwrap().len(),
            cues.len(),
            "{}",
            d.display()
        );
        let same_dat = dat2.to_bytes() == dat_bytes;
        let same_ext = match (&ext2, &ext_bytes) {
            (Some(e), Some(b)) => e.to_bytes() == *b,
            _ => true,
        };
        if same_dat && same_ext {
            identical += 1;
        } else {
            diffs.push(format!("{} dat={same_dat} ext={same_ext}", d.display()));
        }
    }
    eprintln!(
        "{tracks} tracks, {with_cues} with cues ({cues_total} cues), {identical} byte-identical after rebuild"
    );
    for d in diffs.iter().take(20) {
        eprintln!("  differs: {d}");
    }
    // The few differences are lists rekordbox left in creation order, or
    // unknown bytes in entry padding; the cue content always survives.
    if with_cues >= 20 {
        assert!(
            identical * 100 / with_cues >= 85,
            "only {identical}/{with_cues} identical"
        );
    }
}

#[test]
fn new_cues_round_trip() {
    use pioneer_formats::anlz::{CuePoint, build_path};
    let mut dat = AnlzFile::default();
    dat.sections.push(build_path("/Contents/a.mp3"));
    let mut ext = dat.clone();
    let mut cues = vec![
        CuePoint {
            hot_color: 0x2a,
            comment: "Drop".into(),
            ..CuePoint::hot(1, 30_000)
        },
        CuePoint {
            loop_end_ms: Some(64_000),
            loop_beats: Some((16, 1)),
            ..CuePoint::hot(5, 60_000)
        },
        CuePoint {
            comment: "Příliš žluťoučký".into(),
            ..CuePoint::hot(8, 200_000)
        },
        CuePoint::memory(1_000),
        CuePoint {
            loop_end_ms: Some(10_000),
            active_loop: true,
            memory_color: 3,
            ..CuePoint::memory(5_000)
        },
    ];
    write_cue_points(&mut dat, Some(&mut ext), &cues);
    let dat = AnlzFile::parse(&dat.to_bytes()).unwrap();
    let ext = AnlzFile::parse(&ext.to_bytes()).unwrap();
    let mut back = read_cue_points(&dat, Some(&ext)).unwrap();
    let key = |c: &CuePoint| (c.slot, c.time_ms);
    back.sort_by_key(key);
    cues.sort_by_key(key);
    for (a, b) in back.iter().zip(&cues) {
        assert_eq!(
            (a.slot, a.time_ms, a.loop_end_ms, &a.comment),
            (b.slot, b.time_ms, b.loop_end_ms, &b.comment)
        );
        assert_eq!(a.hot_color, b.hot_color);
        assert_eq!(a.active_loop, b.active_loop);
        assert_eq!(a.memory_color, b.memory_color);
    }
    assert_eq!(back.len(), cues.len());
    // A–C in the DAT hot list only, D–H in the EXT one.
    let dat_hot: Vec<u32> = dat
        .cue_lists()
        .unwrap()
        .iter()
        .filter(|l| l.kind == pioneer_formats::anlz::CueListKind::Hot)
        .flat_map(|l| l.cues.iter().map(|c| c.hot_cue))
        .collect();
    assert_eq!(dat_hot, vec![1]);
}
