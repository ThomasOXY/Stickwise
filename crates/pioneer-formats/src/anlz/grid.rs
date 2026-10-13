use super::{Section, tag};
use crate::error::{Bytes, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// Position in the bar, 1 = downbeat.
    pub beat_number: u16,
    /// BPM × 100.
    pub tempo: u16,
    /// Milliseconds at normal pitch.
    pub time_ms: u32,
}

/// `PQTZ`: one entry per beat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatGrid {
    pub unknown1: u32,
    pub unknown2: u32,
    pub beats: Vec<Beat>,
}

impl BeatGrid {
    pub fn new(beats: Vec<Beat>) -> Self {
        BeatGrid {
            unknown1: 0,
            unknown2: 0x0008_0000,
            beats,
        }
    }

    pub fn parse(s: &Section) -> Result<Self> {
        let count = s.header_u32(2)? as usize;
        let b = Bytes(&s.body);
        if s.body.len() < count * 8 {
            return Err(Error::Invalid(format!(
                "PQTZ: {count} beats in {} bytes",
                s.body.len()
            )));
        }
        let beats = (0..count)
            .map(|i| {
                Ok(Beat {
                    beat_number: b.u16be(8 * i)?,
                    tempo: b.u16be(8 * i + 2)?,
                    time_ms: b.u32be(8 * i + 4)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(BeatGrid {
            unknown1: s.header_u32(0)?,
            unknown2: s.header_u32(1)?,
            beats,
        })
    }

    pub fn to_section(&self) -> Section {
        let mut header = Vec::with_capacity(12);
        for v in [self.unknown1, self.unknown2, self.beats.len() as u32] {
            header.extend_from_slice(&v.to_be_bytes());
        }
        let mut body = Vec::with_capacity(self.beats.len() * 8);
        for b in &self.beats {
            body.extend_from_slice(&b.beat_number.to_be_bytes());
            body.extend_from_slice(&b.tempo.to_be_bytes());
            body.extend_from_slice(&b.time_ms.to_be_bytes());
        }
        Section::new(tag::BEAT_GRID, header, body)
    }
}
