//! Audio file → mono samples.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{Error, Result};

/// The whole track as one channel.
pub struct Audio {
    pub samples: Vec<f32>,
    pub rate: u32,
}

impl Audio {
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.rate as f64
    }
}

/// Decode `path` and mix it down to mono.
pub fn decode(path: &Path) -> Result<Audio> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            // No gapless trimming: players count time from the first decoded sample.
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| Error::Decode("no audio track".into()))?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;

    let mut samples = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged frame: skip it, as players do.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let channels = spec.channels.count().max(1);
        let b = match &mut buf {
            Some(b) if b.capacity() >= decoded.capacity() * channels => b,
            _ => buf.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        b.copy_interleaved_ref(decoded);
        let s = b.samples();
        samples.reserve(s.len() / channels);
        for frame in s.chunks_exact(channels) {
            samples.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
    if rate == 0 || samples.is_empty() {
        return Err(Error::Decode("no audio decoded".into()));
    }
    let lead = lame_info_frame(path);
    if lead > 0 {
        samples.splice(0..0, std::iter::repeat_n(0.0, lead));
    }
    Ok(Audio { samples, rate })
}

/// Samples of an MP3's LAME header frame. Symphonia drops that frame;
/// rekordbox counts it as silence, so its beat grids sit one frame later.
/// Measured on rekordbox's own analyses: every LAME-tagged file was off by
/// exactly one frame, files from other encoders were not.
fn lame_info_frame(path: &Path) -> usize {
    let is_mp3 = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if !is_mp3 {
        return 0;
    }
    let Ok(f) = File::open(path) else { return 0 };
    let mut b = Vec::new();
    if std::io::Read::read_to_end(&mut std::io::Read::take(f, 256 * 1024), &mut b).is_err() {
        return 0;
    }
    let mut i = 0;
    if b.len() > 10 && &b[..3] == b"ID3" {
        let size = (b[6] as usize) << 21 | (b[7] as usize) << 14 | (b[8] as usize) << 7 | b[9] as usize;
        i = 10 + size + if b[5] & 0x10 != 0 { 10 } else { 0 };
    }
    while i + 4 < b.len() && !(b[i] == 0xFF && b[i + 1] & 0xE0 == 0xE0) {
        i += 1;
    }
    if i + 4 >= b.len() {
        return 0;
    }
    // MPEG-1 layer III: 1152 samples per frame; MPEG-2 / 2.5: 576.
    let mpeg1 = b[i + 1] & 0x18 == 0x18;
    let frame = &b[i..(i + 400).min(b.len())];
    let has = |t: &[u8]| frame.windows(t.len()).any(|w| w == t);
    if (has(b"Info") || has(b"Xing")) && has(b"LAME") {
        if mpeg1 { 1152 } else { 576 }
    } else {
        0
    }
}
