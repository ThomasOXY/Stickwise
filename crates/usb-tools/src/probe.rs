//! Identify an audio file from its first bytes: container, codec, sample
//! rate, bit depth. Only the start of the file is read.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Container {
    Wav,
    Aiff,
    Flac,
    Mp3,
    Mp4,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    /// Integer PCM.
    Pcm,
    /// IEEE float PCM (WAV format tag 3, AIFC `fl32`/`fl64`).
    Float,
    Flac,
    Mp3,
    /// AAC or ALAC inside MP4 — not told apart yet.
    Mp4Audio,
    /// Anything else (compressed WAV/AIFC, …), with its tag.
    Other(u32),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInfo {
    pub container: Container,
    pub codec: Codec,
    pub sample_rate: Option<u32>,
    pub bits: Option<u16>,
    pub channels: Option<u16>,
    /// MP3 only: bytes of garbage between the end the ID3 tag declares and
    /// the first audio frame (a tag written with a wrong size).
    pub junk_after_tag: usize,
}

impl AudioInfo {
    fn unknown(container: Container) -> Self {
        AudioInfo {
            container,
            codec: Codec::Unknown,
            sample_rate: None,
            bits: None,
            channels: None,
            junk_after_tag: 0,
        }
    }

    pub fn describe(&self) -> String {
        let codec = match self.codec {
            Codec::Pcm => "PCM".to_string(),
            Codec::Float => "float".to_string(),
            Codec::Flac => "FLAC".to_string(),
            Codec::Mp3 => "MP3".to_string(),
            Codec::Mp4Audio => "AAC/ALAC".to_string(),
            Codec::Other(t) => format!("codec {t:#x}"),
            Codec::Unknown => "?".to_string(),
        };
        let mut s = format!("{:?} {codec}", self.container);
        if let Some(b) = self.bits {
            s += &format!(" {b}-bit");
        }
        if let Some(r) = self.sample_rate {
            s += &format!(" {:.1} kHz", r as f64 / 1000.0);
        }
        s
    }
}

const HEAD: usize = 256 * 1024;

pub fn probe_file(path: &Path) -> std::io::Result<AudioInfo> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(HEAD);
    f.by_ref().take(HEAD as u64).read_to_end(&mut buf)?;
    Ok(probe(&buf))
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u16be(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32be(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Probe from the first bytes of a file.
pub fn probe(h: &[u8]) -> AudioInfo {
    match h.get(..4) {
        Some(b"RIFF" | b"RF64") if h.get(8..12) == Some(b"WAVE") => wav(h),
        Some(b"FORM") if matches!(h.get(8..12), Some(b"AIFF" | b"AIFC")) => aiff(h),
        Some(b"fLaC") => flac(h),
        _ if h.get(4..8) == Some(b"ftyp") => AudioInfo {
            codec: Codec::Mp4Audio,
            ..AudioInfo::unknown(Container::Mp4)
        },
        _ => match mp3_junk(h) {
            Some(junk_after_tag) => AudioInfo {
                codec: Codec::Mp3,
                junk_after_tag,
                ..AudioInfo::unknown(Container::Mp3)
            },
            None => AudioInfo::unknown(Container::Unknown),
        },
    }
}

fn wav(h: &[u8]) -> AudioInfo {
    let mut o = 12;
    while let (Some(id), Some(size)) = (h.get(o..o + 4), u32le(h, o + 4)) {
        if id == b"fmt " {
            let p = o + 8;
            let mut tag = u16le(h, p).unwrap_or(0) as u32;
            // WAVE_FORMAT_EXTENSIBLE: the real format is the sub-format GUID's first word.
            if tag == 0xFFFE {
                tag = u16le(h, p + 24).map(u32::from).unwrap_or(tag);
            }
            let codec = match tag {
                1 => Codec::Pcm,
                3 => Codec::Float,
                t => Codec::Other(t),
            };
            return AudioInfo {
                container: Container::Wav,
                codec,
                channels: u16le(h, p + 2),
                sample_rate: u32le(h, p + 4),
                bits: u16le(h, p + 14),
                junk_after_tag: 0,
            };
        }
        o += 8 + size as usize + (size as usize & 1);
    }
    AudioInfo::unknown(Container::Wav)
}

/// 80-bit IEEE extended float as used by AIFF's sample rate.
fn extended_to_u32(b: &[u8]) -> Option<u32> {
    let exp = (u16be(b, 0)? & 0x7fff) as i32 - 16383;
    let mant = u64::from_be_bytes(b.get(2..10)?.try_into().ok()?);
    if !(0..=31).contains(&exp) {
        return None;
    }
    Some((mant >> (63 - exp)) as u32)
}

fn aiff(h: &[u8]) -> AudioInfo {
    let aifc = h.get(8..12) == Some(b"AIFC");
    let mut o = 12;
    while let (Some(id), Some(size)) = (h.get(o..o + 4), u32be(h, o + 4)) {
        if id == b"COMM" {
            let p = o + 8;
            let codec = if aifc {
                match h.get(p + 18..p + 22) {
                    Some(b"NONE" | b"twos" | b"sowt") => Codec::Pcm,
                    Some(b"fl32" | b"fl64" | b"FL32" | b"FL64") => Codec::Float,
                    Some(t) => Codec::Other(u32::from_be_bytes(t.try_into().unwrap())),
                    None => Codec::Unknown,
                }
            } else {
                Codec::Pcm
            };
            return AudioInfo {
                container: Container::Aiff,
                codec,
                channels: u16be(h, p),
                bits: u16be(h, p + 6),
                sample_rate: h.get(p + 8..p + 18).and_then(extended_to_u32),
                junk_after_tag: 0,
            };
        }
        o += 8 + size as usize + (size as usize & 1);
    }
    AudioInfo::unknown(Container::Aiff)
}

fn flac(h: &[u8]) -> AudioInfo {
    // STREAMINFO is always the first metadata block.
    let Some(si) = h.get(8..26) else {
        return AudioInfo::unknown(Container::Flac);
    };
    let v = u64::from_be_bytes(si[10..18].try_into().unwrap());
    AudioInfo {
        container: Container::Flac,
        codec: Codec::Flac,
        sample_rate: Some((v >> 44) as u32),
        channels: Some(((v >> 41) & 7) as u16 + 1),
        bits: Some(((v >> 36) & 31) as u16 + 1),
        junk_after_tag: 0,
    }
}

fn is_frame_sync(h: &[u8], o: usize) -> bool {
    // 11 sync bits, MPEG version != reserved, layer != reserved, bitrate != bad.
    h.get(o..o + 3).is_some_and(|b| {
        b[0] == 0xFF
            && b[1] & 0xE0 == 0xE0
            && b[1] & 0x18 != 0x08
            && b[1] & 0x06 != 0
            && b[2] & 0xF0 != 0xF0
    })
}

/// `Some(junk)` if this looks like an MP3: an ID3v2 tag (if any) followed —
/// possibly after `junk` stray bytes — by an MPEG audio frame.
fn mp3_junk(h: &[u8]) -> Option<usize> {
    let mut o = 0;
    if h.get(..3) == Some(b"ID3") {
        let s = h.get(6..10)?;
        o = 10
            + ((s[0] as usize) << 21
                | (s[1] as usize) << 14
                | (s[2] as usize) << 7
                | s[3] as usize);
        if h.get(5).is_some_and(|f| f & 0x10 != 0) {
            o += 10; // footer
        }
        if h.len() <= o {
            // Tag longer than what we read: trust the ID3 header.
            return Some(0);
        }
    }
    // A wrongly sized tag leaves bytes before the audio; look a little further.
    let limit = if o > 0 { 16 * 1024 } else { 0 };
    (o..=(o + limit).min(h.len().saturating_sub(3)))
        .find(|&i| is_frame_sync(h, i))
        .map(|i| i - o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_header(tag: u16, bits: u16, rate: u32) -> Vec<u8> {
        let mut h = b"RIFF\0\0\0\0WAVE".to_vec();
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&tag.to_le_bytes());
        h.extend_from_slice(&2u16.to_le_bytes());
        h.extend_from_slice(&rate.to_le_bytes());
        h.extend_from_slice(&(rate * 2 * bits as u32 / 8).to_le_bytes());
        h.extend_from_slice(&(2 * bits / 8).to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        h
    }

    #[test]
    fn wav_pcm_and_float() {
        let i = probe(&wav_header(1, 24, 48000));
        assert_eq!(
            (i.codec, i.bits, i.sample_rate),
            (Codec::Pcm, Some(24), Some(48000))
        );
        let i = probe(&wav_header(3, 32, 44100));
        assert_eq!((i.codec, i.bits), (Codec::Float, Some(32)));
    }

    #[test]
    fn wav_skips_leading_chunks() {
        let mut h = b"RIFF\0\0\0\0WAVE".to_vec();
        h.extend_from_slice(b"JUNK");
        h.extend_from_slice(&3u32.to_le_bytes());
        h.extend_from_slice(&[0, 0, 0, 0]); // 3 bytes + pad
        h.extend_from_slice(&wav_header(1, 16, 44100)[12..]);
        assert_eq!(probe(&h).bits, Some(16));
    }

    #[test]
    fn aiff_rate() {
        // 44100 Hz as 80-bit extended: 0x400E AC44 0000 0000 0000
        assert_eq!(
            extended_to_u32(&[0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]),
            Some(44100)
        );
    }

    #[test]
    fn mp3_and_flac() {
        assert_eq!(probe(&[0xFF, 0xFB, 0x90, 0x64]).codec, Codec::Mp3);
        // ID3 tag declaring 4 bytes, followed by 6 junk bytes, then audio.
        let mut m = b"ID3\x04\0\0\0\0\0\x04TAGS".to_vec();
        m.extend_from_slice(b"F\0L\0\0\0");
        m.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        let i = probe(&m);
        assert_eq!((i.codec, i.junk_after_tag), (Codec::Mp3, 6));
        let mut f = b"fLaC\0\0\0\x22".to_vec();
        // min/max block, min/max frame, then 20 bits rate, 3 ch-1, 5 bps-1
        f.extend_from_slice(&[0x10, 0, 0x10, 0, 0, 0, 0, 0, 0, 0]);
        let v: u64 = (96000u64 << 44) | (1 << 41) | (23 << 36);
        f.extend_from_slice(&v.to_be_bytes());
        let i = probe(&f);
        assert_eq!(
            (i.sample_rate, i.channels, i.bits),
            (Some(96000), Some(2), Some(24))
        );
    }
}
