//! The local Python analysis sidecar (CUE-DETR) and its result cache.
//!
//! The sidecar lives in `sidecar/` of the project (a `uv` environment);
//! results are cached per track in the app data folder, so a track is only
//! ever analysed once.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Candidate {
    /// Seconds.
    pub time: f64,
    /// 0..1, relative to the strongest candidate in the track.
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiCues {
    pub duration: f64,
    pub candidates: Vec<Candidate>,
}

pub struct Sidecar {
    pub dir: PathBuf,
    pub data: PathBuf,
}

impl Sidecar {
    pub fn new(data: PathBuf) -> Self {
        let dir = std::env::var_os("RBX_SIDECAR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sidecar"));
        Sidecar { dir, data }
    }

    fn python(&self) -> PathBuf {
        self.dir.join(".venv/bin/python")
    }

    pub fn installed(&self) -> bool {
        self.python().is_file() && self.dir.join("cue_detr.py").is_file()
    }

    fn cache_dir(&self) -> PathBuf {
        self.data.join("cue-detr")
    }

    /// Cache key: stick name, path on the stick and file size.
    pub fn key(stick: &str, file_path: &str, size: u64) -> String {
        // FNV-1a, enough to name a cache file.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{stick}|{file_path}|{size}").bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    }

    pub fn cached(&self, key: &str) -> Option<AiCues> {
        let b = std::fs::read(self.cache_dir().join(format!("{key}.json"))).ok()?;
        serde_json::from_slice(&b).ok()
    }

    fn store(&self, key: &str, cues: &AiCues) {
        let dir = self.cache_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(
                dir.join(format!("{key}.json")),
                serde_json::to_vec(cues).unwrap_or_default(),
            );
        }
    }

    /// Analyse `jobs` (cache key, audio path). `progress(done, current)` is
    /// called after each track. Returns the number analysed and the errors.
    pub fn run(
        &self,
        jobs: &[(String, PathBuf)],
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize, &str),
    ) -> Result<(usize, Vec<String>), String> {
        if !self.installed() {
            return Err("AI analýza není nainstalovaná (chybí sidecar/.venv).".into());
        }
        let by_path: HashMap<String, &String> = jobs
            .iter()
            .map(|(k, p)| (p.display().to_string(), k))
            .collect();
        let mut child = Command::new(self.python())
            .arg(self.dir.join("cue_detr.py"))
            .args(jobs.iter().map(|(_, p)| p))
            .env("HF_HOME", self.data.join("hf"))
            .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("AI analýzu nejde spustit: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let (mut done, mut errors) = (0, Vec::new());
        for line in BufReader::new(stdout).lines() {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let path = v["path"].as_str().unwrap_or_default().to_string();
            if let Some(e) = v["error"].as_str() {
                errors.push(format!("{path}: {e}"));
            } else if let (Some(key), Ok(cues)) = (
                by_path.get(&path),
                serde_json::from_value::<AiCues>(v.clone()),
            ) {
                self.store(key, &cues);
            }
            done += 1;
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            progress(done, &name);
        }
        let _ = child.wait();
        Ok((done, errors))
    }
}
