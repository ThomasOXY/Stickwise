//! Data sent to the UI. Field names are camelCase on the wire.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StickInfo {
    pub path: String,
    pub name: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// `msdos`, `exfat`, `hfs`, …
    pub filesystem: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: u32,
    pub parent_id: u32,
    pub name: String,
    pub is_folder: bool,
    pub sort_order: u32,
    pub track_ids: Vec<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    /// Seconds.
    pub duration: u16,
    /// Short format label from the database, e.g. `MP3 320`, `WAV 24`.
    pub format: String,
    /// Path on the device, `/Contents/…`.
    pub file_path: String,
    /// Absolute path on this Mac.
    pub abs_path: String,
    pub rating: u8,
    pub file_size: u64,
    /// Absolute path of the artwork image, if the track has one.
    pub artwork: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickData {
    pub info: StickInfo,
    pub playlists: Vec<Playlist>,
    pub history: Vec<Playlist>,
    pub tracks: Vec<Track>,
    pub problems: Vec<String>,
}

/// Hot cue colours per track for the table: 8 slots, `None` = not set.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueSummary {
    pub track_id: u32,
    pub hot: Vec<Option<[u8; 3]>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    /// `three-band` (mid, high, low per column), `color` (PWV5 16-bit words)
    /// or `mono` (PWV3 / PWAV bytes).
    pub kind: &'static str,
    pub entry_len: usize,
    pub data: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    /// 0 = memory cue, 1..=8 = hot cue A..H.
    pub slot: u32,
    pub time_ms: u32,
    pub loop_end_ms: Option<u32>,
    pub color: Option<[u8; 3]>,
    pub comment: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub container: String,
    pub codec: String,
    pub bits: Option<u16>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDetail {
    pub track_id: u32,
    /// Whole-track waveforms, one per available style.
    pub previews: Vec<Waveform>,
    /// Scrolling waveforms at 150 columns per second, one per available style.
    pub details: Vec<Waveform>,
    pub cues: Vec<Cue>,
    /// Beat times in ms with their bar position (1 = downbeat).
    pub beats: Vec<(u32, u16)>,
    pub audio: Option<AudioInfo>,
    pub file_size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerResult {
    pub player: String,
    pub filesystem_ok: bool,
    pub filesystem_note: String,
    /// Tracks this player cannot play.
    pub unplayable: Vec<u32>,
    /// Short summary of format problems, e.g. "4 × WAV 32f".
    pub format_note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub track_id: u32,
    pub title: String,
    pub file_path: String,
    pub playlists: Vec<String>,
    /// `error` or `warning`.
    pub severity: &'static str,
    /// Machine-readable kind: `float`, `broken-tag`, `size`, `missing`, …
    pub kind: &'static str,
    pub message: String,
    pub audio: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub tracks: usize,
    pub filesystem: String,
    pub players: Vec<PlayerResult>,
    pub issues: Vec<Issue>,
    pub apple_double: usize,
    pub db_problems: Vec<String>,
    pub formats: Vec<(String, usize)>,
    pub seconds: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    pub stick: String,
    pub dest: String,
    pub playlists: Vec<u32>,
    pub numbered: bool,
    pub artist_title: bool,
    pub m3u: bool,
    pub history: bool,
    /// `clone`, `copy` or `symlink`.
    pub duplicates: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub id: u32,
    pub label: String,
    /// `running`, `done`, `cancelled`, `error`.
    pub state: &'static str,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
    pub message: String,
    pub dest: String,
}
