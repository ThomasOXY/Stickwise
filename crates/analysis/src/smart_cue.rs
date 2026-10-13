//! Smart Cue: find the structural points DJs put cues on — where the mix
//! can start, drops, breakdowns, build-ups, outro and where to mix out.
//!
//! Dance music is built from 4- and 8-bar phrases and its sections are told
//! apart mostly by the low end: the kick and bass are in for a drop and out
//! for a breakdown. So the track is cut into bars using the beat grid, each
//! bar gets its average band energy from the waveform, bars are grouped into
//! 4-bar blocks aligned to the 8-bar phrase grid, and every change between
//! "bass in" and "bass out" becomes a candidate point.

/// What a suggested cue marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// First downbeat: where to start the mix.
    MixIn,
    /// Bass and kick come (back) in.
    Drop,
    /// Bass and kick drop out.
    Break,
    /// Energy rises before a drop.
    Build,
    /// A new section with more energy (vocal, lead, percussion comes in).
    Up,
    /// A new section with less energy.
    Down,
    /// Last section winds down.
    Outro,
    /// Last full phrase point before the music ends.
    MixOut,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::MixIn => "Mix in",
            Kind::Drop => "Drop",
            Kind::Break => "Break",
            Kind::Build => "Build",
            Kind::Up => "Up",
            Kind::Down => "Down",
            Kind::Outro => "Outro",
            Kind::MixOut => "Mix out",
        }
    }

    /// rekordbox hot cue colour code used for this kind.
    pub fn color_code(self) -> u8 {
        match self {
            Kind::MixIn => 0x16,  // green
            Kind::Drop => 0x2a,   // red
            Kind::Break => 0x01,  // blue
            Kind::Build => 0x22,  // orange
            Kind::Up => 0x1a,     // yellow-green
            Kind::Down => 0x09,   // cyan
            Kind::Outro => 0x3c,  // purple
            Kind::MixOut => 0x2d, // pink
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub kind: Kind,
    pub time_ms: u32,
    /// Bar number from the first downbeat, starting at 1.
    pub bar: usize,
    /// e.g. "Drop 2".
    pub label: String,
    /// 0..1, how clear the change in the music is.
    pub confidence: f32,
}

/// One waveform column: low, mid and high band energy.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bands {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

pub struct Input<'a> {
    /// Beat grid: (time in ms, position in the bar, 1 = downbeat).
    pub beats: &'a [(u32, u16)],
    /// Waveform columns.
    pub columns: &'a [Bands],
    /// Columns per second (150 for rekordbox scroll waveforms).
    pub rate: f32,
    pub bpm: f32,
}

/// Decode rekordbox 3-band scroll data (PWV7: mid, high, low per column).
pub fn bands_from_three_band(data: &[u8]) -> Vec<Bands> {
    data.chunks_exact(3)
        .map(|c| Bands {
            low: c[2] as f32,
            mid: c[0] as f32,
            high: c[1] as f32,
        })
        .collect()
}

/// Decode a colour scroll waveform (PWV5) into rough bands: red ≈ low,
/// green ≈ mid, blue ≈ high, scaled by height.
pub fn bands_from_color(data: &[u8]) -> Vec<Bands> {
    data.chunks_exact(2)
        .map(|c| {
            let w = u16::from_be_bytes([c[0], c[1]]);
            let h = ((w >> 2) & 31) as f32;
            Bands {
                low: h * ((w >> 13) & 7) as f32,
                mid: h * ((w >> 10) & 7) as f32,
                high: h * ((w >> 7) & 7) as f32,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Bar {
    start_ms: u32,
    low: f32,
    mid: f32,
    high: f32,
}

fn bars(input: &Input) -> Vec<Bar> {
    let mut starts: Vec<u32> = input
        .beats
        .iter()
        .filter(|b| b.1 == 1)
        .map(|b| b.0)
        .collect();
    if starts.len() < 4 {
        // No usable bar information: every 4th beat, or from the tempo.
        starts = if input.beats.len() >= 16 {
            input.beats.iter().step_by(4).map(|b| b.0).collect()
        } else {
            let bar_ms = 4.0 * 60000.0 / input.bpm.max(1.0);
            let len_ms = input.columns.len() as f32 / input.rate * 1000.0;
            (0..(len_ms / bar_ms) as usize)
                .map(|i| (i as f32 * bar_ms) as u32)
                .collect()
        };
    }
    let col = |ms: u32| ((ms as f32 / 1000.0) * input.rate) as usize;
    let mut out = Vec::with_capacity(starts.len());
    for (i, &s) in starts.iter().enumerate() {
        let end = starts
            .get(i + 1)
            .copied()
            .unwrap_or(s + starts.get(1).map_or(2000, |b| b - starts[0]));
        let (a, b) = (col(s), col(end).min(input.columns.len()));
        if a >= b {
            continue;
        }
        let n = (b - a) as f32;
        let (mut low, mut mid, mut high) = (0.0, 0.0, 0.0);
        for c in &input.columns[a..b] {
            low += c.low;
            mid += c.mid;
            high += c.high;
        }
        out.push(Bar {
            start_ms: s,
            low: low / n,
            mid: mid / n,
            high: high / n,
        });
    }
    out
}

fn percentile(values: &mut [f32], p: f32) -> f32 {
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[((values.len() - 1) as f32 * p) as usize].max(1e-6)
}

const BLOCK: usize = 4;
const PHRASE: usize = 8;

/// Find the structural points of a track.
pub fn analyze(input: &Input) -> Vec<Suggestion> {
    let bars = bars(input);
    if bars.len() < 16 {
        return bars
            .first()
            .map(|b| {
                vec![Suggestion {
                    kind: Kind::MixIn,
                    time_ms: b.start_ms,
                    bar: 1,
                    label: "Mix in".into(),
                    confidence: 1.0,
                }]
            })
            .unwrap_or_default();
    }
    let norm = |f: fn(&Bar) -> f32| {
        let r = percentile(&mut bars.iter().map(f).collect::<Vec<_>>(), 0.9);
        bars.iter().map(|b| f(b) / r).collect::<Vec<f32>>()
    };
    let low = norm(|b| b.low);
    let mid = norm(|b| b.mid);
    let high = norm(|b| b.high);
    let total: Vec<f32> = (0..bars.len())
        .map(|i| (low[i] + mid[i] + high[i]) / 3.0)
        .collect();
    let n = bars.len();
    let mean = |v: &[f32], a: usize, b: usize| {
        let (a, b) = (a.min(n), b.min(n));
        if b <= a {
            0.0
        } else {
            v[a..b].iter().sum::<f32>() / (b - a) as f32
        }
    };

    // How much the music changes at bar `b`: band differences of the 4 bars
    // after against the 4 before, the low end weighted most.
    let novelty = |b: usize| -> (f32, f32, f32) {
        let w = BLOCK;
        let d = |v: &[f32]| mean(v, b, b + w) - mean(v, b.saturating_sub(w), b);
        let (dl, dm, dh) = (d(&low), d(&mid), d(&high));
        (
            dl.abs() + 0.6 * dm.abs() + 0.4 * dh.abs(),
            dl,
            (dl + dm + dh) / 3.0,
        )
    };

    // Align to the 8-bar phrase grid where changes are strongest.
    let offset = (0..PHRASE)
        .max_by(|&x, &y| {
            let score = |o: usize| {
                let mut s: f32 = (o..n)
                    .step_by(PHRASE)
                    .filter(|&b| b > 0)
                    .map(|b| novelty(b).0)
                    .sum();
                if o == 0 {
                    s *= 1.05;
                }
                s
            };
            score(x).partial_cmp(&score(y)).unwrap()
        })
        .unwrap_or(0);
    let on_phrase = |b: usize| b >= offset && (b - offset).is_multiple_of(PHRASE);

    // Candidates every 4 bars on that grid; phrase starts count a bit more.
    let first = offset % BLOCK;
    let mut cands: Vec<(usize, f32, f32, f32)> = (first.max(BLOCK.min(first + BLOCK))
        ..n.saturating_sub(2))
        .step_by(BLOCK)
        .filter(|&b| b >= 2)
        .map(|b| {
            let (nov, dl, dt) = novelty(b);
            (b, if on_phrase(b) { nov * 1.2 } else { nov }, dl, dt)
        })
        .collect();
    if first > 0 {
        let (nov, dl, dt) = novelty(first);
        cands.insert(0, (first, nov, dl, dt));
    }
    let mut sorted: Vec<f32> = cands.iter().map(|c| c.1).collect();
    let threshold = (0.5 * percentile(&mut sorted, 0.9)).max(0.16);

    // Keep the strongest change within each 8-bar neighbourhood.
    let mut kept: Vec<(usize, f32, f32, f32)> = Vec::new();
    for c in cands.iter().filter(|c| c.1 >= threshold) {
        let rivals = cands
            .iter()
            .filter(|o| o.0 != c.0 && o.0.abs_diff(c.0) < PHRASE && o.1 > c.1)
            .count();
        if rivals == 0 {
            kept.push(*c);
        }
    }

    let end_bar = {
        let mut e = n;
        while e > 1 && total[e - 1] < 0.12 {
            e -= 1;
        }
        e
    };
    let outro_from = (end_bar as f32 * 0.72) as usize;
    let bass_on = |a: usize, b: usize| mean(&low, a, b) > 0.55;

    let mut out = vec![Suggestion {
        kind: Kind::MixIn,
        time_ms: bars[0].start_ms,
        bar: 1,
        label: "Mix in".into(),
        confidence: 1.0,
    }];
    let (mut drops, mut breaks) = (0, 0);
    for &(b, nov, _dl, dt) in &kept {
        if b >= end_bar {
            continue;
        }
        let before = bass_on(b.saturating_sub(BLOCK), b);
        let after = bass_on(b, b + BLOCK);
        let confidence = (nov / (2.0 * threshold)).min(1.0);
        let (kind, label) = match (before, after) {
            (false, true) => {
                drops += 1;
                (Kind::Drop, format!("Drop {drops}"))
            }
            (true, false) if b >= outro_from && !bass_on(b, end_bar) => {
                (Kind::Outro, "Outro".into())
            }
            // A real breakdown keeps the low end out for a whole phrase.
            (true, false) if mean(&low, b, b + PHRASE) <= 0.45 => {
                breaks += 1;
                (Kind::Break, format!("Break {breaks}"))
            }
            _ if dt > 0.0 => (Kind::Up, "Up".into()),
            _ => (Kind::Down, "Down".into()),
        };
        out.push(Suggestion {
            kind,
            time_ms: bars[b].start_ms,
            bar: b + 1,
            label,
            confidence,
        });
    }

    // Build-ups: the phrase before a drop when it climbs without the low end.
    let drops_at: Vec<(usize, String)> = out
        .iter()
        .filter(|s| s.kind == Kind::Drop)
        .map(|s| (s.bar - 1, s.label.replace("Drop", "Build")))
        .collect();
    for (d, label) in drops_at {
        if d < PHRASE {
            continue;
        }
        let b = d - PHRASE;
        let rise = mean(&total, b + BLOCK, d) - mean(&total, b, b + BLOCK);
        if rise > 0.05 && !bass_on(b, d) && !out.iter().any(|s| s.bar.abs_diff(b + 1) < BLOCK) {
            out.push(Suggestion {
                kind: Kind::Build,
                time_ms: bars[b].start_ms,
                bar: b + 1,
                label,
                confidence: (rise / 0.2).clamp(0.3, 1.0),
            });
        }
    }

    // Mix out: 32 bars before the music ends, on the phrase grid.
    if end_bar > 48 && !out.iter().any(|s| s.kind == Kind::Outro) {
        let mut m = end_bar - 32;
        while m > 0 && !on_phrase(m) {
            m -= 1;
        }
        if m > 0 && !out.iter().any(|s| s.bar.abs_diff(m + 1) < PHRASE) {
            out.push(Suggestion {
                kind: Kind::MixOut,
                time_ms: bars[m].start_ms,
                bar: m + 1,
                label: "Mix out".into(),
                confidence: 0.5,
            });
        }
    }
    out.sort_by_key(|s| s.time_ms);
    out
}

/// Pick up to 8 suggestions for hot cues A–H, keeping time order and
/// dropping the least important ones first.
pub fn hot_cue_plan(s: &[Suggestion]) -> Vec<Suggestion> {
    let mut picked: Vec<Suggestion> = s.to_vec();
    let rank = |k: Kind| match k {
        Kind::MixIn | Kind::Drop => 0,
        Kind::Break | Kind::MixOut => 1,
        Kind::Outro | Kind::Up => 2,
        Kind::Build | Kind::Down => 3,
    };
    while picked.len() > 8 {
        let (i, _) = picked
            .iter()
            .enumerate()
            .max_by(|a, b| {
                (rank(a.1.kind), -(a.1.confidence * 100.0) as i32)
                    .cmp(&(rank(b.1.kind), -(b.1.confidence * 100.0) as i32))
            })
            .unwrap();
        picked.remove(i);
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 128 BPM, 4/4, sections given as (bars, bass on?).
    fn track(sections: &[(usize, bool)]) -> (Vec<(u32, u16)>, Vec<Bands>) {
        let beat_ms = 60000.0 / 128.0;
        let total_bars: usize = sections.iter().map(|s| s.0).sum();
        let beats: Vec<(u32, u16)> = (0..total_bars * 4)
            .map(|i| ((i as f32 * beat_ms) as u32, (i % 4 + 1) as u16))
            .collect();
        let mut cols = Vec::new();
        for &(n, on) in sections {
            let len = (n as f32 * 4.0 * beat_ms / 1000.0 * 150.0) as usize;
            for _ in 0..len {
                cols.push(Bands {
                    low: if on { 200.0 } else { 15.0 },
                    mid: 90.0,
                    high: 60.0,
                });
            }
        }
        (beats, cols)
    }

    #[test]
    fn finds_drops_breaks_and_outro() {
        let (beats, cols) = track(&[
            (16, false),
            (32, true),
            (16, false),
            (32, true),
            (16, false),
        ]);
        let s = analyze(&Input {
            beats: &beats,
            columns: &cols,
            rate: 150.0,
            bpm: 128.0,
        });
        let kinds: Vec<(Kind, usize)> = s.iter().map(|x| (x.kind, x.bar)).collect();
        assert!(kinds.contains(&(Kind::MixIn, 1)), "{kinds:?}");
        assert!(kinds.contains(&(Kind::Drop, 17)), "{kinds:?}");
        assert!(kinds.contains(&(Kind::Break, 49)), "{kinds:?}");
        assert!(kinds.contains(&(Kind::Drop, 65)), "{kinds:?}");
        assert!(kinds.contains(&(Kind::Outro, 97)), "{kinds:?}");
        let plan = hot_cue_plan(&s);
        assert!(plan.len() <= 8);
    }

    #[test]
    fn short_blips_are_ignored() {
        // A 4-bar bass dropout inside a drop is not a breakdown.
        let (beats, cols) = track(&[(16, false), (24, true), (4, false), (36, true), (16, false)]);
        let s = analyze(&Input {
            beats: &beats,
            columns: &cols,
            rate: 150.0,
            bpm: 128.0,
        });
        assert_eq!(
            s.iter().filter(|x| x.kind == Kind::Break).count(),
            0,
            "{s:?}"
        );
    }
}
