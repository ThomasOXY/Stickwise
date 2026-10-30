//! Raw Beat This! output: `beats <models dir> <audio>`.
use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = Path::new(&a[1]);
    let t = analyzer::tracker::Tracker::load(&d.join("mel_spectrogram.onnx"), &d.join("beat_this_small.onnx")).unwrap();
    let au = analyzer::decode::decode(Path::new(&a[2])).unwrap();
    let (b, db) = t.beats(&au.samples, au.rate).unwrap();
    println!("rate {} dur {:.1} beats {} downbeats {}", au.rate, au.duration(), b.len(), db.len());
    let ibi: Vec<String> = b.windows(2).skip(100).take(16).map(|w| format!("{:.3}", w[1] - w[0])).collect();
    println!("beats[100..] {:?}", &b[100..110]);
    println!("ibi {}", ibi.join(" "));
    println!("fit {:?}", analyzer::beats::fit(&b, &db));
}
