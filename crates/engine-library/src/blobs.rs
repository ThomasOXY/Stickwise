//! The performance-data blobs Engine keeps per track: `trackData`,
//! `beatData`, `quickCues`, `loops` and `overviewWaveFormData`.
//!
//! All but `loops` are zlib-compressed behind a 4-byte **big-endian**
//! uncompressed length (Qt's `qCompress` framing). Byte order inside is
//! mixed, and it has to stay that way:
//! * `trackData`, `quickCues`, `overviewWaveFormData`: big-endian;
//! * `beatData`: big-endian header and marker counts, little-endian markers;
//! * `loops`: not compressed, little-endian.
//!
//! Layouts follow rb2engine's measurements of Engine DJ 4.3 output (MIT) and
//! were checked against libraries Engine DJ wrote (see the tests).

use std::io::{Read, Write};

use crate::{Error, Result};

pub const SLOTS: usize = 8;

fn compress(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_be_bytes().to_vec();
    let mut z = flate2::write::ZlibEncoder::new(&mut out, flate2::Compression::default());
    z.write_all(payload).expect("writing to a Vec");
    z.finish().expect("writing to a Vec");
    out
}

/// The payload of a compressed blob (empty for an empty blob).
pub fn decompress(blob: &[u8]) -> Result<Vec<u8>> {
    if blob.is_empty() {
        return Ok(Vec::new());
    }
    if blob.len() < 4 {
        return Err(Error::Blob(
            "compressed blob shorter than its header".into(),
        ));
    }
    let len = u32::from_be_bytes(blob[..4].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(len);
    flate2::read::ZlibDecoder::new(&blob[4..])
        .read_to_end(&mut out)
        .map_err(|e| Error::Blob(format!("zlib: {e}")))?;
    if out.len() != len {
        return Err(Error::Blob(format!(
            "uncompressed length {} differs from header {len}",
            out.len()
        )));
    }
    Ok(out)
}

/// Bounds-checked reader.
struct Cur<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8]) -> Self {
        Cur { b, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .b
            .get(self.at..self.at + n)
            .ok_or_else(|| Error::Blob(format!("truncated at byte {}", self.at)))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn f64be(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.arr()?))
    }
    fn f64le(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.arr()?))
    }
    fn i64be(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.arr()?))
    }
    fn i64le(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    fn i32be(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.arr()?))
    }
    fn i32le(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn rest(&self) -> &'a [u8] {
        &self.b[self.at..]
    }
    fn label(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// Labels are length-prefixed with one byte.
fn put_label(out: &mut Vec<u8>, s: &str) {
    let mut end = s.len().min(255);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    out.push(end as u8);
    out.extend_from_slice(&s.as_bytes()[..end]);
}

/// Pad colour, alpha first. `(0, 0, 0, 0)` marks an empty slot.
pub type Argb = [u8; 4];

// ---- trackData ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrackData {
    pub sample_rate: f64,
    pub samples: i64,
    /// Engine key ordinal 0..=23, see [`crate::convert::key_ordinal`].
    pub key: i32,
    pub loudness: [f64; 3],
}

impl TrackData {
    pub fn encode(&self) -> Vec<u8> {
        let mut p = Vec::with_capacity(44);
        p.extend_from_slice(&self.sample_rate.to_be_bytes());
        p.extend_from_slice(&self.samples.to_be_bytes());
        p.extend_from_slice(&self.key.to_be_bytes());
        for l in self.loudness {
            p.extend_from_slice(&l.to_be_bytes());
        }
        compress(&p)
    }

    pub fn decode(blob: &[u8]) -> Result<Self> {
        let raw = decompress(blob)?;
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let mut c = Cur::new(&raw);
        Ok(TrackData {
            sample_rate: c.f64be()?,
            samples: c.i64be()?,
            key: c.i32be()?,
            loudness: [c.f64be()?, c.f64be()?, c.f64be()?],
        })
    }
}

// ---- beatData -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatMarker {
    pub sample_offset: f64,
    /// Beat index; the first marker sits at -4, downbeats on multiples of 4.
    pub beat_index: i64,
    /// Beats until the next marker (0 on the last one).
    pub beats_to_next: i32,
    pub unknown: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeatData {
    pub sample_rate: f64,
    pub samples: f64,
    pub is_set: bool,
    pub default_grid: Vec<BeatMarker>,
    pub adjusted_grid: Vec<BeatMarker>,
    /// Trailing bytes Engine writes (nine zeros); kept on re-encode.
    pub extra: Vec<u8>,
}

impl BeatData {
    pub fn encode(&self) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&self.sample_rate.to_be_bytes());
        p.extend_from_slice(&self.samples.to_be_bytes());
        p.push(self.is_set as u8);
        for grid in [&self.default_grid, &self.adjusted_grid] {
            p.extend_from_slice(&(grid.len() as i64).to_be_bytes());
            for m in grid.iter() {
                p.extend_from_slice(&m.sample_offset.to_le_bytes());
                p.extend_from_slice(&m.beat_index.to_le_bytes());
                p.extend_from_slice(&m.beats_to_next.to_le_bytes());
                p.extend_from_slice(&m.unknown.to_le_bytes());
            }
        }
        p.extend_from_slice(&self.extra);
        compress(&p)
    }

    pub fn decode(blob: &[u8]) -> Result<Self> {
        let raw = decompress(blob)?;
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let mut c = Cur::new(&raw);
        let sample_rate = c.f64be()?;
        let samples = c.f64be()?;
        let is_set = c.u8()? != 0;
        let mut grids = [Vec::new(), Vec::new()];
        for g in &mut grids {
            let n = c.i64be()?;
            if !(0..=100_000).contains(&n) {
                return Err(Error::Blob(format!("beat grid with {n} markers")));
            }
            for _ in 0..n {
                g.push(BeatMarker {
                    sample_offset: c.f64le()?,
                    beat_index: c.i64le()?,
                    beats_to_next: c.i32le()?,
                    unknown: c.i32le()?,
                });
            }
        }
        let [default_grid, adjusted_grid] = grids;
        Ok(BeatData {
            sample_rate,
            samples,
            is_set,
            default_grid,
            adjusted_grid,
            extra: c.rest().to_vec(),
        })
    }
}

// ---- quickCues ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct QuickCue {
    pub label: String,
    /// Samples; -1 for an empty pad.
    pub sample_offset: f64,
    pub color: Argb,
}

impl QuickCue {
    pub fn empty() -> Self {
        QuickCue {
            label: String::new(),
            sample_offset: -1.0,
            color: [0; 4],
        }
    }

    pub fn is_set(&self) -> bool {
        self.sample_offset >= 0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuickCues {
    /// Always eight pads, A..H.
    pub cues: Vec<QuickCue>,
    /// Where the track loads (samples).
    pub main_cue: f64,
    pub main_cue_adjusted: bool,
    pub default_main_cue: f64,
    pub extra: Vec<u8>,
}

impl Default for QuickCues {
    fn default() -> Self {
        QuickCues {
            cues: vec![QuickCue::empty(); SLOTS],
            main_cue: 0.0,
            main_cue_adjusted: false,
            default_main_cue: 0.0,
            extra: Vec::new(),
        }
    }
}

impl QuickCues {
    pub fn encode(&self) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(SLOTS as i64).to_be_bytes());
        for i in 0..SLOTS {
            let cue = self.cues.get(i).cloned().unwrap_or_else(QuickCue::empty);
            put_label(&mut p, &cue.label);
            p.extend_from_slice(&cue.sample_offset.to_be_bytes());
            p.extend_from_slice(&cue.color);
        }
        p.extend_from_slice(&self.main_cue.to_be_bytes());
        p.push(self.main_cue_adjusted as u8);
        p.extend_from_slice(&self.default_main_cue.to_be_bytes());
        p.extend_from_slice(&self.extra);
        compress(&p)
    }

    pub fn decode(blob: &[u8]) -> Result<Self> {
        let raw = decompress(blob)?;
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let mut c = Cur::new(&raw);
        let n = c.i64be()?;
        if !(0..=64).contains(&n) {
            return Err(Error::Blob(format!("{n} quick cues")));
        }
        let mut cues = Vec::new();
        for _ in 0..n {
            cues.push(QuickCue {
                label: c.label()?,
                sample_offset: c.f64be()?,
                color: c.arr()?,
            });
        }
        Ok(QuickCues {
            cues,
            main_cue: c.f64be()?,
            main_cue_adjusted: c.u8()? != 0,
            default_main_cue: c.f64be()?,
            extra: c.rest().to_vec(),
        })
    }
}

// ---- loops ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub label: String,
    pub start: f64,
    pub end: f64,
    pub start_set: bool,
    pub end_set: bool,
    pub color: Argb,
}

impl Loop {
    pub fn empty() -> Self {
        Loop {
            label: String::new(),
            start: -1.0,
            end: -1.0,
            start_set: false,
            end_set: false,
            color: [0; 4],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loops {
    pub loops: Vec<Loop>,
    pub extra: Vec<u8>,
}

impl Default for Loops {
    fn default() -> Self {
        Loops {
            loops: vec![Loop::empty(); SLOTS],
            extra: Vec::new(),
        }
    }
}

impl Loops {
    /// Not compressed.
    pub fn encode(&self) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(SLOTS as i64).to_le_bytes());
        for i in 0..SLOTS {
            let l = self.loops.get(i).cloned().unwrap_or_else(Loop::empty);
            put_label(&mut p, &l.label);
            p.extend_from_slice(&l.start.to_le_bytes());
            p.extend_from_slice(&l.end.to_le_bytes());
            p.push(l.start_set as u8);
            p.push(l.end_set as u8);
            p.extend_from_slice(&l.color);
        }
        p.extend_from_slice(&self.extra);
        p
    }

    pub fn decode(blob: &[u8]) -> Result<Self> {
        if blob.is_empty() {
            return Ok(Self::default());
        }
        let mut c = Cur::new(blob);
        let n = c.i64le()?;
        if !(0..=64).contains(&n) {
            return Err(Error::Blob(format!("{n} loops")));
        }
        let mut loops = Vec::new();
        for _ in 0..n {
            loops.push(Loop {
                label: c.label()?,
                start: c.f64le()?,
                end: c.f64le()?,
                start_set: c.u8()? != 0,
                end_set: c.u8()? != 0,
                color: c.arr()?,
            });
        }
        Ok(Loops {
            loops,
            extra: c.rest().to_vec(),
        })
    }
}

// ---- overviewWaveFormData -------------------------------------------------

/// The overview strip: Engine writes 1024 points of (low, mid, high).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    pub samples_per_point: f64,
    pub points: Vec<[u8; 3]>,
}

impl Overview {
    pub const POINTS: usize = 1024;

    pub fn encode(&self) -> Vec<u8> {
        let n = self.points.len() as i64;
        let mut p = Vec::with_capacity(24 + 3 * self.points.len() + 3);
        p.extend_from_slice(&n.to_be_bytes());
        p.extend_from_slice(&n.to_be_bytes());
        p.extend_from_slice(&self.samples_per_point.to_be_bytes());
        let mut max = [0u8; 3];
        for pt in &self.points {
            p.extend_from_slice(pt);
            for i in 0..3 {
                max[i] = max[i].max(pt[i]);
            }
        }
        p.extend_from_slice(&max);
        compress(&p)
    }

    pub fn decode(blob: &[u8]) -> Result<Self> {
        let raw = decompress(blob)?;
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let mut c = Cur::new(&raw);
        let n = c.i64be()?;
        let n2 = c.i64be()?;
        if n != n2 || !(0..=100_000).contains(&n) {
            return Err(Error::Blob(format!("overview with {n}/{n2} points")));
        }
        let samples_per_point = c.f64be()?;
        let points = (0..n).map(|_| c.arr()).collect::<Result<_>>()?;
        Ok(Overview {
            samples_per_point,
            points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips() {
        let t = TrackData {
            sample_rate: 44100.0,
            samples: 10_503_935,
            key: 22,
            loudness: [0.0; 3],
        };
        assert_eq!(TrackData::decode(&t.encode()).unwrap(), t);
        let mut q = QuickCues::default();
        q.cues[2] = QuickCue {
            label: "Drop".into(),
            sample_offset: 4_961_250.0,
            color: [255, 170, 85, 196],
        };
        assert_eq!(QuickCues::decode(&q.encode()).unwrap(), q);
        let mut l = Loops::default();
        l.loops[0] = Loop {
            label: "Loop".into(),
            start: 1.0,
            end: 2.0,
            start_set: true,
            end_set: true,
            color: [255, 1, 2, 3],
        };
        assert_eq!(l.encode().len(), 8 + 7 * 23 + 27);
        assert_eq!(Loops::decode(&l.encode()).unwrap(), l);
        assert_eq!(Loops::default().encode().len(), 192);
    }
}
