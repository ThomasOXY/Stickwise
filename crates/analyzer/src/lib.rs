//! Analysis of tracks rekordbox has not seen: decoding, beat grid, key and
//! waveforms, written as `ANLZ0000.DAT/.EXT/.2EX` like rekordbox's own, so
//! the app and the players show them the same way.
//!
//! Beats: Beat This! (neural, MIT) gives tempo and bar; the exact tempo and
//! the phase come from onsets, the way rekordbox places its grids.
//! Measured against rekordbox on 158 tracks of a DJ's library: tempo
//! agrees on 79 %, of those the grid lies within 15 ms on 86 % and the bar
//! one on 92 %; the key matches exactly on 44 %. Tags win for the key.

pub mod bands;
pub mod beats;
pub mod decode;
pub mod key;
pub mod tracker;
pub mod waveform;

use std::path::{Path, PathBuf};

use pioneer_formats::anlz::{AnlzFile, Beat, BeatGrid, Section, build_path, complete, tag, write_cue_points};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{}", i18n::t!("analyzer.cantDecode", error = .0))]
    Decode(String),
    #[error("model Beat This!: {0}")]
    Model(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Frames per second of the measurements beats are found in (a multiple
/// of the waveforms' 150).
pub const BEAT_FPS: f64 = 600.0;

pub struct Analysis {
    /// Seconds.
    pub duration: f64,
    pub grid: Option<beats::Grid>,
    /// As rekordbox names keys ("Am", "F#").
    pub key: Option<&'static str>,
    pub waveforms: waveform::Waveforms,
    /// Length in samples for an MP3 (rekordbox's `PVBR` holds it), else 0.
    pub mp3_samples: u32,
}

/// Analyse an audio file. With a `tracker` the beats come from Beat This!,
/// otherwise from the built-in detector (`bpm_hint`: tempo from the tags).
pub fn analyze(path: &Path, bpm_hint: Option<f64>, tracker: Option<&tracker::Tracker>) -> Result<Analysis> {
    let audio = decode::decode(path)?;
    let frames = bands::frames(&audio.samples, audio.rate, BEAT_FPS);
    let grid = match tracker {
        Some(t) => {
            let (b, d) = t.beats(&audio.samples, audio.rate)?;
            // A drifting tempo (live bands) has no steady grid to fit.
            beats::refine(&frames, &b, &d).or_else(|| beats::detect(&frames, bpm_hint.filter(|b| *b > 0.0)))
        }
        None => beats::detect(&frames, bpm_hint.filter(|b| *b > 0.0)),
    };
    Ok(Analysis {
        duration: audio.duration(),
        grid,
        key: key::detect(&audio.samples, audio.rate),
        waveforms: waveform::build(&frames),
        mp3_samples: if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp3")) {
            audio.samples.len() as u32
        } else {
            0
        },
    })
}

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_be_bytes()).collect()
}

/// The three analysis files, as rekordbox lays them out. `audio_path` goes
/// into their `PPTH` (a sync rewrites it for the stick).
pub fn anlz_files(a: &Analysis, audio_path: &str) -> [AnlzFile; 3] {
    let w = &a.waveforms;
    let path = build_path(audio_path);
    let scroll_cols = w.scroll.len() as u32;

    let mut dat = AnlzFile::default();
    dat.sections.push(path.clone());
    if let Some(g) = &a.grid {
        let tempo = (g.bpm * 100.0).round() as u16;
        let beats = g
            .beats(a.duration)
            .into_iter()
            .map(|(t, n)| Beat {
                beat_number: n as u16,
                tempo,
                time_ms: (t * 1000.0).round() as u32,
            })
            .collect();
        dat.sections.push(BeatGrid::new(beats).to_section());
    }
    dat.sections.push(Section::new(tag::WAVE_PREVIEW, u32s(&[400, 0x0001_0000]), w.preview.clone()));
    dat.sections.push(Section::new(tag::WAVE_TINY, u32s(&[100, 0x0001_0000]), w.tiny.clone()));

    let mut ext = AnlzFile::default();
    ext.sections.push(path.clone());
    ext.sections.push(Section::new(tag::WAVE_SCROLL, u32s(&[1, scroll_cols, 0x0096_0000]), w.scroll.clone()));
    ext.sections.push(Section::new(
        tag::WAVE_COLOR_SCROLL,
        u32s(&[2, scroll_cols, 0x0096_0305]),
        w.color_scroll.clone(),
    ));
    write_cue_points(&mut dat, Some(&mut ext), &[]);

    let mut two = AnlzFile::default();
    two.sections.push(path);
    two.sections.push(Section::new(
        tag::WAVE_3BAND_SCROLL,
        u32s(&[3, scroll_cols, 0x0096_0000]),
        w.three_band_scroll.clone(),
    ));
    two.sections.push(Section::new(tag::WAVE_3BAND_PREVIEW, u32s(&[3, 1200]), w.three_band_preview.clone()));
    // rekordbox's other sections, in its order: without them it shows
    // neither grid nor waveform.
    complete(&mut dat, Some(&mut ext), Some(&mut two), a.mp3_samples);
    [dat, ext, two]
}

/// Write `ANLZ0000.DAT/.EXT/.2EX` into `dir`; returns the `.DAT` path.
pub fn write(a: &Analysis, audio_path: &str, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for (file, ext) in anlz_files(a, audio_path).iter().zip(["DAT", "EXT", "2EX"]) {
        let p = dir.join(format!("ANLZ0000.{ext}"));
        let tmp = p.with_extension(format!("{ext}.tmp"));
        std::fs::write(&tmp, file.to_bytes())?;
        std::fs::rename(&tmp, &p)?;
    }
    Ok(dir.join("ANLZ0000.DAT"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kick track at 120 BPM whose level changes every 4 bars, as
    /// sections of a club track do.
    fn clicks(seconds: f64, rate: u32) -> Vec<f32> {
        let mut s = vec![0f32; (seconds * rate as f64) as usize];
        let beat = 0.5;
        let mut k = 0;
        loop {
            let t = 0.25 + k as f64 * beat;
            let at = (t * rate as f64) as usize;
            if at + 2000 >= s.len() {
                break;
            }
            let amp = if (k / 16) % 2 == 0 { 0.9 } else { 0.35 };
            for i in 0..2000 {
                let e = (-(i as f32) / 300.0).exp();
                // A kick (60 Hz) with a click on top.
                s[at + i] += amp * e * ((i as f32 * 2.0 * std::f32::consts::PI * 60.0 / rate as f32).sin()
                    + 0.5 * ((i * 7919 % 13) as f32 / 6.0 - 1.0));
            }
            k += 1;
        }
        s
    }

    fn wav(path: &Path, s: &[f32], rate: u32) {
        let mut b = Vec::new();
        let data = s.len() as u32 * 2;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data.to_le_bytes());
        for x in s {
            b.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn click_track_gets_grid_waveforms_and_files() {
        let d = tempfile::tempdir().unwrap();
        let audio = d.path().join("clicks.wav");
        wav(&audio, &clicks(40.0, 44100), 44100);
        let a = analyze(&audio, None, None).unwrap();
        let g = a.grid.unwrap();
        assert_eq!(g.bpm, 120.0);
        assert!((g.first - 0.25).abs() < 0.012, "first beat at {}", g.first);
        assert_eq!(g.first_number, 1);
        assert_eq!(a.waveforms.preview.len(), 400);
        assert_eq!(a.waveforms.three_band_preview.len(), 3600);
        assert_eq!(a.waveforms.scroll.len(), 6000);

        let dat = write(&a, "/m/clicks.wav", &d.path().join("anlz")).unwrap();
        let read = |ext: &str| AnlzFile::parse(&std::fs::read(dat.with_extension(ext)).unwrap()).unwrap();
        let f = read("DAT");
        assert_eq!(f.path().as_deref(), Some("/m/clicks.wav"));
        let beats = f.beat_grid().unwrap().unwrap().beats;
        assert_eq!((beats[0].beat_number, beats[0].tempo, beats[1].beat_number), (1, 12000, 2));
        let tags = |f: &AnlzFile| f.sections.iter().map(|s| s.tag_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(tags(&f), "PPTH PVBR PQTZ PWAV PWV2 PCOB PCOB");
        assert_eq!(tags(&read("EXT")), "PPTH PWV3 PCOB PCOB PCO2 PCO2 PQT2 PWV5 PWV4");
        assert_eq!(tags(&read("2EX")), "PPTH PWV7 PWV6 PWVC");
    }
}
