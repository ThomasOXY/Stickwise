//! Beats and downbeats from Beat This! (Foscarin, Schlüter, Widmer, ISMIR
//! 2024; MIT), a neural beat tracker, run in pure Rust (`beat-this`, rten).

use std::path::Path;
use std::sync::{Condvar, Mutex};

use beat_this::{BeatThis, RtenRuntime, Runtime};

type RtenModel = <RtenRuntime as Runtime>::Model;
type Model = BeatThis<RtenModel>;

use crate::{Error, Result};

/// The loaded models. One track at a time uses only part of the cores, so a
/// few models run side by side; analyses beyond that take turns.
pub struct Tracker {
    free: Mutex<Vec<Model>>,
    returned: Condvar,
}

/// A model on loan; goes back even if inference panics.
struct Loan<'a> {
    tracker: &'a Tracker,
    model: Option<Model>,
}

impl Drop for Loan<'_> {
    fn drop(&mut self) {
        if let Some(m) = self.model.take() {
            self.tracker.free.lock().unwrap_or_else(|e| e.into_inner()).push(m);
            self.tracker.returned.notify_one();
        }
    }
}

impl Tracker {
    /// `mel`: `mel_spectrogram.onnx`, `beat`: `beat_this.onnx` (or the small one).
    pub fn load(mel: &Path, beat: &Path) -> Result<Self> {
        Self::load_many(mel, beat, 1)
    }

    /// With `count` models, that many tracks run at once (each model holds
    /// its own copy of the weights, about 100 MB).
    pub fn load_many(mel: &Path, beat: &Path, count: usize) -> Result<Self> {
        let free = (0..count.max(1))
            .map(|_| BeatThis::new(&RtenRuntime, mel, beat).map_err(|e| Error::Model(e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        Ok(Tracker { free: Mutex::new(free), returned: Condvar::new() })
    }

    /// Beat and downbeat times in seconds of mono `samples`.
    pub fn beats(&self, samples: &[f32], rate: u32) -> Result<(Vec<f32>, Vec<f32>)> {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        let model = loop {
            match free.pop() {
                Some(m) => break m,
                None => free = self.returned.wait(free).unwrap_or_else(|e| e.into_inner()),
            }
        };
        drop(free);
        let mut loan = Loan { tracker: self, model: Some(model) };
        let a = loan
            .model
            .as_mut()
            .expect("the loan holds its model")
            .analyze_audio(samples, rate)
            .map_err(|e| Error::Model(e.to_string()))?;
        Ok((a.beats, a.downbeats))
    }
}
