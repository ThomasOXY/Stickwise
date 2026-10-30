//! Tempo and beat grid, assuming a steady tempo (club music).
//!
//! An onset envelope (how sharply the energy rises, mostly in the mids and highs)
//! is autocorrelated to find the tempo, then a comb of beats is slid over the
//! whole track to pin the exact tempo and phase. The downbeat is the beat in
//! the bar where the low end changes most: phrases start on bar ones.

use crate::bands::Frames;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    /// Seconds of the first beat at or after 0.
    pub first: f64,
    /// Position in the bar of that first beat, 1..=4.
    pub first_number: u8,
}

impl Grid {
    pub fn period(&self) -> f64 {
        60.0 / self.bpm
    }

    /// Every beat up to `duration`: (seconds, position in bar 1..=4).
    pub fn beats(&self, duration: f64) -> Vec<(f64, u8)> {
        let p = self.period();
        let mut out = Vec::new();
        let mut n = self.first_number;
        let mut k = 0;
        loop {
            let t = self.first + k as f64 * p;
            if t >= duration {
                break;
            }
            out.push((t, n));
            n = n % 4 + 1;
            k += 1;
        }
        out
    }
}

const MIN_BPM: f64 = 70.0;
const MAX_BPM: f64 = 180.0;

/// Weights of low, mid, high and the whole-signal peak in an onset envelope.
type Mix = [f32; 4];
/// Finding the tempo: mids and highs (snares, claps, hats) mark the beat most evenly.
const TEMPO_MIX: Mix = [0.0, 1.0, 1.0, 0.0];
/// Placing the beats: the attack, which the whole spectrum shows first.
const PHASE_MIX: Mix = [0.0, 1.0, 1.0, 0.0];

/// Positive, log-compressed rises in energy, minus their local average.
fn onset_envelope(f: &Frames, m: Mix) -> Vec<f32> {
    let e: Vec<f32> = (0..f.low.len())
        .map(|i| (1.0 + 100.0 * (m[0] * f.low[i] + m[1] * f.mid[i] + m[2] * f.high[i] + m[3] * f.peak[i])).ln())
        .collect();
    let mut o = vec![0f32; e.len()];
    for i in 1..e.len() {
        o[i] = (e[i] - e[i - 1]).max(0.0);
    }
    // Remove the slow trend (~0.25 s) so loud and quiet parts count alike.
    let w = (f.fps * 0.125) as usize;
    let mut prefix = vec![0f64; o.len() + 1];
    for (i, &v) in o.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v as f64;
    }
    let out: Vec<f32> = (0..o.len())
        .map(|i| {
            let a = i.saturating_sub(w);
            let b = (i + w + 1).min(o.len());
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (o[i] - mean as f32).max(0.0)
        })
        .collect();
    smooth(&out, 1.5)
}

/// Gaussian blur with `sigma` frames.
fn smooth(x: &[f32], sigma: f32) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as isize;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    (0..x.len() as isize)
        .map(|i| {
            let mut acc = 0.0;
            for (j, w) in k.iter().enumerate() {
                let at = i + j as isize - r;
                if at >= 0 && (at as usize) < x.len() {
                    acc += x[at as usize] * w;
                }
            }
            acc / sum
        })
        .collect()
}

#[inline]
fn at(x: &[f32], pos: f64) -> f32 {
    let i = pos.floor();
    if i < 0.0 {
        return 0.0;
    }
    let i = i as usize;
    if i + 1 >= x.len() {
        return 0.0;
    }
    let f = (pos - i as f64) as f32;
    x[i] * (1.0 - f) + x[i + 1] * f
}

fn autocorr(o: &[f32], lag: f64) -> f64 {
    let n = o.len();
    let l = lag as usize;
    if l + 1 >= n {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n - l - 1 {
        s += o[i] as f64 * at(o, i as f64 + lag) as f64;
    }
    s / (n - l) as f64
}

/// How well beats every `period` frames starting at `phase` hit onsets.
fn comb(o: &[f32], period: f64, phase: f64) -> f64 {
    let mut s = 0.0;
    let mut t = phase;
    let end = o.len() as f64;
    while t < end {
        s += at(o, t) as f64;
        t += period;
    }
    s
}

/// Best phase for `period` and its score.
fn best_phase(o: &[f32], period: f64, step: f64) -> (f64, f64) {
    let mut best = (0.0, f64::MIN);
    let mut ph = 0.0;
    while ph < period {
        let s = comb(o, period, ph);
        if s > best.1 {
            best = (ph, s);
        }
        ph += step;
    }
    best
}

/// Best phase within `reach` frames of `phase`, and its score.
fn best_near(o: &[f32], period: f64, phase: f64, reach: f64) -> (f64, f64) {
    let mut best = (phase, f64::MIN);
    let mut ph = phase - reach;
    while ph <= phase + reach {
        let s = comb(o, period, ph.rem_euclid(period));
        if s > best.1 {
            best = (ph.rem_euclid(period), s);
        }
        ph += 1.0;
    }
    best
}

/// Tempo with a preference for the range club music lives in.
fn coarse_tempo(o: &[f32], fps: f64) -> f64 {
    let prior = |bpm: f64| {
        // Wide: hip hop at 90 and drum & bass at 175 must still win.
        let x = (bpm / 125.0).log2() / 2.5;
        (-0.5 * x * x).exp()
    };
    let mut best = (120.0, f64::MIN);
    let mut bpm = MIN_BPM;
    while bpm <= MAX_BPM {
        let lag = fps * 60.0 / bpm;
        // The beat and its bar: both should repeat.
        let s = (autocorr(o, lag) + 0.5 * autocorr(o, lag * 2.0) + 0.25 * autocorr(o, lag * 4.0)) * prior(bpm);
        if s > best.1 {
            best = (bpm, s);
        }
        bpm += 0.25;
    }
    best.0
}

/// Tempo and grid. `hint`: BPM from the file's tags, trusted when it fits.
pub fn detect(f: &Frames, hint: Option<f64>) -> Option<Grid> {
    detect_with(f, hint, None)
}

/// [`detect`], or with `known` (a tempo from the neural tracker) only the
/// fine tempo and the phase are searched, close to it.
fn detect_with(f: &Frames, hint: Option<f64>, known: Option<f64>) -> Option<Grid> {
    let fps = f.fps;
    let o = onset_envelope(f, TEMPO_MIX);
    if o.len() < (fps * 10.0) as usize || o.iter().all(|&v| v == 0.0) {
        return None;
    }
    // The rough tempo needs no fine time resolution.
    let coarse = known.unwrap_or_else(|| {
        let small: Vec<f32> = o.chunks(4).map(|c| c.iter().copied().fold(0.0, f32::max)).collect();
        coarse_tempo(&small, fps / 4.0)
    });
    // A tag tempo near the found one (or its double / half) is more exact.
    let center = hint
        .filter(|h| (MIN_BPM / 1.5..MAX_BPM * 1.5).contains(h))
        .and_then(|h| {
            [h, h * 2.0, h / 2.0]
                .into_iter()
                .find(|c| (c / coarse - 1.0).abs() < 0.04)
        })
        .unwrap_or(coarse);
    let span = if known.is_some() {
        0.008
    } else if hint.is_some() && center != coarse {
        0.004
    } else {
        0.03
    };

    // Coarse then fine tempo steps, phase searched for each.
    let search = |lo: f64, hi: f64, step: f64, ph_step: f64| {
        let mut best = (center, 0.0, f64::MIN);
        let mut bpm = lo;
        while bpm <= hi {
            let period = fps * 60.0 / bpm;
            let (ph, s) = best_phase(&o, period, ph_step);
            // Longer periods hit fewer beats: compare per beat.
            let s = s * period;
            if s > best.2 {
                best = (bpm, ph, s);
            }
            bpm += step;
        }
        best
    };
    let (b1, _, _) = search(center * (1.0 - span), center * (1.0 + span), 0.05, 2.0);
    let (mut bpm, mut phase, score) = search(b1 - 0.06, b1 + 0.06, 0.01, 1.0);
    // Producers use whole tempos: take one when it fits as well.
    let whole = bpm.round();
    if (bpm - whole).abs() <= 0.06 {
        let period = fps * 60.0 / whole;
        let (ph, s) = best_phase(&o, period, 1.0);
        if s * period >= score * 0.985 {
            bpm = whole;
            phase = ph;
        }
    }
    let period = fps * 60.0 / bpm;
    // Beat or off-beat? Kick drums sit on the beat: more bass just after it.
    let bass = |ph: f64| {
        let w = (0.05 * fps) as usize;
        let mut t = ph;
        let mut s = 0.0;
        while (t as usize) + w < f.low.len() {
            s += f.low[t as usize..t as usize + w].iter().map(|&v| v as f64).sum::<f64>();
            t += period;
        }
        s
    };
    let half = (phase + period / 2.0).rem_euclid(period);
    // Only on clear evidence: hats and claps placed the phase well already.
    if bass(half) > bass(phase) * 1.4 {
        phase = half;
    }
    // Move the beats onto the attacks, within ±70 ms.
    let att = onset_envelope(f, PHASE_MIX);
    phase = best_near(&att, period, phase, 0.07 * fps).0;
    let o = att;
    // Sub-frame phase.
    let (mut lo, mut hi) = (phase - 1.0, phase + 1.0);
    for _ in 0..20 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if comb(&o, period, m1) < comb(&o, period, m2) {
            lo = m1;
        } else {
            hi = m2;
        }
    }
    phase = (lo + hi) / 2.0;
    let first = (phase / fps).rem_euclid(60.0 / bpm);

    let grid = Grid {
        bpm: (bpm * 100.0).round() / 100.0,
        first,
        first_number: 1,
    };
    Some(Grid {
        first_number: downbeat(f, &grid),
        ..grid
    })
}

/// Position in the bar (1..=4) of the grid's first beat.
fn downbeat(f: &Frames, g: &Grid) -> u8 {
    let duration = f.low.len() as f64 / f.fps;
    let beats = g.beats(duration);
    if beats.len() < 16 {
        return 1;
    }
    // Low-end level of each beat.
    let level: Vec<f64> = beats
        .iter()
        .map(|&(t, _)| {
            let a = (t * f.fps) as usize;
            let b = (((t + g.period()) * f.fps) as usize).min(f.low.len());
            if b <= a {
                return 0.0;
            }
            f.low[a..b].iter().map(|&v| v as f64).sum::<f64>() / (b - a) as f64
        })
        .collect();
    // Big changes in the low end (drops, breaks, bass in / out) land on bar ones.
    let mut score = [0f64; 4];
    for k in 1..level.len() {
        let change = (level[k] - level[k - 1]).abs();
        score[k % 4] += change;
    }
    let j = (0..4).max_by(|&a, &b| score[a].total_cmp(&score[b])).unwrap();
    // Beat j is a bar one, so the first beat sits (4 - j) % 4 + 1 in its bar.
    ((4 - j) % 4 + 1) as u8
}

/// Tempo and bar from the neural tracker's beats, the exact tempo and the
/// phase from the onsets as [`detect`] finds them: rekordbox places its
/// grids on the onsets, and matched them to the millisecond that way.
pub fn refine(f: &Frames, beats: &[f32], downbeats: &[f32]) -> Option<Grid> {
    let tracked = fit(beats, downbeats)?;
    let g = detect_with(f, None, Some(tracked.bpm))?;
    let period = g.period();
    let mut votes = [0usize; 4];
    for &d in downbeats {
        let k = ((d as f64 - g.first) / period).round();
        if k >= 0.0 && ((d as f64 - g.first) - k * period).abs() < 0.06 {
            votes[k as usize % 4] += 1;
        }
    }
    let Some(j) = (0..4).max_by_key(|&j| votes[j]).filter(|&j| votes[j] > 0) else {
        return Some(g);
    };
    Some(Grid {
        first_number: ((4 - j) % 4 + 1) as u8,
        ..g
    })
}

/// A steady grid through tracked beats, as rekordbox's grids are: tempo and
/// phase from a robust line fit over the whole track, the bar from where
/// most downbeats fall.
pub fn fit(beats: &[f32], downbeats: &[f32]) -> Option<Grid> {
    if beats.len() < 16 {
        return None;
    }
    let b: Vec<f64> = beats.iter().map(|&t| t as f64).collect();
    // The tracker works in 20 ms frames: single intervals are coarse, spans
    // of 8 beats are not.
    let mut spans: Vec<f64> = b.windows(9).map(|w| (w[8] - w[0]) / 8.0).collect();
    spans.sort_by(|x, y| x.total_cmp(y));
    let mut period = spans[spans.len() / 2];
    // Number the beats step by step (a missed beat skips a number), so the
    // count never drifts.
    let mut k = vec![0f64; b.len()];
    for i in 1..b.len() {
        k[i] = k[i - 1] + ((b[i] - b[i - 1]) / period).round().max(1.0);
    }
    let mut first_off: Vec<f64> = k.iter().zip(&b).map(|(&k, &t)| t - k * period).collect();
    first_off.sort_by(|x, y| x.total_cmp(y));
    let mut origin = first_off[first_off.len() / 2];
    // Line fit of beat time against beat number; beats far off the line
    // (fills, breaks) are dropped and the fit repeated.
    let mut tol = 0.08;
    for _ in 0..4 {
        let pts: Vec<(f64, f64)> = k
            .iter()
            .zip(&b)
            .map(|(&k, &t)| (k, t))
            .filter(|&(k, t)| (t - (origin + k * period)).abs() < tol)
            .collect();
        if pts.len() < 8 {
            return None;
        }
        let n = pts.len() as f64;
        let (sk, st) = pts.iter().fold((0.0, 0.0), |a, &(k, t)| (a.0 + k, a.1 + t));
        let (mk, mt) = (sk / n, st / n);
        let (num, den) = pts
            .iter()
            .fold((0.0, 0.0), |a, &(k, t)| (a.0 + (k - mk) * (t - mt), a.1 + (k - mk).powi(2)));
        if den <= 0.0 {
            return None;
        }
        period = num / den;
        origin = mt - period * mk;
        tol = (tol * 0.7).max(0.03);
    }
    // rekordbox keeps tempos within about 70–180 BPM (none outside on a
    // DJ's 4 400 analysed tracks): a tracker counting eighths is halved.
    while 60.0 / period >= 180.0 {
        // Of the two halves of the beats, the one the downbeats sit on.
        let on = |o: f64| {
            downbeats
                .iter()
                .filter(|&&d| {
                    let x = (d as f64 - o) / (2.0 * period);
                    (x - x.round()).abs() * 2.0 * period < 0.04
                })
                .count()
        };
        if on(origin + period) > on(origin) {
            origin += period;
        }
        period *= 2.0;
    }
    while 60.0 / period < 70.0 {
        period /= 2.0;
    }
    let mut bpm = 60.0 / period;
    // Whole tempos when the beats agree: producers set them, rekordbox shows them.
    let residual = |p: f64| {
        let mut d: Vec<f64> = b
            .iter()
            .map(|&t| {
                let x = (t - origin) / p;
                (x - x.round()).abs() * p
            })
            .collect();
        d.sort_by(|x, y| x.total_cmp(y));
        d[d.len() * 3 / 4]
    };
    let whole = bpm.round();
    if (bpm - whole).abs() < 0.1 && residual(60.0 / whole) <= residual(period) * 1.3 + 0.003 {
        bpm = whole;
    } else {
        bpm = (bpm * 100.0).round() / 100.0;
    }
    let period = 60.0 / bpm;
    // Phase for the final tempo: mean offset of the beats near the grid
    // (the 20 ms steps average out over hundreds of beats).
    let off: Vec<f64> = b
        .iter()
        .map(|&t| {
            let x = (t - origin) / period;
            (x - x.round()) * period
        })
        .filter(|o| o.abs() < 0.03)
        .collect();
    let origin = origin + if off.is_empty() { 0.0 } else { off.iter().sum::<f64>() / off.len() as f64 };
    let first = origin.rem_euclid(period);
    // Which beat of the bar the first one is: vote over the downbeats.
    let mut votes = [0usize; 4];
    for &d in downbeats {
        let k = ((d as f64 - first) / period).round();
        if k >= 0.0 {
            votes[k as usize % 4] += 1;
        }
    }
    let j = (0..4).max_by_key(|&j| votes[j]).unwrap_or(0);
    Some(Grid {
        bpm,
        first,
        first_number: ((4 - j) % 4 + 1) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Beats as the tracker gives them: on its 20 ms frame grid.
    fn tracked(bpm: f64, first: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|k| {
                let t = first + k as f64 * 60.0 / bpm;
                ((t / 0.02).round() * 0.02) as f32
            })
            .collect()
    }

    #[test]
    fn steady_grid_through_frame_rounded_beats() {
        let all = tracked(127.0, 0.217, 380);
        let downbeats: Vec<f32> = all.iter().copied().skip(2).step_by(4).collect();
        let mut b = all;
        b.remove(150); // a missed beat
        let g = fit(&b, &downbeats).unwrap();
        assert_eq!(g.bpm, 127.0);
        assert!((g.first - 0.217).abs() < 0.004, "first {}", g.first);
        // Beat 2 is a bar one, so the first beat is the bar's third.
        assert_eq!(g.first_number, 3);
    }

    #[test]
    fn eighths_fold_into_rekordbox_range() {
        let b = tracked(240.0, 0.1, 600);
        let downbeats: Vec<f32> = b.iter().copied().skip(1).step_by(8).collect();
        let g = fit(&b, &downbeats).unwrap();
        assert_eq!(g.bpm, 120.0);
        // The half the downbeats sit on: 0.1 s + one eighth (within the
        // tracker's frame, as these beats fall exactly between two).
        assert!((g.first - 0.35).abs() < 0.011, "first {}", g.first);
    }
}
