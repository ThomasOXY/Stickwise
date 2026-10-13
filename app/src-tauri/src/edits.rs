//! Cue edits that have not been written to a stick yet, kept in the app's
//! data folder so they survive a restart. Keyed by stick name and the
//! track's path on the stick.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use pioneer_formats::anlz::CuePoint;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredCue {
    pub slot: u8,
    pub time_ms: u32,
    pub loop_end_ms: Option<u32>,
    pub loop_beats: Option<(u16, u16)>,
    pub comment: String,
    pub hot_color: u8,
    pub hot_led: Option<[u8; 3]>,
    pub memory_color: u8,
    pub active_loop: bool,
}

impl From<&StoredCue> for CuePoint {
    fn from(c: &StoredCue) -> Self {
        CuePoint {
            slot: c.slot,
            time_ms: c.time_ms,
            loop_end_ms: c.loop_end_ms,
            loop_beats: c.loop_beats,
            comment: c.comment.clone(),
            hot_color: c.hot_color,
            hot_led: c.hot_led,
            memory_color: c.memory_color,
            active_loop: c.active_loop,
        }
    }
}

impl From<&CuePoint> for StoredCue {
    fn from(c: &CuePoint) -> Self {
        StoredCue {
            slot: c.slot,
            time_ms: c.time_ms,
            loop_end_ms: c.loop_end_ms,
            loop_beats: c.loop_beats,
            comment: c.comment.clone(),
            hot_color: c.hot_color,
            hot_led: c.hot_led,
            memory_color: c.memory_color,
            active_loop: c.active_loop,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    /// "stick name|/Contents/…" → cues.
    cues: BTreeMap<String, Vec<StoredCue>>,
}

pub struct Edits {
    path: PathBuf,
    data: Mutex<File>,
}

pub fn key(stick: &str, file_path: &str) -> String {
    format!("{stick}|{file_path}")
}

impl Edits {
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Edits {
            path,
            data: Mutex::new(data),
        }
    }

    fn save(&self, data: &File) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    pub fn get(&self, key: &str) -> Option<Vec<StoredCue>> {
        self.data.lock().unwrap().cues.get(key).cloned()
    }

    pub fn set(&self, key: String, cues: Vec<StoredCue>) -> Result<(), String> {
        let mut d = self.data.lock().unwrap();
        d.cues.insert(key, cues);
        self.save(&d)
    }

    pub fn remove(&self, key: &str) -> Result<(), String> {
        let mut d = self.data.lock().unwrap();
        d.cues.remove(key);
        self.save(&d)
    }

    /// Edited file paths for one stick.
    pub fn pending(&self, stick: &str) -> Vec<String> {
        let prefix = format!("{stick}|");
        self.data
            .lock()
            .unwrap()
            .cues
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix).map(str::to_string))
            .collect()
    }
}
