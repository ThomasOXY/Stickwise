use super::{Section, tag};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformKind {
    /// `PWAV`: 400 columns, 1 byte each (height 5 bits, whiteness 3 bits).
    Preview,
    /// `PWV2`: 100 columns for small displays.
    Tiny,
    /// `PWV3`: 150 columns per second, 1 byte each.
    Scroll,
    /// `PWV4`: 1200 columns, 6 bytes each.
    ColorPreview,
    /// `PWV5`: 150 columns per second, 2 bytes each (RGB + height).
    ColorScroll,
    /// `PWV6`: CDJ-3000 3-band preview, 3 bytes per column.
    ThreeBandPreview,
    /// `PWV7`: CDJ-3000 3-band scroll, 3 bytes per column.
    ThreeBandScroll,
}

impl WaveformKind {
    pub fn tag(self) -> [u8; 4] {
        match self {
            WaveformKind::Preview => tag::WAVE_PREVIEW,
            WaveformKind::Tiny => tag::WAVE_TINY,
            WaveformKind::Scroll => tag::WAVE_SCROLL,
            WaveformKind::ColorPreview => tag::WAVE_COLOR_PREVIEW,
            WaveformKind::ColorScroll => tag::WAVE_COLOR_SCROLL,
            WaveformKind::ThreeBandPreview => tag::WAVE_3BAND_PREVIEW,
            WaveformKind::ThreeBandScroll => tag::WAVE_3BAND_SCROLL,
        }
    }

    pub fn from_tag(t: [u8; 4]) -> Option<Self> {
        Some(match &t {
            b"PWAV" => WaveformKind::Preview,
            b"PWV2" => WaveformKind::Tiny,
            b"PWV3" => WaveformKind::Scroll,
            b"PWV4" => WaveformKind::ColorPreview,
            b"PWV5" => WaveformKind::ColorScroll,
            b"PWV6" => WaveformKind::ThreeBandPreview,
            b"PWV7" => WaveformKind::ThreeBandScroll,
            _ => return None,
        })
    }

    /// Bytes per column.
    pub fn entry_len(self) -> usize {
        match self {
            WaveformKind::Preview | WaveformKind::Tiny | WaveformKind::Scroll => 1,
            WaveformKind::ColorScroll => 2,
            WaveformKind::ThreeBandPreview | WaveformKind::ThreeBandScroll => 3,
            WaveformKind::ColorPreview => 6,
        }
    }
}

/// Any waveform section: its column data plus the raw header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waveform {
    pub kind: WaveformKind,
    pub header: Vec<u8>,
    pub data: Vec<u8>,
}

impl Waveform {
    pub fn parse(s: &Section) -> Result<Self> {
        let kind = WaveformKind::from_tag(s.tag)
            .ok_or_else(|| Error::Invalid(format!("{} is not a waveform", s.tag_str())))?;
        Ok(Waveform {
            kind,
            header: s.header.clone(),
            data: s.body.clone(),
        })
    }

    pub fn columns(&self) -> usize {
        self.data.len() / self.kind.entry_len()
    }

    pub fn to_section(&self) -> Section {
        Section::new(self.kind.tag(), self.header.clone(), self.data.clone())
    }
}
