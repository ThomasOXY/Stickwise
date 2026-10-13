//! Round-trip every ANLZ file found under the fixture directories, plus any
//! directory listed in `RBX_ANLZ_CORPUS` (colon separated, read only).
//!
//! For each file: parse → write must give the original bytes, and every
//! typed section (cues, grid, path, waveforms) must re-encode to its raw bytes.

use std::path::{Path, PathBuf};

use pioneer_formats::anlz::{
    AnlzFile, BeatGrid, CueList, ExtCueList, Waveform, WaveformKind, build_path, parse_path, tag,
};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            // .3EX holds rekordbox 7 recommendation embeddings (MessagePack), not analysis.
            n.starts_with("ANLZ") && [".DAT", ".EXT", ".2EX"].iter().any(|x| n.ends_with(x))
        }) {
            out.push(p);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/rekordcrate/data");
    let mut dirs = vec![root];
    if let Ok(extra) = std::env::var("RBX_ANLZ_CORPUS") {
        dirs.extend(extra.split(':').map(PathBuf::from));
    }
    let mut files = Vec::new();
    for d in dirs {
        collect(&d, &mut files);
    }
    files
}

#[test]
fn anlz_files_round_trip() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("no ANLZ fixtures found, skipping");
        return;
    }
    let mut failures = Vec::new();
    let mut cues = 0usize;
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let mut fail = |what: String| failures.push(format!("{}: {what}", f.display()));
        let a = match AnlzFile::parse(&bytes) {
            Ok(a) => a,
            Err(e) => {
                fail(format!("parse: {e}"));
                continue;
            }
        };
        if a.to_bytes() != bytes {
            fail("container rewrite differs".into());
        }
        for s in &a.sections {
            let re = match &s.tag {
                b"PCOB" => CueList::parse(s).map(|l| {
                    cues += l.cues.len();
                    l.to_section()
                }),
                b"PCO2" => ExtCueList::parse(s).map(|l| l.to_section()),
                b"PQTZ" => BeatGrid::parse(s).map(|g| g.to_section()),
                b"PPTH" => parse_path(s).map(|p| build_path(&p)),
                t if WaveformKind::from_tag(*t).is_some() => {
                    Waveform::parse(s).map(|w| w.to_section())
                }
                _ => continue,
            };
            match re {
                Ok(re) if re == *s => {}
                Ok(re) => fail(format!(
                    "{} re-encodes differently (header {:02x?} vs {:02x?}, body {}B vs {}B, first diff {:?})",
                    s.tag_str(),
                    re.header,
                    s.header,
                    re.body.len(),
                    s.body.len(),
                    re.body.iter().zip(&s.body).position(|(a, b)| a != b)
                )),
                Err(e) => fail(format!("{}: {e}", s.tag_str())),
            }
        }
        let _ = a.section(tag::PATH);
    }
    eprintln!("checked {} ANLZ files, {} PCOB cues", files.len(), cues);
    if !failures.is_empty() {
        for f in failures.iter().take(30) {
            eprintln!("{f}");
        }
        panic!("{} of {} files failed", failures.len(), files.len());
    }
}
