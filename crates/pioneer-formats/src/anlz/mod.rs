//! Analysis files `ANLZ0000.DAT`, `.EXT` and `.2EX` (big-endian).
//!
//! A file is a `PMAI` header followed by tagged sections. Every section is
//! `fourcc, len_header, len_tag` followed by tag-specific header fields
//! (up to `len_header`) and a payload (up to `len_tag`). [`AnlzFile`] keeps
//! both parts as bytes so any file can be rewritten unchanged; the typed
//! modules decode and build the sections we care about.

mod cues;
mod grid;
mod path;
mod waveform;

pub use cues::{Cue, CueList, CueListKind, CueType, ExtCue, ExtCueList};
pub use grid::{Beat, BeatGrid};
pub use path::{build_path, parse_path};
pub use waveform::{Waveform, WaveformKind};

use crate::error::{Bytes, Error, Result};

pub mod tag {
    pub const PATH: [u8; 4] = *b"PPTH";
    pub const VBR: [u8; 4] = *b"PVBR";
    pub const BEAT_GRID: [u8; 4] = *b"PQTZ";
    pub const BEAT_GRID_2: [u8; 4] = *b"PQT2";
    pub const CUES: [u8; 4] = *b"PCOB";
    pub const CUES_2: [u8; 4] = *b"PCO2";
    pub const WAVE_PREVIEW: [u8; 4] = *b"PWAV";
    pub const WAVE_TINY: [u8; 4] = *b"PWV2";
    pub const WAVE_SCROLL: [u8; 4] = *b"PWV3";
    pub const WAVE_COLOR_PREVIEW: [u8; 4] = *b"PWV4";
    pub const WAVE_COLOR_SCROLL: [u8; 4] = *b"PWV5";
    pub const WAVE_3BAND_PREVIEW: [u8; 4] = *b"PWV6";
    pub const WAVE_3BAND_SCROLL: [u8; 4] = *b"PWV7";
    pub const WAVE_COLOR_2EX: [u8; 4] = *b"PWVC";
    pub const SONG_STRUCTURE: [u8; 4] = *b"PSSI";
}

/// One tagged section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub tag: [u8; 4],
    /// Header fields after the common 12 bytes (`len_header - 12` bytes).
    pub header: Vec<u8>,
    /// Payload after the header (`len_tag - len_header` bytes).
    pub body: Vec<u8>,
}

impl Section {
    pub fn new(tag: [u8; 4], header: Vec<u8>, body: Vec<u8>) -> Self {
        Section { tag, header, body }
    }

    pub fn tag_str(&self) -> String {
        String::from_utf8_lossy(&self.tag).into_owned()
    }

    pub fn len(&self) -> usize {
        12 + self.header.len() + self.body.len()
    }

    pub fn is_empty(&self) -> bool {
        self.header.is_empty() && self.body.is_empty()
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tag);
        out.extend_from_slice(&((12 + self.header.len()) as u32).to_be_bytes());
        out.extend_from_slice(&(self.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.header);
        out.extend_from_slice(&self.body);
    }

    /// Big-endian u32 header field number `i`.
    pub(crate) fn header_u32(&self, i: usize) -> Result<u32> {
        Bytes(&self.header).u32be(4 * i)
    }
}

/// A whole analysis file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnlzFile {
    /// `PMAI` header bytes after `magic, len_header, len_file`.
    pub header: Vec<u8>,
    pub sections: Vec<Section>,
}

impl Default for AnlzFile {
    fn default() -> Self {
        // As written by rekordbox 6/7.
        AnlzFile {
            header: vec![0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
            sections: Vec::new(),
        }
    }
}

impl AnlzFile {
    pub fn parse(buf: &[u8]) -> Result<Self> {
        let b = Bytes(buf);
        if b.slice(0, 4)? != b"PMAI" {
            return Err(Error::Magic {
                offset: 0,
                expected: "PMAI",
            });
        }
        let len_header = b.u32be(4)? as usize;
        let len_file = b.u32be(8)? as usize;
        if len_header < 12 || len_file > buf.len() || len_header > len_file {
            return Err(Error::Invalid(format!(
                "bad PMAI lengths header={len_header} file={len_file} actual={}",
                buf.len()
            )));
        }
        let header = b.slice(12, len_header - 12)?.to_vec();
        let mut sections = Vec::new();
        let mut at = len_header;
        while at < len_file {
            let tag: [u8; 4] = b.slice(at, 4)?.try_into().unwrap();
            let lh = b.u32be(at + 4)? as usize;
            let lt = b.u32be(at + 8)? as usize;
            if lh < 12 || lt < lh || at + lt > len_file {
                return Err(Error::Invalid(format!(
                    "section {} at {at:#x}: bad lengths header={lh} tag={lt}",
                    String::from_utf8_lossy(&tag)
                )));
            }
            sections.push(Section {
                tag,
                header: b.slice(at + 12, lh - 12)?.to_vec(),
                body: b.slice(at + lh, lt - lh)?.to_vec(),
            });
            at += lt;
        }
        Ok(AnlzFile { header, sections })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let len_header = 12 + self.header.len();
        let len_file = len_header + self.sections.iter().map(Section::len).sum::<usize>();
        let mut out = Vec::with_capacity(len_file);
        out.extend_from_slice(b"PMAI");
        out.extend_from_slice(&(len_header as u32).to_be_bytes());
        out.extend_from_slice(&(len_file as u32).to_be_bytes());
        out.extend_from_slice(&self.header);
        for s in &self.sections {
            s.write(&mut out);
        }
        out
    }

    pub fn section(&self, tag: [u8; 4]) -> Option<&Section> {
        self.sections.iter().find(|s| s.tag == tag)
    }

    pub fn sections_with(&self, tag: [u8; 4]) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(move |s| s.tag == tag)
    }

    /// Replace the first section with the same tag, or append.
    pub fn set(&mut self, section: Section) {
        match self.sections.iter_mut().find(|s| s.tag == section.tag) {
            Some(s) => *s = section,
            None => self.sections.push(section),
        }
    }

    pub fn path(&self) -> Option<String> {
        self.section(tag::PATH).and_then(|s| parse_path(s).ok())
    }

    pub fn beat_grid(&self) -> Option<Result<BeatGrid>> {
        self.section(tag::BEAT_GRID).map(BeatGrid::parse)
    }

    /// Legacy cue lists (`PCOB`): usually one memory list and one hot cue list.
    pub fn cue_lists(&self) -> Result<Vec<CueList>> {
        self.sections_with(tag::CUES).map(CueList::parse).collect()
    }

    /// Extended cue lists (`PCO2`) with colours and comments.
    pub fn ext_cue_lists(&self) -> Result<Vec<ExtCueList>> {
        self.sections_with(tag::CUES_2)
            .map(ExtCueList::parse)
            .collect()
    }
}
