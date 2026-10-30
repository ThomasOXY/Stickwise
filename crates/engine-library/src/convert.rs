//! rekordbox data → Engine data: key, beat grid, cues, overview waveform.

use pioneer_formats::anlz::{Beat, CuePoint, Waveform, WaveformKind, hot_cue_color};

use crate::blobs::{Argb, BeatMarker, Loop, Loops, Overview, QuickCue, QuickCues, SLOTS};

/// Engine's key ordinal: 0 = C, 1 = Am, then up the circle of fifths in
/// major/minor pairs (G, Em, D, Bm, …). Accepts `Am`, `F#m`, `Dbm`, `C`,
/// `Abmaj`, `8A`, … ; `None` when the key is not recognised.
pub fn key_ordinal(name: &str) -> Option<i32> {
    let s: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    // Camelot: 8B = C, 8A = Am.
    let (num, letter) = s.split_at(s.len() - 1);
    if let Ok(n) = num.parse::<i32>()
        && (1..=12).contains(&n)
    {
        let minor = match letter {
            "A" | "a" => true,
            "B" | "b" => false,
            _ => return None,
        };
        return Some(2 * (n - 8).rem_euclid(12) + minor as i32);
    }
    let mut chars = s.chars();
    let mut pc: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut rest: &str = chars.as_str();
    if let Some(r) = rest.strip_prefix(['#', '♯']) {
        pc += 1;
        rest = r;
    } else if let Some(r) = rest.strip_prefix(['b', '♭']) {
        pc += 11;
        rest = r;
    }
    let minor = match rest.to_ascii_lowercase().as_str() {
        "" | "maj" | "major" => false,
        "m" | "min" | "minor" => true,
        _ => return None,
    };
    // Camelot number of the key (major: C = 8; minor: its relative major).
    let pc = if minor { pc + 3 } else { pc } % 12;
    let n = (7 * pc + 8) % 12;
    let n = if n == 0 { 12 } else { n };
    Some(2 * (n - 8).rem_euclid(12) + minor as i32)
}

/// rekordbox lists every beat; Engine wants sparse markers with the tempo
/// implied between neighbours, the first one at beat -4 and the last one
/// past the end of the track. Downbeats land on multiples of 4.
pub fn beat_grid(beats: &[Beat], sample_rate: f64, samples: i64) -> Vec<BeatMarker> {
    if beats.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    let at = |b: &Beat| b.time_ms as f64 * sample_rate / 1000.0;
    let base = (beats[0].beat_number.max(1) as i64 - 1).rem_euclid(4);
    // Keep the first beat, every tempo change and the last beat.
    let mut kept: Vec<(f64, i64)> = vec![(at(&beats[0]), base)];
    for i in 1..beats.len() {
        if beats[i].tempo != beats[i - 1].tempo || i == beats.len() - 1 {
            kept.push((at(&beats[i]), base + i as i64));
        }
    }
    if kept.len() == 1 {
        let bpm = beats[0].tempo as f64 / 100.0;
        let spb = if bpm > 0.0 {
            60.0 * sample_rate / bpm
        } else {
            sample_rate / 2.0
        };
        kept.push((kept[0].0 + spb, kept[0].1 + 1));
    }
    let spb = |a: (f64, i64), b: (f64, i64)| (b.0 - a.0) / (b.1 - a.1) as f64;
    // First marker moved back to beat -4.
    let first_spb = spb(kept[0], kept[1]);
    kept[0] = (kept[0].0 - (kept[0].1 + 4) as f64 * first_spb, -4);
    // Last marker moved to the first beat past the end.
    let n = kept.len();
    let last_spb = spb(kept[n - 2], kept[n - 1]);
    let end = samples.max(0) as f64;
    if kept[n - 1].0 <= end && last_spb > 0.0 {
        let k = (((end - kept[n - 1].0) / last_spb).ceil() as i64).max(1);
        kept[n - 1] = (kept[n - 1].0 + k as f64 * last_spb, kept[n - 1].1 + k);
    }
    kept.iter()
        .enumerate()
        .map(|(i, &(off, idx))| BeatMarker {
            sample_offset: off,
            beat_index: idx,
            beats_to_next: kept.get(i + 1).map_or(0, |next| (next.1 - idx) as i32),
            unknown: 0,
        })
        .collect()
}

/// rekordbox's memory cue colours (`colors` table rows 1..=8).
fn memory_rgb(code: u8) -> Option<[u8; 3]> {
    Some(match code {
        1 => [0xf8, 0x70, 0xc0], // pink
        2 => [0xe6, 0x28, 0x28], // red
        3 => [0xff, 0x8c, 0x00], // orange
        4 => [0xe6, 0xd2, 0x00], // yellow
        5 => [0x1e, 0xd2, 0x3c], // green
        6 => [0x00, 0xc8, 0xc8], // aqua
        7 => [0x30, 0x5a, 0xff], // blue
        8 => [0xa0, 0x4c, 0xff], // purple
        _ => return None,
    })
}

fn color(c: &CuePoint) -> Argb {
    let rgb = if c.slot > 0 {
        hot_cue_color(c.hot_color)
            .map(|(display, _)| display)
            .or(c.hot_led)
            .unwrap_or(if c.is_loop() {
                [0xff, 0x8c, 0x00]
            } else {
                [0x28, 0xe2, 0x14]
            })
    } else {
        memory_rgb(c.memory_color).unwrap_or([0xe6, 0x28, 0x28])
    };
    [255, rgb[0], rgb[1], rgb[2]]
}

/// What happened to a track's cues on the way to Engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CueMapping {
    pub quick_cues: QuickCues,
    pub loops: Loops,
    /// Cues that found no free pad or loop slot.
    pub dropped: usize,
}

/// Engine has eight pads and eight saved loops, and no memory cues:
/// * hot cues stay on their pad (A..H);
/// * memory cues fill the pads left free, in time order;
/// * hot loops, then memory loops, go to the loop slots;
/// * the track loads at the first memory cue (else at 0).
pub fn cues(cues: &[CuePoint], sample_rate: f64) -> CueMapping {
    let pos = |ms: u32| ms as f64 * sample_rate / 1000.0;
    let mut out = CueMapping::default();
    let label = |c: &CuePoint| c.comment.clone();

    for c in cues
        .iter()
        .filter(|c| !c.is_loop() && (1..=SLOTS as u8).contains(&c.slot))
    {
        let pad = &mut out.quick_cues.cues[c.slot as usize - 1];
        if pad.is_set() {
            out.dropped += 1;
            continue;
        }
        *pad = QuickCue {
            label: label(c),
            sample_offset: pos(c.time_ms),
            color: color(c),
        };
    }
    let mut memory: Vec<&CuePoint> = cues
        .iter()
        .filter(|c| !c.is_loop() && c.slot == 0)
        .collect();
    memory.sort_by_key(|c| c.time_ms);
    if let Some(first) = memory.first() {
        out.quick_cues.main_cue = pos(first.time_ms);
        out.quick_cues.default_main_cue = pos(first.time_ms);
    }
    let mut free = (0..SLOTS)
        .filter(|&i| !out.quick_cues.cues[i].is_set())
        .collect::<Vec<_>>()
        .into_iter();
    for c in memory {
        match free.next() {
            Some(i) => {
                out.quick_cues.cues[i] = QuickCue {
                    label: label(c),
                    sample_offset: pos(c.time_ms),
                    color: color(c),
                }
            }
            None => out.dropped += 1,
        }
    }

    let mut loops: Vec<&CuePoint> = cues.iter().filter(|c| c.is_loop()).collect();
    loops.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));
    for (i, c) in loops.into_iter().enumerate() {
        if i >= SLOTS {
            out.dropped += 1;
            continue;
        }
        out.loops.loops[i] = Loop {
            label: label(c),
            start: pos(c.time_ms),
            end: pos(c.loop_end_ms.unwrap_or(c.time_ms)),
            start_set: true,
            end_set: true,
            color: color(c),
        };
    }
    out
}

/// The overview strip from rekordbox's 3-band preview (`PWV6`) or its mono
/// preview (`PWAV`), resampled to 1024 points.
pub fn overview(w: &Waveform, samples: i64) -> Option<Overview> {
    let cols = w.columns();
    if cols == 0 || samples <= 0 {
        return None;
    }
    let col = |i: usize| -> [f64; 3] {
        let d = &w.data[i * w.kind.entry_len()..(i + 1) * w.kind.entry_len()];
        match w.kind {
            // mid, high, low
            WaveformKind::ThreeBandPreview => [d[2] as f64, d[0] as f64, d[1] as f64],
            _ => {
                let h = (d[0] & 31) as f64;
                [h, h * 0.6, h * 0.25]
            }
        }
    };
    let n = Overview::POINTS;
    let mut pts = vec![[0f64; 3]; n];
    for (j, p) in pts.iter_mut().enumerate() {
        let a = j * cols / n;
        let b = ((j + 1) * cols / n).max(a + 1).min(cols);
        for i in a..b {
            let c = col(i);
            for k in 0..3 {
                p[k] = p[k].max(c[k]);
            }
        }
    }
    let max = pts
        .iter()
        .flat_map(|p| p.iter().copied())
        .fold(0.0, f64::max);
    if max <= 0.0 {
        return None;
    }
    let scale = 255.0 / max;
    Some(Overview {
        samples_per_point: samples as f64 / n as f64,
        points: pts
            .iter()
            .map(|p| p.map(|v| (v * scale).round().clamp(0.0, 255.0) as u8))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key_ordinal("C"), Some(0));
        assert_eq!(key_ordinal("Am"), Some(1));
        assert_eq!(key_ordinal("G"), Some(2));
        assert_eq!(key_ordinal("F#m"), Some(7));
        assert_eq!(key_ordinal("Dbm"), Some(9));
        assert_eq!(key_ordinal("C#m"), Some(9));
        assert_eq!(key_ordinal("Abm"), Some(11));
        assert_eq!(key_ordinal("Dm"), Some(23));
        assert_eq!(key_ordinal("F"), Some(22));
        assert_eq!(key_ordinal("8B"), Some(0));
        assert_eq!(key_ordinal("8A"), Some(1));
        assert_eq!(key_ordinal("11A"), Some(7));
        assert_eq!(key_ordinal("7A"), Some(23));
        assert_eq!(key_ordinal("Bbm"), Some(15));
        assert_eq!(key_ordinal(""), None);
        assert_eq!(key_ordinal("H"), None);
    }

    #[test]
    fn constant_grid_becomes_two_markers() {
        // 128 BPM, 44.1 kHz: 20 671.875 samples per beat; starts on beat 1.
        let beats: Vec<Beat> = (0..100)
            .map(|i| Beat {
                beat_number: (i % 4 + 1) as u16,
                tempo: 12800,
                time_ms: (100.0 + i as f64 * 468.75).round() as u32,
            })
            .collect();
        let m = beat_grid(&beats, 44100.0, 44100 * 60);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].beat_index, -4);
        assert_eq!(m[0].beats_to_next, (m[1].beat_index + 4) as i32);
        assert!(m[1].sample_offset > 44100.0 * 60.0);
        let bpm = 44100.0 * 60.0 * (m[1].beat_index - m[0].beat_index) as f64
            / (m[1].sample_offset - m[0].sample_offset);
        assert!((bpm - 128.0).abs() < 0.01, "{bpm}");
        // Beat 0 (a downbeat) sits where rekordbox put the first beat.
        let spb = (m[1].sample_offset - m[0].sample_offset) / (m[1].beat_index + 4) as f64;
        assert!((m[0].sample_offset + 4.0 * spb - 4410.0).abs() < 50.0);
    }

    #[test]
    fn grid_starting_mid_bar_keeps_downbeats() {
        // First beat is beat 3 of its bar: its index must be 2 (mod 4).
        let beats: Vec<Beat> = (0..8)
            .map(|i| Beat {
                beat_number: ((i + 2) % 4 + 1) as u16,
                tempo: 12000,
                time_ms: 1000 + i * 500,
            })
            .collect();
        let m = beat_grid(&beats, 48000.0, 48000 * 10);
        let spb = 24000.0;
        let first_beat_index = -4 + ((48000.0 - m[0].sample_offset) / spb).round() as i64;
        assert_eq!(first_beat_index.rem_euclid(4), 2);
    }

    #[test]
    fn cue_policy() {
        let mut hot = CuePoint::hot(3, 1000);
        hot.comment = "Drop".into();
        let mem = CuePoint::memory(500);
        let mut lp = CuePoint::memory(2000);
        lp.loop_end_ms = Some(4000);
        let m = cues(&[hot, mem, lp], 1000.0);
        assert_eq!(m.quick_cues.cues[2].label, "Drop");
        assert_eq!(m.quick_cues.cues[2].sample_offset, 1000.0);
        // The memory cue takes pad A and is where the track loads.
        assert_eq!(m.quick_cues.cues[0].sample_offset, 500.0);
        assert_eq!(m.quick_cues.main_cue, 500.0);
        assert_eq!(
            (m.loops.loops[0].start, m.loops.loops[0].end),
            (2000.0, 4000.0)
        );
        assert_eq!(m.dropped, 0);
    }
}
