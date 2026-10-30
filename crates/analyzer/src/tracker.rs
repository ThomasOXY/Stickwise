//! Beats and downbeats from Beat This! (Foscarin, Schlüter, Widmer, ISMIR
//! 2024; MIT), a neural beat tracker, run in pure Rust (`beat-this`, rten).

use std::path::Path;
use std::sync::Mutex;

use beat_this::{BeatThis, RtenRuntime, Runtime};

type RtenModel = <RtenRuntime as Runtime>::Model;

use crate::{Error, Result};

/// The loaded models. Inference already uses every core, so analyses
/// share one model and take turns.
pub struct Tracker(Mutex<BeatThis<RtenModel>>);

impl Tracker {
    /// `mel`: `mel_spectrogram.onnx`, `beat`: `beat_this.onnx` (or the small one).
    pub fn load(mel: &Path, beat: &Path) -> Result<Self> {
        BeatThis::new(&RtenRuntime, mel, beat)
            .map(|b| Tracker(Mutex::new(b)))
            .map_err(|e| Error::Model(e.to_string()))
    }

    /// Beat and downbeat times in seconds of mono `samples`.
    pub fn beats(&self, samples: &[f32], rate: u32) -> Result<(Vec<f32>, Vec<f32>)> {
        let mut bt = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let a = bt.analyze_audio(samples, rate).map_err(|e| Error::Model(e.to_string()))?;
        Ok((a.beats, a.downbeats))
    }
}
