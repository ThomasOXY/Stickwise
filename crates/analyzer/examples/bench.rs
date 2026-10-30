//! Compare our analysis with rekordbox's: `bench <tsv>` with lines
//! `audio path \t ANLZ0000.DAT \t rekordbox key`.

use std::path::Path;

use analyzer::{BEAT_FPS, bands, beats, decode, key};
use pioneer_formats::anlz::AnlzFile;

fn main() {
    let tsv = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let (mut n, mut bpm_ok, mut grid_ok, mut bar_ok, mut key_ok, mut key_n) = (0, 0, 0, 0, 0, 0);
    let mut offsets = Vec::new();
    // `RBX_MODELS=<dir>` (mel_spectrogram.onnx + RBX_BEAT model): Beat This!.
    let tracker = std::env::var("RBX_MODELS").ok().map(|d| {
        let d = Path::new(&d).to_path_buf();
        let beat = std::env::var("RBX_BEAT").unwrap_or("beat_this.onnx".into());
        analyzer::tracker::Tracker::load(&d.join("mel_spectrogram.onnx"), &d.join(beat)).unwrap()
    });
    for line in tsv.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (audio, dat, rb_key) = (f[0], f[1], f.get(2).copied().unwrap_or(""));
        let Ok(b) = std::fs::read(dat) else { continue };
        let Some(Ok(rg)) = AnlzFile::parse(&b).ok().and_then(|a| a.beat_grid()) else { continue };
        if rg.beats.len() < 32 { continue; }
        let t = std::time::Instant::now();
        let Ok(a) = decode::decode(Path::new(audio)) else { println!("decode fail {audio}"); continue };
        let g = match &tracker {
            Some(t) => { let (b, d) = t.beats(&a.samples, a.rate).unwrap(); { let fr = bands::frames(&a.samples, a.rate, BEAT_FPS); if std::env::var("RBX_FIT_ONLY").is_ok() { beats::fit(&b, &d) } else { beats::refine(&fr, &b, &d) }.or_else(|| beats::detect(&fr, None)) } }
            None => beats::detect(&bands::frames(&a.samples, a.rate, BEAT_FPS), None),
        };
        let Some(g) = g else { println!("no grid {audio}"); continue };
        let k = key::detect(&a.samples, a.rate).unwrap_or("");
        let dt = t.elapsed();
        n += 1;
        let rb_bpm = rg.beats[rg.beats.len() / 2].tempo as f64 / 100.0;
        let ok = (g.bpm - rb_bpm).abs() < 0.05;
        if ok { bpm_ok += 1; }
        // Offset of rekordbox's beats from ours, mid-track.
        let rb_mid = rg.beats[rg.beats.len() / 2];
        let t_rb = rb_mid.time_ms as f64 / 1000.0;
        let p = g.period();
        let d = ((t_rb - g.first) / p).round();
        let off = (t_rb - (g.first + d * p)) * 1000.0;
        let ours_num = ((g.first_number as i64 - 1 + d as i64).rem_euclid(4) + 1) as u16;
        if ok && off.abs() < 15.0 { grid_ok += 1; offsets.push(off); }
        if ok && off.abs() < 15.0 && ours_num == rb_mid.beat_number { bar_ok += 1; }
        if !rb_key.is_empty() { key_n += 1; if camelot(rb_key) == camelot(k) { key_ok += 1; } }
        println!("{:>7.2} vs {:>7.2} off {:>7.1}ms bar {} vs {} key {:>4} vs {:>4} {:?} {}",
            g.bpm, rb_bpm, off, ours_num, rb_mid.beat_number, k, rb_key, dt, Path::new(audio).file_name().unwrap().to_string_lossy());
    }
    offsets.sort_by(|a, b| a.total_cmp(b));
    let med = offsets.get(offsets.len() / 2).copied().unwrap_or(0.0);
    println!("\n{n} tracks: bpm {bpm_ok}, grid ±15ms {grid_ok}, bar {bar_ok}, key {key_ok}/{key_n}, median offset {med:.1} ms");
}

fn camelot(k: &str) -> String {
    const T: [(&str, &str); 26] = [
        ("Abm", "1A"), ("G#m", "1A"), ("B", "1B"), ("Ebm", "2A"), ("D#m", "2A"), ("F#", "2B"), ("Gb", "2B"),
        ("Bbm", "3A"), ("A#m", "3A"), ("Db", "3B"), ("C#", "3B"), ("Fm", "4A"), ("Ab", "4B"), ("Cm", "5A"),
        ("Eb", "5B"), ("Gm", "6A"), ("Bb", "6B"), ("Dm", "7A"), ("F", "7B"), ("Am", "8A"), ("C", "8B"),
        ("Em", "9A"), ("G", "9B"), ("Bm", "10A"), ("D", "10B"), ("F#m", "11A"),
    ];
    const U: [(&str, &str); 6] = [("Gbm", "11A"), ("A", "11B"), ("C#m", "12A"), ("Dbm", "12A"), ("E", "12B"), ("Cb", "1B")];
    T.iter().chain(U.iter()).find(|(a, _)| *a == k).map(|(_, c)| c.to_string()).unwrap_or_else(|| k.to_string())
}
