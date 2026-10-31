//! A steady beat grid as the DJ edits it: a tempo and one bar one it is
//! anchored on. Every beat of the track follows from the two, as in
//! rekordbox's grid edit mode.

/// Tempo and the time of one downbeat (beat 1 of a bar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    pub anchor_ms: f64,
}

impl Grid {
    pub fn period_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }

    /// Every beat from the start to `duration_ms`: (time ms, beat in bar 1..=4).
    pub fn beats(&self, duration_ms: f64) -> Vec<(u32, u16)> {
        if !(self.bpm.is_finite() && self.bpm > 0.0) {
            return Vec::new();
        }
        let p = self.period_ms();
        let mut k = (-self.anchor_ms / p).ceil() as i64;
        let mut out = Vec::new();
        loop {
            let t = self.anchor_ms + k as f64 * p;
            if t > duration_ms {
                break;
            }
            if t >= 0.0 {
                out.push((t.round() as u32, (k.rem_euclid(4) + 1) as u16));
            }
            k += 1;
        }
        out
    }

    /// The steady grid that best matches analysed beats (time ms, beat in
    /// bar): tempo by a line fit over all of them, anchored on the first
    /// downbeat.
    pub fn from_beats(beats: &[(u32, u16)]) -> Option<Grid> {
        if beats.len() < 2 {
            return None;
        }
        let n = beats.len() as f64;
        let (sk, st) = beats
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |a, (k, b)| (a.0 + k as f64, a.1 + b.0 as f64));
        let (mk, mt) = (sk / n, st / n);
        let (num, den) = beats.iter().enumerate().fold((0.0, 0.0), |a, (k, b)| {
            let dk = k as f64 - mk;
            (a.0 + dk * (b.0 as f64 - mt), a.1 + dk * dk)
        });
        if den <= 0.0 || num <= 0.0 {
            return None;
        }
        let period = num / den;
        let origin = mt - period * mk;
        // The first downbeat, else the bar one before the first beat.
        let (k, b) = beats
            .iter()
            .enumerate()
            .find(|(_, b)| b.1 == 1)
            .map(|(k, b)| (k as i64, b.1))
            .unwrap_or((0, beats[0].1));
        let anchor = origin + (k - (b as i64 - 1)) as f64 * period;
        Some(Grid {
            bpm: 60_000.0 / period,
            anchor_ms: anchor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beats_number_bars_from_the_anchor() {
        let g = Grid {
            bpm: 120.0,
            anchor_ms: 1250.0,
        };
        let b = g.beats(3000.0);
        // The two beats before the anchor close the bar before it.
        assert_eq!(&b[..4], &[(250, 3), (750, 4), (1250, 1), (1750, 2)]);
        assert_eq!(b.last(), Some(&(2750, 4)));
    }

    #[test]
    fn fits_analysed_beats() {
        let beats: Vec<(u32, u16)> = (0..300)
            .map(|k| ((180.0 + k as f64 * 468.75).round() as u32, (k % 4 + 3) as u16 % 4 + 1))
            .collect();
        let g = Grid::from_beats(&beats).unwrap();
        assert!((g.bpm - 128.0).abs() < 0.001, "{}", g.bpm);
        // Beat numbers start at 4: the first downbeat is the second beat.
        assert!((g.anchor_ms - 648.75).abs() < 0.6, "{}", g.anchor_ms);
        assert_eq!(g.beats(5000.0)[1], (649, 1));
    }
}
