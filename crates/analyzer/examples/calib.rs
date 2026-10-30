//! Waveform scale calibration: rekordbox values vs our measurements. `calib <tsv>`
use std::path::Path;
use analyzer::{bands, decode};
use pioneer_formats::anlz::AnlzFile;
fn secs(p: &str) -> Option<AnlzFile> { AnlzFile::parse(&std::fs::read(p).ok()?).ok() }
fn pct(mut v: Vec<f64>, q: f64) -> f64 { v.sort_by(|a, b| a.total_cmp(b)); v[((v.len() - 1) as f64 * q) as usize] }
fn main() {
    let tsv = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    for line in tsv.lines().take(12) {
        let f: Vec<&str> = line.split('\t').collect();
        let base = &f[1][..f[1].len() - 3];
        let (Some(ext), Some(two)) = (secs(&format!("{base}EXT")), secs(&format!("{base}2EX"))) else { continue };
        let Ok(a) = decode::decode(Path::new(f[0])) else { continue };
        let fr = bands::frames(&a.samples, a.rate, 150.0);
        let p7 = &two.section(*b"PWV7").unwrap().body;
        let p3 = &ext.section(*b"PWV3").unwrap().body;
        let p5 = &ext.section(*b"PWV5").unwrap().body;
        let n = (p7.len() / 3).min(fr.low.len()).min(p3.len());
        let col = |i: usize| (p7[i*3] as f64, p7[i*3+1] as f64, p7[i*3+2] as f64);
        let rb_mid: Vec<f64> = (0..n).map(|i| col(i).0).collect();
        let rb_hi: Vec<f64> = (0..n).map(|i| col(i).1).collect();
        let rb_lo: Vec<f64> = (0..n).map(|i| col(i).2).collect();
        let rb_h3: Vec<f64> = (0..n).map(|i| (p3[i] & 31) as f64).collect();
        let rb_w3: Vec<f64> = (0..n).map(|i| (p3[i] >> 5) as f64).collect();
        let w5 = |i: usize| ((p5[i*2] as u16) << 8) | p5[i*2+1] as u16;
        let rb_h5: Vec<f64> = (0..n).map(|i| ((w5(i) >> 2) & 31) as f64).collect();
        let ours = |v: &Vec<f32>| v[..n].iter().map(|&x| x as f64).collect::<Vec<_>>();
        let (ol, om, oh, op) = (ours(&fr.low), ours(&fr.mid), ours(&fr.high), ours(&fr.peak));
        if std::env::var("COLS").is_ok() {
            let mx = p7.iter().max().unwrap();
            println!("PWV7 max {mx}");
            for i in (n / 3..n / 3 + 3000).step_by(300) {
                let w = w5(i);
                let (r, g, b) = ((w >> 13) & 7, (w >> 10) & 7, (w >> 7) & 7);
                let (l, m, h) = (ol[i] * 118.0, om[i] * 380.0, oh[i] * 470.0);
                println!("  rgb {r}{g}{b} h5 {:2} w3 {} | ours l {:3.0} m {:3.0} h {:3.0} pk {:4.1} | rb l {} m {} h {}", (w >> 2) & 31, p3[i] >> 5, l, m, h, op[i] * 29.0, col(i).2, col(i).0, col(i).1);
            }
            continue;
        }
        // Scale = rb p95 / ours p95.
        println!("lo {:5.0} mid {:5.0} hi {:5.0} | h3 {:5.1} h5 {:5.1} (per peak) w3 mean {:.2} | rb p95 lo {} mid {} hi {} h3 {}",
            pct(rb_lo.clone(), 0.95) / pct(ol.clone(), 0.95), pct(rb_mid.clone(), 0.95) / pct(om.clone(), 0.95), pct(rb_hi.clone(), 0.95) / pct(oh.clone(), 0.95),
            pct(rb_h3.clone(), 0.95) / pct(op.clone(), 0.95), pct(rb_h5.clone(), 0.95) / pct(op.clone(), 0.95),
            rb_w3.iter().sum::<f64>() / n as f64, pct(rb_lo, 0.95), pct(rb_mid, 0.95), pct(rb_hi, 0.95), pct(rb_h3, 0.95));
    }
}
