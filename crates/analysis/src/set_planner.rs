//! Set Planner: put a pool of tracks into a playable order for a target
//! length and energy curve, and plan every transition (where to start the
//! next track, how long to blend, tempo and key advice), with rules that
//! depend on how experienced the DJ is.

use crate::smart_cue::Bands;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Beginner,
    Advanced,
    Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    /// Starts low, grows slowly; for opening a night.
    WarmUp,
    /// High throughout with a short build at the start.
    PeakTime,
    /// From high down to low.
    Closing,
    /// Low → peak around two thirds → down at the end.
    Journey,
}

impl Curve {
    /// Target energy 1..=10 at position 0..=1 of the set.
    pub fn at(self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Curve::WarmUp => 3.0 + 4.0 * x,
            Curve::PeakTime => 7.0 + 2.0 * (x / 0.2).min(1.0),
            Curve::Closing => 8.5 - 5.0 * x,
            Curve::Journey => {
                if x < 0.66 {
                    4.0 + 5.0 * (x / 0.66)
                } else {
                    9.0 - 4.0 * ((x - 0.66) / 0.34)
                }
            }
        }
    }
}

/// What the planner needs to know about a track.
#[derive(Debug, Clone)]
pub struct PlanTrack {
    pub id: i64,
    pub bpm: f64,
    /// Camelot number 1..=12 and letter (true = B / major), if known.
    pub camelot: Option<(u8, bool)>,
    /// 1..=10.
    pub energy: f64,
    /// Seconds.
    pub duration: f64,
    /// Where the next track can start being mixed in on top of this one (ms).
    pub mix_out_ms: u32,
    /// Where this track is started when mixing it in (ms).
    pub mix_in_ms: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Target length in minutes; 0 = use every track.
    pub minutes: f64,
    pub level: Level,
    pub curve: Curve,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub from: i64,
    pub to: i64,
    /// Camelot step: "same", "+1", "-1", "relative", "+2", "clash" …
    pub key_relation: &'static str,
    pub key_ok: bool,
    /// Percent the next track has to be pitched to match.
    pub pitch_percent: f64,
    pub tempo_ok: bool,
    /// Blend length in bars.
    pub bars: u32,
    /// Where in the outgoing track to start the incoming one (ms).
    pub start_at_ms: u32,
    /// Where in the incoming track to start it (ms).
    pub cue_in_ms: u32,
    pub energy_step: f64,
    /// Human advice for this transition.
    pub tips: Vec<String>,
    /// 0 good … higher is harder.
    pub cost: f64,
}

#[derive(Debug, Clone)]
pub struct SetPlan {
    pub order: Vec<i64>,
    pub left_out: Vec<i64>,
    pub transitions: Vec<Transition>,
    /// Start time of each track in the set (seconds), same order as `order`.
    pub starts: Vec<f64>,
    pub total_seconds: f64,
    /// Target curve sampled per track, for drawing.
    pub target: Vec<f64>,
}

/// Camelot from a key name: "8A", "08B", "Am", "F#m", "Db" …
pub fn camelot(key: &str) -> Option<(u8, bool)> {
    let k = key.trim();
    if let Some(letter) = k
        .chars()
        .last()
        .filter(|c| matches!(c, 'A' | 'B' | 'a' | 'b'))
        && let Ok(n) = k[..k.len() - 1].trim().parse::<u8>()
        && (1..=12).contains(&n)
    {
        return Some((n, letter.eq_ignore_ascii_case(&'b')));
    }
    let table: [(&str, u8, bool); 34] = [
        ("Abm", 1, false),
        ("G#m", 1, false),
        ("B", 1, true),
        ("Cb", 1, true),
        ("Ebm", 2, false),
        ("D#m", 2, false),
        ("F#", 2, true),
        ("Gb", 2, true),
        ("Bbm", 3, false),
        ("A#m", 3, false),
        ("Db", 3, true),
        ("C#", 3, true),
        ("Fm", 4, false),
        ("Ab", 4, true),
        ("G#", 4, true),
        ("Cm", 5, false),
        ("Eb", 5, true),
        ("D#", 5, true),
        ("Gm", 6, false),
        ("Bb", 6, true),
        ("A#", 6, true),
        ("Dm", 7, false),
        ("F", 7, true),
        ("Am", 8, false),
        ("C", 8, true),
        ("Em", 9, false),
        ("G", 9, true),
        ("Bm", 10, false),
        ("D", 10, true),
        ("F#m", 11, false),
        ("Gbm", 11, false),
        ("A", 11, true),
        ("C#m", 12, false),
        ("E", 12, true),
    ];
    table
        .iter()
        .find(|(name, _, _)| *name == k)
        .map(|&(_, n, b)| (n, b))
}

/// Harmonic relation and how hard it is (0 = perfect).
fn harmony(
    a: Option<(u8, bool)>,
    b: Option<(u8, bool)>,
    level: Level,
) -> (&'static str, f64, bool) {
    let (Some((na, la)), Some((nb, lb))) = (a, b) else {
        return ("?", 0.4, true);
    };
    let step = ((nb as i32 - na as i32) % 12 + 12) % 12; // 0..11, clockwise
    let (name, cost) = match (step, la == lb) {
        (0, true) => ("same", 0.0),
        (0, false) => ("relative", 0.1),
        (1, true) => ("+1", 0.1),
        (11, true) => ("-1", 0.1),
        (2, true) => ("+2", 0.45),
        (7, true) => ("+7", 0.5),
        (1, false) | (11, false) => ("diagonal", 0.35),
        _ => ("clash", 1.0),
    };
    let allowed = match level {
        Level::Beginner => cost <= 0.1,
        Level::Advanced => cost <= 0.5,
        Level::Pro => true,
    };
    (name, cost, allowed)
}

/// Pitch needed, allowing half/double tempo, in percent.
fn pitch(from_bpm: f64, to_bpm: f64) -> f64 {
    if from_bpm <= 0.0 || to_bpm <= 0.0 {
        return 0.0;
    }
    [to_bpm, to_bpm * 2.0, to_bpm / 2.0]
        .iter()
        .map(|b| (from_bpm / b - 1.0) * 100.0)
        .min_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
        .unwrap()
}

fn max_pitch(level: Level) -> f64 {
    match level {
        Level::Beginner => 3.0,
        Level::Advanced => 6.0,
        Level::Pro => 10.0,
    }
}

fn blend_bars(level: Level) -> u32 {
    match level {
        Level::Beginner => 32,
        Level::Advanced => 16,
        Level::Pro => 8,
    }
}

/// Seconds a track adds to the set: from its mix-in point to the next mix-out.
fn play_seconds(t: &PlanTrack) -> f64 {
    let a = t.mix_in_ms as f64 / 1000.0;
    let b = (t.mix_out_ms as f64 / 1000.0)
        .max(a + 30.0)
        .min(t.duration.max(a + 30.0));
    b - a
}

fn pair_cost(a: &PlanTrack, b: &PlanTrack, level: Level, target_energy: f64) -> f64 {
    let (_, h, allowed) = harmony(a.camelot, b.camelot, level);
    let p = pitch(b.bpm, a.bpm).abs();
    let tempo_pen = if p > max_pitch(level) {
        3.0 + p / 10.0
    } else {
        p / max_pitch(level) * 0.6
    };
    let key_pen = if allowed { h } else { 2.5 + h };
    let energy_pen = (b.energy - target_energy).abs() / 3.0;
    let jump = (b.energy - a.energy).abs();
    let jump_pen = if jump > 2.5 { (jump - 2.5) * 0.5 } else { 0.0 };
    key_pen + tempo_pen + energy_pen + jump_pen
}

/// Plan a set from a pool of tracks.
pub fn plan(pool: &[PlanTrack], params: &Params) -> SetPlan {
    let n = pool.len();
    if n == 0 {
        return SetPlan {
            order: vec![],
            left_out: vec![],
            transitions: vec![],
            starts: vec![],
            total_seconds: 0.0,
            target: vec![],
        };
    }
    let total_pool: f64 = pool.iter().map(play_seconds).sum();
    let target_secs = if params.minutes > 0.0 {
        (params.minutes * 60.0).min(total_pool)
    } else {
        total_pool
    };

    // Beam search over orders; position in the set is measured in time so the
    // energy curve is followed whatever the track lengths.
    #[derive(Clone)]
    struct State {
        seq: Vec<usize>,
        used: Vec<bool>,
        secs: f64,
        cost: f64,
    }
    let width = 256usize.min(n * 8).max(16);
    let mut beam: Vec<State> = (0..n)
        .map(|i| {
            let mut used = vec![false; n];
            used[i] = true;
            State {
                seq: vec![i],
                used,
                secs: play_seconds(&pool[i]),
                cost: (pool[i].energy - params.curve.at(0.0)).abs() / 2.0,
            }
        })
        .collect();
    beam.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());
    beam.truncate(width);
    let mut done: Vec<State> = Vec::new();
    while !beam.is_empty() {
        let mut next: Vec<State> = Vec::new();
        for s in &beam {
            if s.secs >= target_secs - 30.0 || s.seq.len() == n {
                done.push(s.clone());
                continue;
            }
            let last = &pool[*s.seq.last().unwrap()];
            let x = s.secs / target_secs.max(1.0);
            for (j, cand) in pool.iter().enumerate() {
                if s.used[j] {
                    continue;
                }
                let c = pair_cost(last, cand, params.level, params.curve.at(x));
                let mut ns = s.clone();
                ns.seq.push(j);
                ns.used[j] = true;
                ns.secs += play_seconds(&pool[j]);
                ns.cost += c;
                next.push(ns);
            }
        }
        // Compare partial sets by cost per transition.
        next.sort_by(|a, b| {
            (a.cost / a.seq.len() as f64)
                .partial_cmp(&(b.cost / b.seq.len() as f64))
                .unwrap()
        });
        next.truncate(width);
        beam = next;
    }
    // Prefer sets that reach the target length, then lowest mean cost.
    let best = done
        .into_iter()
        .min_by(|a, b| {
            let score = |s: &State| {
                let short = ((target_secs - s.secs) / 60.0).max(0.0);
                s.cost / s.seq.len().max(1) as f64 + short * 0.5
            };
            score(a).partial_cmp(&score(b)).unwrap()
        })
        .unwrap();

    build(pool, &best.seq, params)
}

/// Keep the DJ's own order: only time the set and plan every transition.
pub fn plan_in_order(pool: &[PlanTrack], params: &Params) -> SetPlan {
    let seq: Vec<usize> = (0..pool.len()).collect();
    build(pool, &seq, params)
}

/// Timeline, target curve and transitions for a given order of `pool`.
fn build(pool: &[PlanTrack], seq: &[usize], params: &Params) -> SetPlan {
    let order: Vec<i64> = seq.iter().map(|&i| pool[i].id).collect();
    let left_out: Vec<i64> = pool
        .iter()
        .filter(|t| !order.contains(&t.id))
        .map(|t| t.id)
        .collect();
    let total: f64 = seq.iter().map(|&i| play_seconds(&pool[i])).sum();
    let mut starts = Vec::new();
    let mut target = Vec::new();
    let mut t = 0.0;
    for &i in seq {
        starts.push(t);
        target.push(params.curve.at(t / total.max(1.0)));
        t += play_seconds(&pool[i]);
    }
    let transitions = seq
        .windows(2)
        .enumerate()
        .map(|(k, w)| transition(&pool[w[0]], &pool[w[1]], params.level, target[k + 1]))
        .collect();
    SetPlan {
        order,
        left_out,
        transitions,
        starts,
        total_seconds: total,
        target,
    }
}

fn camelot_name(c: Option<(u8, bool)>) -> String {
    c.map(|(n, b)| format!("{n}{}", if b { "B" } else { "A" }))
        .unwrap_or_else(|| "?".into())
}

fn transition(a: &PlanTrack, b: &PlanTrack, level: Level, target_energy: f64) -> Transition {
    let (relation, _, key_ok) = harmony(a.camelot, b.camelot, level);
    let p = pitch(a.bpm, b.bpm);
    let tempo_ok = p.abs() <= max_pitch(level);
    let bars = blend_bars(level);
    let mut tips = Vec::new();
    tips.push(format!(
        "Pusť ji na Mix out skladby A, blend {bars} taktů, basy přehoď v polovině (po {} taktech).",
        bars / 2
    ));
    if p.abs() >= 0.05 {
        tips.push(format!(
            "Tempo: {:.1} → {:.1} BPM, příchozí skladbu posuň o {:+.1} %.",
            b.bpm, a.bpm, p
        ));
    }
    match relation {
        "same" | "relative" | "+1" | "-1" => tips.push(format!(
            "Tónina {} → {}: sedí, můžeš míchat dlouho.",
            camelot_name(a.camelot),
            camelot_name(b.camelot)
        )),
        "+2" | "+7" => tips.push(format!(
            "Tónina {} → {}: energy boost, krátký přechod přes drop.",
            camelot_name(a.camelot),
            camelot_name(b.camelot)
        )),
        "?" => tips.push("Tónina neznámá, poslouchej, jestli se melodie nepere.".into()),
        _ => tips.push(format!(
            "Tónina {} → {} se tluče: krátký střih nebo echo out, melodie přes sebe nepouštěj.",
            camelot_name(a.camelot),
            camelot_name(b.camelot)
        )),
    }
    if !tempo_ok {
        tips.push(format!(
            "Velký skok tempa ({p:+.1} %): přejdi střihem nebo přes breakdown."
        ));
    }
    let step = b.energy - a.energy;
    if step >= 2.0 {
        tips.push("Velký skok energie nahoru: přechod na drop příchozí skladby.".into());
    } else if step <= -2.0 {
        tips.push("Energie jde dolů: nech dohrát outro, ať to nespadne náhle.".into());
    }
    Transition {
        from: a.id,
        to: b.id,
        key_relation: relation,
        key_ok,
        pitch_percent: p,
        tempo_ok,
        bars,
        start_at_ms: a.mix_out_ms,
        cue_in_ms: b.mix_in_ms,
        energy_step: step,
        tips,
        cost: pair_cost(a, b, level, target_energy),
    }
}

/// Energy 1..=10 from the 3-band waveform between mix-in and mix-out:
/// overall level, how much low end, how busy the top is, and tempo.
pub fn energy(columns: &[Bands], rate: f32, from_ms: u32, to_ms: u32, bpm: f64) -> f64 {
    let col = |ms: u32| ((ms as f32 / 1000.0) * rate) as usize;
    let (a, b) = (
        col(from_ms).min(columns.len()),
        col(to_ms).min(columns.len()),
    );
    let body = if b > a + 100 { &columns[a..b] } else { columns };
    if body.is_empty() {
        return 5.0;
    }
    let n = body.len() as f32;
    let total = body.iter().map(|c| c.low + c.mid + c.high).sum::<f32>() / n;
    let low = body.iter().map(|c| c.low).sum::<f32>() / n;
    let high = body.iter().map(|c| c.high).sum::<f32>() / n;
    // Scaled to what rekordbox's PWV7 holds across a real library: per-band
    // means of low 7–28, mid 36–60, high 28–51 (10th–max percentile).
    let loud = ((total - 70.0) / 60.0).clamp(0.0, 1.0) as f64;
    let bass = ((low - 5.0) / 23.0).clamp(0.0, 1.0) as f64;
    let bright = ((high - 25.0) / 25.0).clamp(0.0, 1.0) as f64;
    let tempo = ((bpm - 100.0) / 50.0).clamp(0.0, 1.0);
    (1.0 + 9.0 * (0.4 * loud + 0.25 * bass + 0.15 * bright + 0.2 * tempo)).clamp(1.0, 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: i64, bpm: f64, key: &str, energy: f64) -> PlanTrack {
        PlanTrack {
            id,
            bpm,
            camelot: camelot(key),
            energy,
            duration: 300.0,
            mix_in_ms: 0,
            mix_out_ms: 240_000,
        }
    }

    #[test]
    fn camelot_names() {
        assert_eq!(camelot("8A"), Some((8, false)));
        assert_eq!(camelot("12B"), Some((12, true)));
        assert_eq!(camelot("Am"), Some((8, false)));
        assert_eq!(camelot("F#m"), Some((11, false)));
        assert_eq!(camelot("Db"), Some((3, true)));
        assert_eq!(camelot(""), None);
    }

    #[test]
    fn beginner_order_is_harmonic_and_follows_the_curve() {
        let pool = vec![
            t(1, 124.0, "8A", 4.0),
            t(2, 126.0, "9A", 5.0),
            t(3, 126.0, "10A", 6.0),
            t(4, 128.0, "10B", 7.0),
            t(5, 128.0, "3A", 7.5), // clashes with everything else
            t(6, 127.0, "11B", 8.0),
        ];
        let p = plan(
            &pool,
            &Params {
                minutes: 0.0,
                level: Level::Beginner,
                curve: Curve::WarmUp,
            },
        );
        assert_eq!(p.order.len() + p.left_out.len(), 6);
        assert_eq!(p.order[0], 1, "warm-up starts low: {:?}", p.order);
        for w in p.transitions.iter() {
            if w.from != 5 && w.to != 5 {
                assert!(w.key_ok, "{:?}", p.order);
            }
        }
        let energies: Vec<f64> = p
            .order
            .iter()
            .map(|id| pool.iter().find(|x| x.id == *id).unwrap().energy)
            .collect();
        assert!(energies.first() < energies.last(), "{energies:?}");
    }

    #[test]
    fn target_length_limits_the_set() {
        let pool: Vec<PlanTrack> = (0..20)
            .map(|i| t(i, 126.0, "8A", 5.0 + (i % 3) as f64))
            .collect();
        let p = plan(
            &pool,
            &Params {
                minutes: 20.0,
                level: Level::Advanced,
                curve: Curve::PeakTime,
            },
        );
        assert!(
            p.total_seconds >= 19.0 * 60.0 && p.total_seconds <= 25.0 * 60.0,
            "{}",
            p.total_seconds
        );
        assert!(!p.left_out.is_empty());
    }

    #[test]
    fn own_order_is_kept() {
        let pool = vec![
            t(3, 128.0, "3A", 8.0),
            t(1, 124.0, "8A", 4.0),
            t(2, 126.0, "9A", 5.0),
        ];
        let p = plan_in_order(
            &pool,
            &Params {
                minutes: 10.0,
                level: Level::Beginner,
                curve: Curve::WarmUp,
            },
        );
        assert_eq!(p.order, vec![3, 1, 2]);
        assert_eq!(p.transitions.len(), 2);
        assert!(!p.transitions[0].key_ok, "3A → 8A clashes for a beginner");
    }

    #[test]
    fn pitch_allows_double_time() {
        assert!((pitch(87.0, 174.0)).abs() < 0.01);
        assert!((pitch(126.0, 128.0) + 1.5625).abs() < 0.01);
    }
}
