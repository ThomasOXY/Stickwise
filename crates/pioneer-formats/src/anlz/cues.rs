//! Cue lists: `PCOB` (all players) and `PCO2` (NXS2 and newer: colours,
//! comments, hot cues D–H).

use super::{Section, tag};
use crate::error::{Bytes, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueListKind {
    Memory,
    Hot,
}

impl CueListKind {
    fn from_u32(v: u32) -> Result<Self> {
        match v {
            0 => Ok(CueListKind::Memory),
            1 => Ok(CueListKind::Hot),
            _ => Err(Error::Invalid(format!("cue list type {v}"))),
        }
    }

    fn to_u32(self) -> u32 {
        match self {
            CueListKind::Memory => 0,
            CueListKind::Hot => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueType {
    Point,
    Loop,
    Other(u8),
}

impl CueType {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => CueType::Point,
            2 => CueType::Loop,
            v => CueType::Other(v),
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            CueType::Point => 1,
            CueType::Loop => 2,
            CueType::Other(v) => v,
        }
    }
}

/// Entry of a `PCOB` list (`PCPT`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// 0 = memory cue, 1..=8 = hot cue A..H.
    pub hot_cue: u32,
    /// 0 = disabled, 1 = enabled, 4 = active loop.
    pub status: u32,
    pub unknown: u32,
    pub order_first: u16,
    pub order_last: u16,
    pub cue_type: CueType,
    pub pad: [u8; 3],
    pub time_ms: u32,
    /// `0xffffffff` when not a loop.
    pub loop_time_ms: u32,
    pub tail: Vec<u8>,
}

/// `PCOB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueList {
    pub kind: CueListKind,
    pub unknown: u16,
    pub memory_count: u32,
    pub cues: Vec<Cue>,
}

const PCPT_HEADER: u32 = 0x1c;
const PCPT_FIXED: usize = 40;

impl CueList {
    pub fn parse(s: &Section) -> Result<Self> {
        let h = Bytes(&s.header);
        let kind = CueListKind::from_u32(h.u32be(0)?)?;
        let count = h.u16be(6)? as usize;
        let b = Bytes(&s.body);
        let mut cues = Vec::with_capacity(count);
        let mut at = 0;
        for _ in 0..count {
            if b.slice(at, 4)? != b"PCPT" {
                return Err(Error::Magic {
                    offset: at,
                    expected: "PCPT",
                });
            }
            let len = b.u32be(at + 8)? as usize;
            if len < PCPT_FIXED {
                return Err(Error::Invalid(format!("PCPT entry length {len}")));
            }
            cues.push(Cue {
                hot_cue: b.u32be(at + 12)?,
                status: b.u32be(at + 16)?,
                unknown: b.u32be(at + 20)?,
                order_first: b.u16be(at + 24)?,
                order_last: b.u16be(at + 26)?,
                cue_type: CueType::from_u8(b.u8(at + 28)?),
                pad: b.slice(at + 29, 3)?.try_into().unwrap(),
                time_ms: b.u32be(at + 32)?,
                loop_time_ms: b.u32be(at + 36)?,
                tail: b.slice(at + PCPT_FIXED, len - PCPT_FIXED)?.to_vec(),
            });
            at += len;
        }
        Ok(CueList {
            kind,
            unknown: h.u16be(4)?,
            memory_count: h.u32be(8)?,
            cues,
        })
    }

    pub fn to_section(&self) -> Section {
        let mut header = Vec::with_capacity(12);
        header.extend_from_slice(&self.kind.to_u32().to_be_bytes());
        header.extend_from_slice(&self.unknown.to_be_bytes());
        header.extend_from_slice(&(self.cues.len() as u16).to_be_bytes());
        header.extend_from_slice(&self.memory_count.to_be_bytes());
        let mut body = Vec::new();
        for c in &self.cues {
            body.extend_from_slice(b"PCPT");
            body.extend_from_slice(&PCPT_HEADER.to_be_bytes());
            body.extend_from_slice(&((PCPT_FIXED + c.tail.len()) as u32).to_be_bytes());
            body.extend_from_slice(&c.hot_cue.to_be_bytes());
            body.extend_from_slice(&c.status.to_be_bytes());
            body.extend_from_slice(&c.unknown.to_be_bytes());
            body.extend_from_slice(&c.order_first.to_be_bytes());
            body.extend_from_slice(&c.order_last.to_be_bytes());
            body.push(c.cue_type.to_u8());
            body.extend_from_slice(&c.pad);
            body.extend_from_slice(&c.time_ms.to_be_bytes());
            body.extend_from_slice(&c.loop_time_ms.to_be_bytes());
            body.extend_from_slice(&c.tail);
        }
        Section::new(tag::CUES, header, body)
    }
}

/// Entry of a `PCO2` list (`PCP2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtCue {
    /// 0 = memory cue, 1..=8 = hot cue A..H.
    pub hot_cue: u32,
    pub cue_type: CueType,
    pub pad1: [u8; 3],
    pub time_ms: u32,
    /// `0xffffffff` when not a loop.
    pub loop_time_ms: u32,
    /// Memory cue / loop colour (index into the rekordbox palette).
    pub color_id: u8,
    pub pad2: [u8; 7],
    /// Quantized loop length in beats as a fraction; 0/0 when not quantized.
    pub loop_numerator: u16,
    pub loop_denominator: u16,
    /// `None` when the entry has no comment field at all.
    pub comment: Option<String>,
    /// Hot cue colour: palette code and RGB, when present.
    pub color: Option<(u8, [u8; 3])>,
    pub tail: Vec<u8>,
}

/// `PCO2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtCueList {
    pub kind: CueListKind,
    pub unknown: u16,
    pub cues: Vec<ExtCue>,
}

const PCP2_HEADER: u32 = 0x10;
const PCP2_FIXED: usize = 40;

impl ExtCueList {
    pub fn parse(s: &Section) -> Result<Self> {
        let h = Bytes(&s.header);
        let kind = CueListKind::from_u32(h.u32be(0)?)?;
        let count = h.u16be(4)? as usize;
        let b = Bytes(&s.body);
        let mut cues = Vec::with_capacity(count);
        let mut at = 0;
        for _ in 0..count {
            if b.slice(at, 4)? != b"PCP2" {
                return Err(Error::Magic {
                    offset: at,
                    expected: "PCP2",
                });
            }
            let len = b.u32be(at + 8)? as usize;
            if len < PCP2_FIXED {
                return Err(Error::Invalid(format!("PCP2 entry length {len}")));
            }
            let entry = b.slice(at, len)?;
            let e = Bytes(entry);
            let mut p = PCP2_FIXED;
            let comment = if len >= p + 4 {
                let n = e.u32be(p)? as usize;
                let raw = e.slice(p + 4, n)?;
                p += 4 + n;
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .take_while(|&u| u != 0)
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else {
                None
            };
            let color = if len >= p + 4 {
                let c = (e.u8(p)?, [e.u8(p + 1)?, e.u8(p + 2)?, e.u8(p + 3)?]);
                p += 4;
                Some(c)
            } else {
                None
            };
            cues.push(ExtCue {
                hot_cue: e.u32be(12)?,
                cue_type: CueType::from_u8(e.u8(16)?),
                pad1: e.slice(17, 3)?.try_into().unwrap(),
                time_ms: e.u32be(20)?,
                loop_time_ms: e.u32be(24)?,
                color_id: e.u8(28)?,
                pad2: e.slice(29, 7)?.try_into().unwrap(),
                loop_numerator: e.u16be(36)?,
                loop_denominator: e.u16be(38)?,
                comment,
                color,
                tail: entry[p..].to_vec(),
            });
            at += len;
        }
        Ok(ExtCueList {
            kind,
            unknown: h.u16be(6)?,
            cues,
        })
    }

    pub fn to_section(&self) -> Section {
        let mut header = Vec::with_capacity(8);
        header.extend_from_slice(&self.kind.to_u32().to_be_bytes());
        header.extend_from_slice(&(self.cues.len() as u16).to_be_bytes());
        header.extend_from_slice(&self.unknown.to_be_bytes());
        let mut body = Vec::new();
        for c in &self.cues {
            let mut e = Vec::with_capacity(96);
            e.extend_from_slice(b"PCP2");
            e.extend_from_slice(&PCP2_HEADER.to_be_bytes());
            e.extend_from_slice(&0u32.to_be_bytes()); // patched below
            e.extend_from_slice(&c.hot_cue.to_be_bytes());
            e.push(c.cue_type.to_u8());
            e.extend_from_slice(&c.pad1);
            e.extend_from_slice(&c.time_ms.to_be_bytes());
            e.extend_from_slice(&c.loop_time_ms.to_be_bytes());
            e.push(c.color_id);
            e.extend_from_slice(&c.pad2);
            e.extend_from_slice(&c.loop_numerator.to_be_bytes());
            e.extend_from_slice(&c.loop_denominator.to_be_bytes());
            if let Some(comment) = &c.comment {
                let mut raw: Vec<u8> = comment.encode_utf16().flat_map(u16::to_be_bytes).collect();
                if !raw.is_empty() {
                    raw.extend_from_slice(&[0, 0]);
                }
                e.extend_from_slice(&(raw.len() as u32).to_be_bytes());
                e.extend_from_slice(&raw);
            }
            if let Some((code, rgb)) = c.color {
                e.push(code);
                e.extend_from_slice(&rgb);
            }
            e.extend_from_slice(&c.tail);
            let len = e.len() as u32;
            e[8..12].copy_from_slice(&len.to_be_bytes());
            body.extend_from_slice(&e);
        }
        Section::new(tag::CUES_2, header, body)
    }
}
