//! Splitting the signal into low / mid / high bands and measuring each in
//! short frames: the raw material for waveforms and beat detection.

use std::f32::consts::PI;

/// A second-order IIR filter (RBJ audio EQ cookbook).
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn new(b: [f32; 3], a: [f32; 3]) -> Self {
        Biquad {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn lowpass(rate: u32, f: f32) -> Self {
        let w = 2.0 * PI * f / rate as f32;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        Self::new(
            [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    fn highpass(rate: u32, f: f32) -> Self {
        let w = 2.0 * PI * f / rate as f32;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        Self::new(
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

fn chain(f: &mut [Biquad; 2], x: f32) -> f32 {
    let y = f[0].run(x);
    f[1].run(y)
}

/// Crossovers between the bands, Hz.
pub const LOW_MID: f32 = 200.0;
pub const MID_HIGH: f32 = 2500.0;

/// Per frame: peak of the whole signal and RMS of each band.
#[derive(Debug, Clone, Default)]
pub struct Frames {
    /// Frames per second.
    pub fps: f64,
    pub peak: Vec<f32>,
    pub low: Vec<f32>,
    pub mid: Vec<f32>,
    pub high: Vec<f32>,
}

/// Measure `samples` in frames of `rate / fps` samples.
pub fn frames(samples: &[f32], rate: u32, fps: f64) -> Frames {
    // Two cascaded filters per edge: 24 dB/oct, bands that do not bleed much.
    let mut lp = [Biquad::lowpass(rate, LOW_MID); 2];
    let mut mid_hp = [Biquad::highpass(rate, LOW_MID); 2];
    let mut mid_lp = [Biquad::lowpass(rate, MID_HIGH); 2];
    let mut hp = [Biquad::highpass(rate, MID_HIGH); 2];
    let hop = rate as f64 / fps;
    let n = (samples.len() as f64 / hop).ceil() as usize;
    let mut out = Frames {
        fps,
        peak: Vec::with_capacity(n),
        low: Vec::with_capacity(n),
        mid: Vec::with_capacity(n),
        high: Vec::with_capacity(n),
    };
    let mut start = 0usize;
    for i in 0..n {
        let end = (((i + 1) as f64 * hop).round() as usize).min(samples.len());
        let (mut pk, mut l, mut m, mut h) = (0f32, 0f32, 0f32, 0f32);
        for &x in &samples[start..end] {
            pk = pk.max(x.abs());
            let a = chain(&mut lp, x);
            let b = chain(&mut mid_lp, chain(&mut mid_hp, x));
            let c = chain(&mut hp, x);
            l += a * a;
            m += b * b;
            h += c * c;
        }
        let len = (end - start).max(1) as f32;
        out.peak.push(pk);
        out.low.push((l / len).sqrt());
        out.mid.push((m / len).sqrt());
        out.high.push((h / len).sqrt());
        start = end;
    }
    out
}
