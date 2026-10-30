//! Waveforms in rekordbox's encodings, from band measurements at 150
//! frames per second. Scales are calibrated on rekordbox's own analyses:
//! heights are absolute (a full-scale peak reaches the top), not
//! normalised per track, so quiet tracks look quiet as on the players.

use crate::bands::Frames;

/// Columns per second of the scrolling waveforms.
pub const RATE: f64 = 150.0;

/// Band RMS → rekordbox's 0..=127 band level: a median fit on rekordbox
/// files, lowered a little so loud parts do not flatten against the top.
const LOW: f32 = 75.0;
const MID: f32 = 300.0;
const HIGH: f32 = 380.0;
/// rekordbox's RGB waveform shows mids in green more sparingly.
const GREEN: f32 = 0.55;
/// Whole-signal peak → 0..=31 height.
const HEIGHT: f32 = 29.0;

/// One column, in rekordbox units.
#[derive(Debug, Clone, Copy, Default)]
struct Col {
    /// 0..=31.
    height: f32,
    low: f32,
    mid: f32,
    high: f32,
}

impl Col {
    /// 0..=7: how much of the sound is mids and highs (bright, white tops).
    fn whiteness(&self) -> u8 {
        let sum = self.low + self.mid + self.high;
        if sum <= 0.0 {
            return 0;
        }
        (7.0 * (self.mid + self.high) / sum).round().clamp(0.0, 7.0) as u8
    }

    /// 3-bit red / green / blue for lows / mids / highs, brightest at 7.
    fn rgb(&self) -> (u16, u16, u16) {
        let g = self.mid * GREEN;
        let m = self.low.max(g).max(self.high);
        if m <= 0.0 {
            return (0, 0, 0);
        }
        let c = |v: f32| (7.0 * v / m).round().clamp(0.0, 7.0) as u16;
        (c(self.low), c(g), c(self.high))
    }

    fn h31(&self) -> u8 {
        self.height.round().clamp(0.0, 31.0) as u8
    }
}

fn level(v: f32, scale: f32) -> f32 {
    (v * scale).clamp(0.0, 127.0)
}

/// Columns at [`RATE`] from frames at a multiple of it.
fn columns(f: &Frames) -> Vec<Col> {
    let k = (f.fps / RATE).round().max(1.0) as usize;
    (0..f.peak.len().div_ceil(k))
        .map(|c| {
            let r = c * k..((c + 1) * k).min(f.peak.len());
            let rms = |v: &[f32]| (v[r.clone()].iter().map(|x| x * x).sum::<f32>() / r.len() as f32).sqrt();
            Col {
                height: f.peak[r.clone()].iter().fold(0f32, |a, &b| a.max(b)) * HEIGHT,
                low: level(rms(&f.low), LOW),
                mid: level(rms(&f.mid), MID),
                high: level(rms(&f.high), HIGH),
            }
        })
        .collect()
}

/// `n` columns over the whole track: loudest of each stretch.
fn squeeze(cols: &[Col], n: usize) -> Vec<Col> {
    (0..n)
        .map(|i| {
            let a = i * cols.len() / n;
            let b = ((i + 1) * cols.len() / n).max(a + 1).min(cols.len());
            cols.get(a..b).unwrap_or_default().iter().fold(Col::default(), |m, c| Col {
                height: m.height.max(c.height),
                low: m.low.max(c.low),
                mid: m.mid.max(c.mid),
                high: m.high.max(c.high),
            })
        })
        .collect()
}

/// Every waveform rekordbox writes (but the 6-byte colour preview `PWV4`,
/// whose encoding is not known well enough).
pub struct Waveforms {
    /// `PWAV`: 400 columns, height 0..=31 | whiteness << 5.
    pub preview: Vec<u8>,
    /// `PWV2`: 100 columns, height 0..=15.
    pub tiny: Vec<u8>,
    /// `PWV3`: 150/s, as `PWAV`.
    pub scroll: Vec<u8>,
    /// `PWV5`: 150/s, 2 bytes: r g b (3 bits each) | height (5 bits) << 2.
    pub color_scroll: Vec<u8>,
    /// `PWV6`: 1200 columns, 3 bytes: mid, high, low (0..=127).
    pub three_band_preview: Vec<u8>,
    /// `PWV7`: 150/s, as `PWV6`.
    pub three_band_scroll: Vec<u8>,
}

fn mono(c: &Col) -> u8 {
    c.h31() | (c.whiteness() << 5)
}

fn three(c: &Col) -> [u8; 3] {
    [c.mid as u8, c.high as u8, c.low as u8]
}

pub fn build(f: &Frames) -> Waveforms {
    let cols = columns(f);
    Waveforms {
        preview: squeeze(&cols, 400).iter().map(mono).collect(),
        tiny: squeeze(&cols, 100)
            .iter()
            .map(|c| (c.height * 15.0 / 31.0).round().clamp(0.0, 15.0) as u8)
            .collect(),
        scroll: cols.iter().map(mono).collect(),
        color_scroll: cols
            .iter()
            .flat_map(|c| {
                let (r, g, b) = c.rgb();
                ((r << 13) | (g << 10) | (b << 7) | ((c.h31() as u16) << 2)).to_be_bytes()
            })
            .collect(),
        three_band_preview: squeeze(&cols, 1200).iter().flat_map(three).collect(),
        three_band_scroll: cols.iter().flat_map(three).collect(),
    }
}
