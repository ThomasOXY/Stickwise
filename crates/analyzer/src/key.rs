//! Musical key: the track's average pitch-class profile (chroma), matched
//! against key profiles of all 24 major and minor keys.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// Names as rekordbox writes them, by pitch class from C.
const MAJOR: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MINOR: [&str; 12] = ["Cm", "C#m", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "G#m", "Am", "Bbm", "Bm"];

/// Key profiles: weight of each scale degree, from the tonic.
#[derive(Debug, Clone, Copy)]
pub struct Profiles {
    pub major: [f64; 12],
    pub minor: [f64; 12],
}

/// Temperley's (Kostka–Payne) profiles.
pub const TEMPERLEY: Profiles = Profiles {
    major: [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0],
    minor: [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0],
};

/// Krumhansl–Kessler probe-tone profiles.
pub const KRUMHANSL: Profiles = Profiles {
    major: [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88],
    minor: [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17],
};

/// Profiles tuned on electronic dance music (Faraldo et al., "edma").
pub const EDMA: Profiles = Profiles {
    major: [0.165, 0.047, 0.083, 0.067, 0.100, 0.093, 0.053, 0.132, 0.052, 0.074, 0.069, 0.064],
    minor: [0.172, 0.040, 0.076, 0.121, 0.051, 0.086, 0.051, 0.139, 0.075, 0.050, 0.058, 0.054],
};

/// Average chroma of the track, loud frames counting more.
pub fn chroma(samples: &[f32], rate: u32) -> [f64; 12] {
    let (n, hop) = (8192, 4096);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect();
    // Bin → pitch class, between ~130 Hz (C3) and ~2 kHz: lower bins are
    // too coarse to tell semitones apart.
    let class: Vec<Option<usize>> = (0..n / 2)
        .map(|k| {
            let hz = k as f64 * rate as f64 / n as f64;
            if !(130.0..2100.0).contains(&hz) {
                return None;
            }
            let midi = 69.0 + 12.0 * (hz / 440.0).log2();
            Some((midi.round() as i64).rem_euclid(12) as usize)
        })
        .collect();
    let mut total = [0f64; 12];
    let mut buf = vec![Complex::new(0f32, 0f32); n];
    let mut start = 0;
    while start + n <= samples.len() {
        for i in 0..n {
            buf[i] = Complex::new(samples[start + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut c = [0f64; 12];
        for (k, cl) in class.iter().enumerate() {
            if let Some(cl) = cl {
                c[*cl] += buf[k].norm() as f64;
            }
        }
        // Each frame normalised, weighed by its loudness (quiet intros count less).
        let sum: f64 = c.iter().sum();
        if sum > 0.0 {
            for i in 0..12 {
                total[i] += c[i] / sum * sum.sqrt();
            }
        }
        start += hop;
    }
    total
}

fn correlation(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let ma = a.iter().sum::<f64>() / 12.0;
    let mb = b.iter().sum::<f64>() / 12.0;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        num += (a[i] - ma) * (b[i] - mb);
        da += (a[i] - ma).powi(2);
        db += (b[i] - mb).powi(2);
    }
    if da == 0.0 || db == 0.0 { 0.0 } else { num / (da * db).sqrt() }
}

/// Best key for a chroma, as rekordbox names it ("Am", "F#", …).
pub fn best_key(c: &[f64; 12], p: &Profiles) -> Option<&'static str> {
    if c.iter().all(|&v| v == 0.0) {
        return None;
    }
    let mut best = (None, f64::MIN);
    for tonic in 0..12 {
        for (profile, names) in [(&p.major, &MAJOR), (&p.minor, &MINOR)] {
            let mut rotated = [0f64; 12];
            for i in 0..12 {
                rotated[(tonic + i) % 12] = profile[i];
            }
            let r = correlation(c, &rotated);
            if r > best.1 {
                best = (Some(names[tonic]), r);
            }
        }
    }
    best.0
}

pub fn detect(samples: &[f32], rate: u32) -> Option<&'static str> {
    // EDMA matched rekordbox best on a DJ's library (45 % exact, against
    // 37 % Temperley and 42 % Krumhansl).
    best_key(&chroma(samples, rate), &EDMA)
}
