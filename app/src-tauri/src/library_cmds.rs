//! Commands for the local library: browse, import from rekordbox, edit
//! playlists and sets, cues, Smart Cue, and sync a selection to a stick.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use library::{Library, Playlist, RekordboxDump};
use pioneer_formats::anlz::CuePoint;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use usb_tools::sync::{self, SyncPlaylist, SyncRequest, SyncTrack};

use crate::commands::{
    CmdResult, blocking, build_detail, cue_dto, cue_point, display_color, err, plan_cues,
    read_anlz, smart_dto, suggestions_for, timestamp,
};
use crate::dto;
use crate::jobs::Jobs;
use crate::sidecar::Sidecar;

pub struct LibState(pub Mutex<Library>);

pub(crate) fn lib<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Library) -> library::Result<T>,
) -> CmdResult<T> {
    let state = app.state::<LibState>();
    let mut l = state.0.lock().map_err(err)?;
    f(&mut l).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    pub duration: u16,
    pub format: String,
    pub file_path: String,
    pub abs_path: String,
    pub rating: u8,
    pub file_size: u64,
    pub artwork: Option<String>,
    /// The audio file is not where the library says.
    pub missing: bool,
    /// Has rekordbox analysis (waveform, beat grid).
    pub analyzed: bool,
    /// `YYYY-MM-DD`, empty if unknown.
    pub date_added: String,
    /// The analysis is this app's own (re-analysable), not rekordbox's.
    pub own_analysis: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryData {
    pub tracks: Vec<LibTrack>,
    pub playlists: Vec<Playlist>,
    /// rekordbox library folders found on this Mac.
    pub rekordbox_sources: Vec<String>,
    pub last_import: Option<String>,
}

fn format_label(path: &str, bitrate: u32, depth: u32) -> String {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    match ext.as_str() {
        "MP3" | "M4A" | "AAC" => format!("{ext} {bitrate}"),
        "AIF" => format!("AIFF {depth}"),
        _ if depth > 0 => format!("{ext} {depth}"),
        _ => ext,
    }
}

/// rekordbox library folders (holding master.db), newest first.
pub(crate) fn rekordbox_sources() -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = Vec::new();
    let mut check = |dir: PathBuf| {
        if let Ok(m) = std::fs::metadata(dir.join("master.db"))
            && let Ok(t) = m.modified()
        {
            found.push((t, dir.display().to_string()));
        }
    };
    if let Ok(home) = std::env::var("HOME") {
        check(PathBuf::from(home).join("Library/Pioneer/rekordbox"));
    }
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        for v in rd.flatten() {
            check(v.path().join("PIONEER/Master"));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|f| f.1).collect()
}

#[tauri::command]
pub async fn library_data(app: AppHandle) -> CmdResult<LibraryData> {
    let own = app.path().app_data_dir().map_err(err)?.join("analysis");
    blocking(move || {
        let (tracks, playlists, last) = lib(&app, |l| {
            Ok((l.tracks()?, l.playlists()?, l.meta("last_import")?))
        })?;
        let tracks = tracks
            .into_iter()
            .map(|t| {
                let size = std::fs::metadata(&t.path).map(|m| m.len()).ok();
                LibTrack {
                    id: t.id,
                    format: format_label(&t.path, t.bitrate, t.sample_depth),
                    file_path: t.path.clone(),
                    abs_path: t.path.clone(),
                    missing: size.is_none(),
                    file_size: size.unwrap_or(0),
                    analyzed: !t.anlz.is_empty() && Path::new(&t.anlz).is_file(),
                    artwork: (!t.artwork.is_empty() && Path::new(&t.artwork).is_file())
                        .then(|| t.artwork.clone()),
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    genre: t.genre,
                    bpm: t.bpm,
                    key: t.key,
                    duration: t.duration.round() as u16,
                    rating: t.rating,
                    date_added: t.date_added,
                    own_analysis: Path::new(&t.anlz).starts_with(&own),
                }
            })
            .collect();
        Ok(LibraryData {
            tracks,
            playlists,
            rekordbox_sources: rekordbox_sources(),
            last_import: last,
        })
    })
    .await
}

/// Import (or refresh) the library from rekordbox.
#[tauri::command]
pub async fn library_import(app: AppHandle, source: String) -> CmdResult<library::ImportReport> {
    blocking(move || {
        crate::sidecar::ensure_installed(&app)?;
        let sc = app.state::<Sidecar>();
        let out = sc.data.join("rekordbox-dump.json");
        let status = std::process::Command::new(sc.dir.join(".venv/bin/python"))
            .arg(sc.script("rekordbox_dump.py"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg(&source)
            .arg(&out)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| i18n::t!("app.library.importCantStart", error = e))?;
        if !status.status.success() {
            let msg = String::from_utf8_lossy(&status.stderr);
            let last = msg
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map_or_else(|| i18n::t!("app.library.unknownError"), str::to_string);
            return Err(i18n::t!("app.library.rekordboxUnreadable", error = last));
        }
        let dump: RekordboxDump =
            serde_json::from_slice(&std::fs::read(&out).map_err(err)?).map_err(err)?;
        let report = lib(&app, |l| {
            let r = l.import_rekordbox(&dump)?;
            l.set_meta("last_import", &format!("{} z {source}", timestamp()))?;
            Ok(r)
        })?;
        let _ = std::fs::remove_file(&out);
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn library_create_playlist(
    app: AppHandle,
    parent: i64,
    name: String,
    kind: String,
) -> CmdResult<i64> {
    lib(&app, |l| {
        l.create_playlist(parent, &name, kind == "folder", kind == "set")
    })
}

#[tauri::command]
pub fn library_rename_playlist(app: AppHandle, id: i64, name: String) -> CmdResult<()> {
    lib(&app, |l| l.rename_playlist(id, &name))
}

#[tauri::command]
pub fn library_delete_playlist(app: AppHandle, id: i64) -> CmdResult<()> {
    lib(&app, |l| l.delete_playlist(id))
}

#[tauri::command]
pub fn library_move_playlist(app: AppHandle, id: i64, parent: i64, index: usize) -> CmdResult<()> {
    lib(&app, |l| l.move_playlist(id, parent, index))
}

#[tauri::command]
pub fn library_order_playlists(app: AppHandle, groups: Vec<Vec<i64>>) -> CmdResult<()> {
    lib(&app, |l| groups.iter().try_for_each(|ids| l.order_playlists(ids)))
}

#[tauri::command]
pub fn library_set_tracks(app: AppHandle, id: i64, track_ids: Vec<i64>) -> CmdResult<()> {
    lib(&app, |l| l.set_playlist_tracks(id, &track_ids))
}

/// Folders and audio files dropped on the playlist list. Folders become
/// playlists under `parent`; loose files go into `playlist` if dropped on one.
#[tauri::command]
pub async fn library_add_folders(
    app: AppHandle,
    paths: Vec<PathBuf>,
    parent: i64,
    playlist: Option<i64>,
) -> CmdResult<library::FolderReport> {
    blocking(move || {
        // Reading tags is the slow part: done before taking the library.
        let scan = library::folders::scan(&paths);
        lib(&app, |l| {
            let r = l.add_scan(&scan, parent)?;
            if let Some(p) = playlist.filter(|_| !r.loose.is_empty()) {
                l.add_to_playlist(p, &r.loose)?;
            }
            Ok(r)
        })
    })
    .await
}

fn track(app: &AppHandle, id: i64) -> CmdResult<library::Track> {
    lib(app, |l| l.track(id))?.ok_or_else(|| i18n::t!("app.library.trackNotFound", id = id))
}

fn ai_key(t: &library::Track) -> String {
    let size = std::fs::metadata(&t.path).map(|m| m.len()).unwrap_or(0);
    Sidecar::key("library", &t.path, size)
}

/// The DJ's edited grid of a track, if any.
fn edited_grid(app: &AppHandle, id: i64) -> Option<analysis::grid::Grid> {
    lib(app, |l| l.grid(id))
        .ok()
        .flatten()
        .map(|(bpm, anchor_ms)| analysis::grid::Grid { bpm, anchor_ms })
}

/// Beats of an edited grid over the whole track.
fn edited_beats(app: &AppHandle, t: &library::Track) -> Option<Vec<(u32, u16)>> {
    let g = edited_grid(app, t.id)?;
    Some(g.beats(track_ms(t)))
}

/// Length of a track for laying a grid over it.
fn track_ms(t: &library::Track) -> f64 {
    if t.duration > 0.0 { t.duration * 1000.0 } else { 3_600_000.0 }
}

#[tauri::command]
pub async fn library_track_detail(app: AppHandle, id: i64) -> CmdResult<dto::TrackDetail> {
    blocking(move || {
        let t = track(&app, id)?;
        let cues = lib(&app, |l| l.cues(id))?;
        let dat = (!t.anlz.is_empty()).then(|| PathBuf::from(&t.anlz));
        let mut d = build_detail(id as u32, dat.as_deref(), Path::new(&t.path), &cues, false);
        if let Some(g) = edited_grid(&app, id) {
            d.beats = g.beats(track_ms(&t));
            d.grid = Some(dto::GridInfo {
                bpm: g.bpm,
                anchor_ms: g.anchor_ms,
                edited: true,
            });
        }
        Ok(d)
    })
    .await
}

/// Keep an edited beat grid (tempo and one bar one in ms).
#[tauri::command]
pub fn library_set_grid(app: AppHandle, id: i64, bpm: f64, anchor_ms: f64) -> CmdResult<()> {
    if !(20.0..=400.0).contains(&bpm) || !anchor_ms.is_finite() {
        return Err(i18n::t!("app.library.invalidGrid", bpm = bpm));
    }
    lib(&app, |l| l.set_grid(id, bpm, anchor_ms))
}

/// Back to the analysed grid and its tempo.
#[tauri::command]
pub async fn library_reset_grid(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(move || {
        let t = track(&app, id)?;
        let bpm = (!t.anlz.is_empty())
            .then(|| read_anlz(Path::new(&t.anlz)))
            .flatten()
            .and_then(|d| d.beat_grid())
            .and_then(|g| g.ok())
            .and_then(|g| {
                let beats: Vec<(u32, u16)> = g.beats.iter().map(|b| (b.time_ms, b.beat_number)).collect();
                analysis::grid::Grid::from_beats(&beats)
            })
            .map(|g| (g.bpm * 100.0).round() / 100.0);
        lib(&app, |l| l.clear_grid(id, bpm))
    })
    .await
}

#[tauri::command]
pub async fn library_save_cues(
    app: AppHandle,
    id: i64,
    cues: Vec<dto::Cue>,
) -> CmdResult<Vec<dto::Cue>> {
    blocking(move || {
        let points: Vec<CuePoint> = cues.iter().map(cue_point).collect();
        lib(&app, |l| l.set_cues(id, &points))?;
        let mut out: Vec<dto::Cue> = points.iter().map(cue_dto).collect();
        out.sort_by_key(|c| (c.slot == 0, c.slot, c.time_ms));
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn library_cue_summary(
    app: AppHandle,
    track_ids: Vec<i64>,
) -> CmdResult<Vec<dto::CueSummary>> {
    blocking(move || {
        let mut out = Vec::new();
        for id in track_ids {
            let cues = lib(&app, |l| l.cues(id))?;
            let mut hot = vec![None; 8];
            for c in cues.iter().filter(|c| (1..=8).contains(&c.slot)) {
                hot[c.slot as usize - 1] = Some(display_color(c));
            }
            out.push(dto::CueSummary {
                track_id: id as u32,
                hot,
                memory: cues.iter().filter(|c| c.slot == 0).count(),
            });
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn library_smart_cue(app: AppHandle, id: i64) -> CmdResult<Vec<dto::SmartCue>> {
    blocking(move || {
        let t = track(&app, id)?;
        let s = suggestions_for(&app, Path::new(&t.anlz), t.bpm as f32, &ai_key(&t), edited_beats(&app, &t).as_deref())
            .ok_or_else(|| i18n::t!("app.smartCue.noAnalysis"))?;
        Ok(smart_dto(&s))
    })
    .await
}

#[tauri::command]
pub async fn library_smart_cue_apply(
    app: AppHandle,
    track_ids: Vec<i64>,
    replace: bool,
) -> CmdResult<dto::SmartCueBatch> {
    blocking(move || {
        let mut r = dto::SmartCueBatch {
            applied: 0,
            skipped: 0,
            failed: 0,
        };
        for id in track_ids {
            let Ok(t) = track(&app, id) else { continue };
            let cues = lib(&app, |l| l.cues(id))?;
            if cues.iter().any(|c| c.slot > 0) && !replace {
                r.skipped += 1;
                continue;
            }
            let Some(s) = suggestions_for(&app, Path::new(&t.anlz), t.bpm as f32, &ai_key(&t), edited_beats(&app, &t).as_deref())
            else {
                r.failed += 1;
                continue;
            };
            let next = plan_cues(&cues, &s);
            lib(&app, |l| l.set_cues(id, &next))?;
            r.applied += 1;
        }
        Ok(r)
    })
    .await
}

#[tauri::command]
pub async fn library_ai_status(
    app: AppHandle,
    track_ids: Vec<i64>,
) -> CmdResult<crate::commands::AiStatus> {
    blocking(move || {
        let sc = app.state::<Sidecar>();
        let analyzed = track_ids
            .into_iter()
            .filter(|&id| track(&app, id).is_ok_and(|t| sc.cached(&ai_key(&t)).is_some()))
            .map(|id| id as u32)
            .collect();
        Ok(crate::commands::AiStatus {
            installed: sc.available(),
            analyzed,
        })
    })
    .await
}

pub(crate) fn emit_job(app: &AppHandle, ev: &dto::JobEvent) {
    let _ = app.emit("job", ev.clone());
}

/// Where this app keeps its own analysis files.
pub(crate) fn analysis_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_data_dir().map_err(err)?.join("analysis"))
}

/// Library tracks of `ids` that need analysing: no analysis, or with
/// `again` also this app's own. rekordbox's analyses are never replaced.
fn needs_analysis(app: &AppHandle, ids: impl IntoIterator<Item = i64>, again: bool) -> CmdResult<Vec<library::Track>> {
    let dir = analysis_dir(app)?;
    let mut seen = HashSet::new();
    Ok(ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .filter_map(|id| track(app, id).ok())
        .filter(|t| {
            let missing = t.anlz.is_empty() || !Path::new(&t.anlz).is_file();
            (missing || (again && Path::new(&t.anlz).starts_with(&dir))) && Path::new(&t.path).is_file()
        })
        .collect())
}

/// Modification time of a file, seconds (0 when unknown).
fn file_mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Analyse `todo` (waveforms, beat grid, key) into the app's data folder
/// and record it in the library. `progress(done, file name)` after each
/// track. Returns how many were done, the errors, and whether Beat This!
/// was used.
fn analyze_all(
    app: &AppHandle,
    todo: &[library::Track],
    cancel: &std::sync::atomic::AtomicBool,
    progress: impl Fn(usize, &str) + Sync,
) -> (usize, Vec<String>, bool) {
    let next = std::sync::atomic::AtomicUsize::new(0);
    analyze_from(app, || todo.get(next.fetch_add(1, Ordering::Relaxed)).cloned(), cancel, progress)
}

/// [`analyze_all`] over tracks `next` hands out until it has none.
fn analyze_from(
    app: &AppHandle,
    next: impl Fn() -> Option<library::Track> + Sync,
    cancel: &std::sync::atomic::AtomicBool,
    progress: impl Fn(usize, &str) + Sync,
) -> (usize, Vec<String>, bool) {
    let Ok(dir) = analysis_dir(app) else {
        return (0, vec![i18n::t!("app.analysis.noDataFolder")], false);
    };
    let tracker = beat_tracker(app);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let errors = Mutex::new(Vec::new());
    let budget = AudioBudget::default();
    // Decoding, waveforms and key in parallel; the beat models take turns
    // when there are fewer of them than workers.
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2).clamp(1, 4));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Some(t) = next() else { break };
                let name = Path::new(&t.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // The library's length; a track without one counts as an hour.
                let held = budget.take(if t.duration > 0.0 { t.duration } else { 3600.0 });
                let result = analyzer::analyze(Path::new(&t.path), (t.bpm > 0.0).then_some(t.bpm), tracker)
                    .and_then(|a| {
                        let dat = analyzer::write(&a, &t.path, &dir.join(format!("{:016x}", path_hash(&t.path))))?;
                        Ok((a, dat))
                    })
                    .map_err(|e| e.to_string())
                    .and_then(|(a, dat)| {
                        lib(app, |l| {
                            l.set_analysis(t.id, &dat.display().to_string(), a.grid.map(|g| g.bpm), a.key, a.duration)
                        })
                    });
                budget.give_back(held);
                if let Err(e) = result {
                    // A sync does not try it again until the file changes.
                    let _ = lib(app, |l| l.set_analysis_failed(t.id, file_mtime(Path::new(&t.path))));
                    errors.lock().unwrap().push(format!("{name}: {e}"));
                }
                progress(done.fetch_add(1, Ordering::Relaxed) + 1, &name);
            });
        }
    });
    (done.into_inner(), errors.into_inner().unwrap(), tracker.is_some())
}

/// Decoded audio the analysis workers may hold at once, as seconds of audio
/// (mono f32 at 44.1 kHz is 176 kB/s; 2 GB is about 3 hours).
const AUDIO_BUDGET_SECS: f64 = 3.0 * 3600.0;

/// Keeps long mixes from being decoded side by side and filling the memory.
#[derive(Default)]
struct AudioBudget {
    used: Mutex<f64>,
    freed: std::sync::Condvar,
}

impl AudioBudget {
    /// Wait for room for `secs` of audio. A track over the whole budget runs
    /// once nothing else does.
    fn take(&self, secs: f64) -> f64 {
        let secs = secs.max(0.0);
        let mut used = self.used.lock().unwrap();
        while *used > 0.0 && *used + secs > AUDIO_BUDGET_SECS {
            used = self.freed.wait(used).unwrap();
        }
        *used += secs;
        secs
    }

    fn give_back(&self, secs: f64) {
        let mut used = self.used.lock().unwrap();
        *used = (*used - secs).max(0.0);
        self.freed.notify_all();
    }
}

/// The running analysis job, which tracks asked for later join.
#[derive(Default)]
pub struct AnalyzeQueue(Mutex<Option<Analysis>>);

struct Analysis {
    id: u32,
    todo: std::collections::VecDeque<library::Track>,
    /// Every track this job has had, so one asked for twice runs once.
    ids: HashSet<i64>,
    done: usize,
    event: dto::JobEvent,
}

impl Analysis {
    fn add(&mut self, tracks: Vec<library::Track>) {
        for t in tracks {
            if self.ids.insert(t.id) {
                self.todo.push_back(t);
            }
        }
        self.event.files_total = self.ids.len();
        self.event.bytes_total = self.ids.len().max(1) as u64;
    }
}

/// Analyse tracks that have no analysis (waveforms, beat grid, key), a few
/// at a time, as a background job. The files go to the app's data folder.
/// While a job runs, more tracks join it: one job, one progress over all.
#[tauri::command]
pub fn library_analyze(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    queue: State<'_, AnalyzeQueue>,
    track_ids: Vec<i64>,
    again: bool,
) -> CmdResult<u32> {
    let todo = needs_analysis(&app, track_ids, again)?;
    let mut running = queue.0.lock().unwrap();
    if let Some(a) = running.as_mut() {
        a.add(todo);
        emit_job(&app, &a.event);
        return Ok(a.id);
    }
    let (id, cancel) = jobs.start();
    let mut a = Analysis {
        id,
        todo: Default::default(),
        ids: HashSet::new(),
        done: 0,
        event: dto::JobEvent { id, label: i18n::t!("app.analysis.label"), state: "running", ..Default::default() },
    };
    a.add(todo);
    emit_job(&app, &a.event);
    *running = Some(a);
    drop(running);
    std::thread::spawn(move || {
        let queue = app.state::<AnalyzeQueue>();
        let (mut errors, mut model) = (Vec::new(), false);
        let finished = loop {
            let (_, e, m) = analyze_from(
                &app,
                || queue.0.lock().unwrap().as_mut()?.todo.pop_front(),
                &cancel,
                |_, name| {
                    if let Some(a) = queue.0.lock().unwrap().as_mut() {
                        a.done += 1;
                        a.event.files_done = a.done;
                        a.event.bytes_done = a.done as u64;
                        a.event.current = name.to_string();
                        emit_job(&app, &a.event);
                    }
                },
            );
            errors.extend(e);
            model |= m;
            // Tracks that joined after the workers ran dry: another round.
            let mut running = queue.0.lock().unwrap();
            if cancel.load(Ordering::Relaxed) || running.as_ref().is_none_or(|a| a.todo.is_empty()) {
                break running.take();
            }
        };
        let Some(Analysis { done: n, mut event, .. }) = finished else { return };
        event.files_done = n;
        event.bytes_done = event.bytes_total;
        (event.state, event.message) = if cancel.load(Ordering::Relaxed) {
            ("cancelled", i18n::t!("app.analysis.stopped", count = n))
        } else if errors.is_empty() {
            (
                "done",
                if model {
                    i18n::t!("app.analysis.done", count = n)
                } else {
                    i18n::t!("app.analysis.doneNoModel", count = n)
                },
            )
        } else {
            ("error", i18n::t!("app.analysis.failed", count = errors.len(), errors = errors.join("; ")))
        };
        emit_job(&app, &event);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

/// Beat This!, loaded on first use from the app's resources (in a dev
/// build from `src-tauri/models`). `None` when the models are missing: the
/// built-in detector takes over.
pub(crate) fn beat_tracker(app: &AppHandle) -> Option<&'static analyzer::tracker::Tracker> {
    static TRACKER: std::sync::OnceLock<Option<analyzer::tracker::Tracker>> = std::sync::OnceLock::new();
    TRACKER
        .get_or_init(|| {
            let dirs = [
                app.path().resource_dir().ok().map(|d| d.join("models")),
                Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")),
            ];
            dirs.into_iter().flatten().find_map(|d| {
                // Past two or three models they only get in each other's way.
                let models = std::thread::available_parallelism().map_or(1, |n| (n.get() / 3).clamp(1, 3));
                analyzer::tracker::Tracker::load_many(&d.join("mel_spectrogram.onnx"), &d.join("beat_this_small.onnx"), models).ok()
            })
        })
        .as_ref()
}

/// FNV-1a of a path: the folder name for its analysis files.
fn path_hash(path: &str) -> u64 {
    path.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[tauri::command]
pub fn library_ai_analyze(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    track_ids: Vec<i64>,
) -> CmdResult<u32> {
    let todo: Vec<(String, PathBuf)> = {
        let sc = app.state::<Sidecar>();
        track_ids
            .iter()
            .filter_map(|&id| track(&app, id).ok())
            .map(|t| (ai_key(&t), PathBuf::from(&t.path)))
            .filter(|(k, p)| sc.cached(k).is_none() && p.is_file())
            .collect()
    };
    let (id, cancel) = jobs.start();
    let total = todo.len();
    std::thread::spawn(move || {
        let mut ev = dto::JobEvent {
            id,
            label: i18n::t!("app.ai.label"),
            state: "running",
            files_done: 0,
            files_total: total,
            bytes_done: 0,
            bytes_total: total as u64,
            current: String::new(),
            message: String::new(),
            dest: String::new(),
            ..Default::default()
        };
        emit_job(&app, &ev);
        let result = crate::sidecar::ensure_installed(&app).and_then(|()| {
            app.state::<Sidecar>().run(&todo, &cancel, |done, name| {
                let mut e = ev.clone();
                e.files_done = done;
                e.bytes_done = done as u64;
                e.current = name.to_string();
                emit_job(&app, &e);
            })
        });
        ev.files_done = total;
        ev.bytes_done = total as u64;
        match result {
            Ok((done, errors)) => {
                ev.state = if cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else if errors.is_empty() {
                    "done"
                } else {
                    "error"
                };
                ev.message = if errors.is_empty() {
                    i18n::t!("app.ai.done", count = done)
                } else {
                    i18n::t!("app.analysis.failed", count = errors.len(), errors = errors.join("; "))
                };
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e;
            }
        }
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

/// Selected playlists with the folders above them and everything inside
/// selected folders, plus their tracks. `flat`: no folders on the stick,
/// every playlist at the top in the order the tree shows them.
fn sync_request(
    app: &AppHandle,
    stick: &str,
    playlist_ids: &[i64],
    flat: bool,
) -> CmdResult<SyncRequest> {
    let (all, tracks) = lib(app, |l| Ok((l.playlists()?, l.tracks()?)))?;
    let by_id: HashMap<i64, &Playlist> = all.iter().map(|p| (p.id, p)).collect();
    let mut chosen: HashSet<i64> = HashSet::new();
    let mut stack: Vec<i64> = playlist_ids.to_vec();
    while let Some(id) = stack.pop() {
        if chosen.insert(id) {
            stack.extend(all.iter().filter(|p| p.parent_id == id).map(|p| p.id));
        }
    }
    for &id in playlist_ids {
        let mut p = by_id.get(&id).map(|p| p.parent_id).unwrap_or(0);
        while p != 0 && chosen.insert(p) {
            p = by_id.get(&p).map(|x| x.parent_id).unwrap_or(0);
        }
    }
    let lists: Vec<&Playlist> = all.iter().filter(|p| chosen.contains(&p.id)).collect();
    let wanted: HashSet<i64> = lists
        .iter()
        .flat_map(|p| p.track_ids.iter().copied())
        .collect();
    let mut sync_tracks = Vec::new();
    for t in tracks.into_iter().filter(|t| wanted.contains(&t.id)) {
        let cues = lib(app, |l| l.cues(t.id))?;
        let grid = edited_grid(app, t.id).map(|g| {
            let tempo = (g.bpm * 100.0).round() as u16;
            g.beats(track_ms(&t))
                .into_iter()
                .map(|(time_ms, beat_number)| pioneer_formats::anlz::Beat { beat_number, tempo, time_ms })
                .collect()
        });
        sync_tracks.push(SyncTrack {
            grid,
            key: t.id,
            audio: PathBuf::from(&t.path),
            title: t.title,
            artist: t.artist,
            album: t.album,
            genre: t.genre,
            label: t.label,
            remixer: t.remixer,
            key_name: t.key,
            bpm: t.bpm,
            duration: t.duration,
            bitrate: t.bitrate,
            sample_rate: t.sample_rate,
            sample_depth: t.sample_depth,
            rating: t.rating,
            color: t.color,
            comment: t.comment,
            year: t.year,
            track_number: t.track_number,
            date_added: t.date_added,
            anlz_dat: (!t.anlz.is_empty()).then(|| PathBuf::from(t.anlz)),
            cues,
        });
    }
    let name = Path::new(stick)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut playlists: Vec<SyncPlaylist> = lists
        .iter()
        .map(|p| SyncPlaylist {
            key: p.id,
            parent: if chosen.contains(&p.parent_id) {
                p.parent_id
            } else {
                0
            },
            name: p.name.clone(),
            is_folder: p.is_folder,
            sort: p.sort_order,
            tracks: p.track_ids.clone(),
        })
        .collect();
    if flat {
        playlists = flatten(playlists);
    }
    Ok(SyncRequest {
        stick: PathBuf::from(stick),
        tracks: sync_tracks,
        playlists,
        device_name: name,
        date: timestamp()[..10].to_string(),
        settings: None,
        engine: None,
    })
}

/// Folders dissolved: their playlists move to the top, in tree order. A
/// name that would repeat gets its folder's name in brackets.
fn flatten(lists: Vec<SyncPlaylist>) -> Vec<SyncPlaylist> {
    fn walk<'a>(all: &'a [SyncPlaylist], parent: i64, folder: &str, out: &mut Vec<(&'a SyncPlaylist, String)>) {
        let mut kids: Vec<&SyncPlaylist> = all.iter().filter(|p| p.parent == parent).collect();
        kids.sort_by_key(|p| (p.sort, p.key));
        for p in kids {
            if p.is_folder {
                walk(all, p.key, &p.name, out);
            } else {
                out.push((p, folder.to_string()));
            }
        }
    }
    let mut order = Vec::new();
    walk(&lists, 0, "", &mut order);
    let mut count: HashMap<&str, usize> = HashMap::new();
    for (p, _) in &order {
        *count.entry(p.name.as_str()).or_default() += 1;
    }
    order
        .iter()
        .enumerate()
        .map(|(i, (p, folder))| SyncPlaylist {
            parent: 0,
            sort: i as i64,
            name: if count[p.name.as_str()] > 1 && !folder.is_empty() {
                format!("{} ({folder})", p.name)
            } else {
                p.name.clone()
            },
            ..(*p).clone()
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlanDto {
    pub tracks: usize,
    pub playlists: usize,
    pub copy_files: usize,
    pub copy_bytes: u64,
    pub reuse_files: usize,
    pub free_bytes: u64,
    pub missing_audio: Vec<String>,
    pub no_analysis: Vec<String>,
}

fn selection_key(stick: &str) -> String {
    format!(
        "sync:{}",
        Path::new(stick)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    )
}

/// Playlists last synced to this stick.
#[tauri::command]
pub fn sync_selection(app: AppHandle, stick: String) -> CmdResult<Vec<i64>> {
    let v = lib(&app, |l| l.meta(&selection_key(&stick)))?;
    Ok(v.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn sync_plan(
    app: AppHandle,
    stick: String,
    playlist_ids: Vec<i64>,
) -> CmdResult<SyncPlanDto> {
    blocking(move || {
        let req = sync_request(&app, &stick, &playlist_ids, false)?;
        let p = sync::plan(&req);
        Ok(SyncPlanDto {
            tracks: p.tracks.len(),
            playlists: req.playlists.iter().filter(|p| !p.is_folder).count(),
            copy_files: p.copy_files,
            copy_bytes: p.copy_bytes,
            reuse_files: p.reuse_files,
            free_bytes: p.free_bytes,
            missing_audio: p.missing_audio,
            no_analysis: p.no_analysis,
        })
    })
    .await
}

/// Sync phases and the keys of their names.
const PHASES: [(&str, &str); 8] = [
    ("analyze", "app.sync.phase.analyze"),
    ("backup", "app.sync.phase.backup"),
    ("copy", "app.sync.phase.copy"),
    ("analysis", "app.sync.phase.analysis"),
    ("database", "app.sync.phase.database"),
    ("settings", "app.sync.phase.settings"),
    ("engine", "app.sync.phase.engine"),
    ("verify", "app.sync.phase.verify"),
];

#[tauri::command]
pub fn sync_start(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    stick: String,
    playlist_ids: Vec<i64>,
    write_settings: bool,
    engine: Option<String>,
    flat: bool,
) -> CmdResult<u32> {
    let settings = if write_settings {
        Some(crate::player_cmds::profile(&app)?.0)
    } else {
        None
    };
    let engine = match &engine {
        Some(e) => Some(engine_library::Schema::parse(e).ok_or_else(|| i18n::t!("app.sync.unknownEngine"))?),
        None => None,
    };
    let request = {
        let app = app.clone();
        let stick = stick.clone();
        let playlist_ids = playlist_ids.clone();
        move || -> CmdResult<SyncRequest> {
            let mut req = sync_request(&app, &stick, &playlist_ids, flat)?;
            req.settings = settings.clone();
            req.engine = engine;
            Ok(req)
        }
    };
    let mut req = request()?;
    lib(&app, |l| {
        l.set_meta(
            &selection_key(&stick),
            &serde_json::to_string(&playlist_ids).unwrap_or_default(),
        )
    })?;
    let (id, cancel) = jobs.start();
    std::thread::spawn(move || {
        let name = req.device_name.clone();
        let mut plan = sync::plan(&req);
        // Tracks without analysis are analysed first, so the players get
        // their waveforms and grids. Not those whose analysis is on the stick
        // already, nor those that failed before and did not change since.
        let on_stick: HashSet<i64> =
            plan.tracks.iter().filter(|pt| pt.stick_anlz.is_some()).map(|pt| pt.key).collect();
        let unanalysed: Vec<library::Track> = needs_analysis(
            &app,
            req.tracks
                .iter()
                .filter(|t| t.anlz_dat.as_ref().is_none_or(|p| !p.is_file()) && !on_stick.contains(&t.key))
                .map(|t| t.key),
            false,
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|t| !lib(&app, |l| l.analysis_failed(t.id, file_mtime(Path::new(&t.path)))).unwrap_or(false))
        .collect();
        // Share of the whole bar for the analysis: ~3.5 s a track against
        // copying at ~25 MB/s.
        let n = unanalysed.len();
        let copy_secs = plan.copy_bytes as f64 / 25e6 + 15.0;
        let w = if n == 0 { 0.0 } else { n as f64 * 3.5 / (n as f64 * 3.5 + copy_secs) };
        let mut analysed = String::new();
        if n > 0 {
            let ev = dto::JobEvent {
                id,
                label: format!("Sync {name}"),
                state: "running",
                files_total: n,
                bytes_total: n as u64,
                dest: req.stick.display().to_string(),
                phase: "analyze".into(),
                fraction: Some(0.0),
                current: i18n::t!(PHASES[0].1),
                ..Default::default()
            };
            emit_job(&app, &ev);
            let (_, errors, _) = analyze_all(&app, &unanalysed, &cancel, |k, track| {
                let mut e = ev.clone();
                e.files_done = k;
                e.bytes_done = k as u64;
                e.fraction = Some(w * k as f64 / n as f64);
                e.current = format!("{}: {track}", i18n::t!(PHASES[0].1));
                emit_job(&app, &e);
            });
            // The library now points at the new analysis files.
            match request() {
                Ok(r) => {
                    req = r;
                    plan = sync::plan(&req);
                }
                Err(e) => {
                    let mut e2 = ev.clone();
                    e2.state = "error";
                    e2.message = e;
                    emit_job(&app, &e2);
                    app.state::<Jobs>().finish(id);
                    return;
                }
            }
            analysed = format!(" {}", i18n::t!("app.sync.analysedFirst", count = n - errors.len()));
            if !errors.is_empty() {
                analysed += &format!(" {}", i18n::t!("app.sync.leftUnanalysed", count = errors.len(), errors = errors.join("; ")));
            }
        }
        let mut ev = dto::JobEvent {
            id,
            label: format!("Sync {name}"),
            state: "running",
            files_done: 0,
            files_total: plan.copy_files,
            bytes_done: 0,
            bytes_total: plan.copy_bytes.max(1),
            current: String::new(),
            message: String::new(),
            dest: req.stick.display().to_string(),
            phase: "backup".into(),
            fraction: Some(w),
        };
        emit_job(&app, &ev);
        let home = std::env::var("HOME").unwrap_or_default();
        let backup = PathBuf::from(home)
            .join("Stickwise/backups")
            .join(&name)
            .join(format!("sync-{}", timestamp()));
        let result = sync::execute(&req, &plan, &backup, &cancel, |p| {
            let mut e = ev.clone();
            e.files_done = p.done;
            e.files_total = p.total;
            e.bytes_done = p.bytes_done;
            e.bytes_total = p.bytes_total.max(1);
            e.phase = p.phase.to_string();
            e.fraction = Some(w + (1.0 - w) * p.fraction);
            let phase = PHASES
                .iter()
                .find(|x| x.0 == p.phase)
                .map_or_else(String::new, |x| i18n::t!(x.1));
            e.current = if p.current.is_empty() {
                phase.to_string()
            } else {
                format!("{phase}: {}", p.current)
            };
            emit_job(&app, &e);
        });
        match result {
            Ok(r) if r.cancelled => {
                ev.state = "cancelled";
                ev.message =
                    i18n::t!("app.sync.stopped");
            }
            Ok(r) => {
                // Written: what could not go along is told, not a failure.
                ev.state = "done";
                ev.files_done = ev.files_total;
                ev.bytes_done = ev.bytes_total;
                ev.fraction = Some(1.0);
                let mut msg = i18n::t!(
                    "app.sync.onStick",
                    count = r.tracks,
                    playlists = r.playlists,
                    copied = r.copied,
                    reused = r.reused
                );
                msg += " ";
                msg += &i18n::t!("app.sync.anlz", written = r.anlz_written, unchanged = r.anlz_unchanged);
                if !r.skipped.is_empty() {
                    msg += " ";
                    msg += &i18n::t!("app.sync.skipped", count = r.skipped.len(), tracks = r.skipped.join("; "));
                }
                msg += &analysed;
                if r.settings_written {
                    msg += " ";
                    msg += &i18n::t!("app.sync.settingsOnStick");
                }
                if let Some(e) = &r.engine {
                    let schema = e.schema.unwrap_or(engine_library::Schema::DEFAULT);
                    msg += " ";
                    msg += &i18n::t!(
                        "app.sync.denon",
                        firmware = schema.min_firmware(),
                        count = e.tracks,
                        playlists = e.playlists
                    );
                    if e.dropped_cues > 0 {
                        msg += " ";
                        msg += &i18n::t!("app.sync.droppedCues", count = e.dropped_cues);
                    }
                }
                if !r.moved_aside.is_empty() {
                    msg += " ";
                    msg += &i18n::t!("app.sync.movedAside", files = r.moved_aside.join(", "));
                }
                if !r.warnings.is_empty() {
                    msg += " ";
                    msg += &i18n::t!("app.sync.warnings", warnings = r.warnings.join("; "));
                }
                ev.message = msg;
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e.to_string();
            }
        }
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

// ---- Set Planner -------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTrackDto {
    pub track_id: i64,
    /// Start in the set, seconds.
    pub start: f64,
    pub energy: f64,
    pub target: f64,
    pub bpm: f64,
    pub key: String,
    pub mix_in_ms: u32,
    pub mix_out_ms: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionDto {
    pub from: i64,
    pub to: i64,
    pub key_relation: String,
    pub key_ok: bool,
    pub pitch_percent: f64,
    pub tempo_ok: bool,
    pub bars: u32,
    pub start_at_ms: u32,
    pub cue_in_ms: u32,
    pub energy_step: f64,
    /// `long-blend`, `breakdown-blend`, `drop-swap`, `double-drop`, `echo-out`.
    pub technique: String,
    pub technique_name: String,
    /// Cues for the outgoing and incoming track: (ms, label).
    pub a_marks: Vec<(u32, String)>,
    pub b_marks: Vec<(u32, String)>,
    pub tips: Vec<String>,
    pub cost: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPlanDto {
    pub tracks: Vec<PlannedTrackDto>,
    pub left_out: Vec<i64>,
    pub transitions: Vec<TransitionDto>,
    pub total_seconds: f64,
}

/// What the planner needs about one library track.
fn plan_track(app: &AppHandle, t: &library::Track) -> analysis::set_planner::PlanTrack {
    use analysis::smart_cue::{Kind, bands_from_color, bands_from_three_band};
    use pioneer_formats::anlz::tag;
    let duration_ms = (t.duration * 1000.0) as u32;
    let bar_ms = (4.0 * 60000.0 / t.bpm.max(60.0)) as u32;
    let mut mix_in = 0;
    let mut mix_out = duration_ms.saturating_sub(32 * bar_ms);
    let mut energy = 5.0;
    let (mut drops, mut breaks, mut builds) = (Vec::new(), Vec::new(), Vec::new());
    if !t.anlz.is_empty() {
        let dat = Path::new(&t.anlz);
        if let Some(s) = suggestions_for(app, dat, t.bpm as f32, &ai_key(t), edited_beats(app, t).as_deref()) {
            let at = |k: Kind| {
                s.iter()
                    .filter(|x| x.kind == k)
                    .map(|x| x.time_ms)
                    .collect::<Vec<u32>>()
            };
            drops = at(Kind::Drop);
            breaks = at(Kind::Break);
            builds = at(Kind::Build);
            if let Some(m) = s.iter().find(|x| x.kind == Kind::MixIn) {
                mix_in = m.time_ms;
            }
            if let Some(m) = s
                .iter()
                .find(|x| x.kind == Kind::Outro)
                .or_else(|| s.iter().find(|x| x.kind == Kind::MixOut))
            {
                mix_out = m.time_ms;
            }
        }
        let columns = crate::commands::read_anlz(&crate::commands::sibling(dat, "2EX"))
            .and_then(|f| {
                f.section(tag::WAVE_3BAND_SCROLL)
                    .map(|s| bands_from_three_band(&s.body))
            })
            .or_else(|| {
                crate::commands::read_anlz(&crate::commands::sibling(dat, "EXT")).and_then(|f| {
                    f.section(tag::WAVE_COLOR_SCROLL)
                        .map(|s| bands_from_color(&s.body))
                })
            });
        if let Some(c) = columns {
            energy = analysis::set_planner::energy(&c, 150.0, mix_in, mix_out, t.bpm);
        }
    }
    analysis::set_planner::PlanTrack {
        id: t.id,
        bpm: t.bpm,
        camelot: analysis::set_planner::camelot(&t.key),
        energy,
        duration: t.duration,
        mix_out_ms: mix_out
            .max(mix_in + 30_000)
            .min(duration_ms.max(mix_in + 30_000)),
        mix_in_ms: mix_in,
        drops,
        breaks,
        builds,
    }
}

#[tauri::command]
pub async fn plan_set(
    app: AppHandle,
    set_id: i64,
    minutes: f64,
    level: String,
    curve: String,
    keep_order: bool,
) -> CmdResult<SetPlanDto> {
    use analysis::set_planner::{Curve, Level, Params, plan, plan_in_order};
    blocking(move || {
        let (lists, tracks) = lib(&app, |l| Ok((l.playlists()?, l.tracks()?)))?;
        let set = lists
            .iter()
            .find(|p| p.id == set_id)
            .ok_or("set nenalezen")?;
        let by_id: HashMap<i64, &library::Track> = tracks.iter().map(|t| (t.id, t)).collect();
        let mut seen = HashSet::new();
        let pool: Vec<analysis::set_planner::PlanTrack> = set
            .track_ids
            .iter()
            .filter(|id| seen.insert(**id))
            .filter_map(|id| by_id.get(id))
            .filter(|t| Path::new(&t.path).is_file())
            .map(|t| plan_track(&app, t))
            .collect();
        if pool.len() < 2 {
            return Err(i18n::t!("app.library.setTooSmall"));
        }
        let params = Params {
            minutes,
            level: match level.as_str() {
                "beginner" => Level::Beginner,
                "pro" => Level::Pro,
                _ => Level::Advanced,
            },
            curve: match curve.as_str() {
                "warmup" => Curve::WarmUp,
                "closing" => Curve::Closing,
                "journey" => Curve::Journey,
                _ => Curve::PeakTime,
            },
        };
        let p = if keep_order {
            plan_in_order(&pool, &params)
        } else {
            plan(&pool, &params)
        };
        let feat: HashMap<i64, &analysis::set_planner::PlanTrack> =
            pool.iter().map(|t| (t.id, t)).collect();
        Ok(SetPlanDto {
            tracks: p
                .order
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let f = feat[id];
                    PlannedTrackDto {
                        track_id: *id,
                        start: p.starts[i],
                        energy: (f.energy * 10.0).round() / 10.0,
                        target: (p.target[i] * 10.0).round() / 10.0,
                        bpm: f.bpm,
                        key: by_id.get(id).map(|t| t.key.clone()).unwrap_or_default(),
                        mix_in_ms: f.mix_in_ms,
                        mix_out_ms: f.mix_out_ms,
                    }
                })
                .collect(),
            left_out: p.left_out,
            transitions: p
                .transitions
                .into_iter()
                .map(|t| TransitionDto {
                    from: t.from,
                    to: t.to,
                    key_relation: t.key_relation.to_string(),
                    key_ok: t.key_ok,
                    pitch_percent: (t.pitch_percent * 10.0).round() / 10.0,
                    tempo_ok: t.tempo_ok,
                    bars: t.bars,
                    start_at_ms: t.start_at_ms,
                    cue_in_ms: t.cue_in_ms,
                    energy_step: (t.energy_step * 10.0).round() / 10.0,
                    technique: t.technique.id().into(),
                    technique_name: t.technique.name(),
                    // Labels that point at the next track get its title.
                    a_marks: t
                        .a_marks
                        .iter()
                        .map(|m| {
                            let next = by_id.get(&t.to).map(|x| x.title.as_str()).unwrap_or("");
                            // Cue labels are English in every app language: they
                            // end up on the player, next to Mix in, Drop and the like.
                            let label = if m.label.ends_with('→') || m.label.ends_with(": play") {
                                format!("{} {next}", m.label)
                            } else {
                                m.label.clone()
                            };
                            (m.time_ms, label)
                        })
                        .collect(),
                    b_marks: t
                        .b_marks
                        .iter()
                        .map(|m| (m.time_ms, m.label.clone()))
                        .collect(),
                    tips: t.tips,
                    cost: t.cost,
                })
                .collect(),
            total_seconds: p.total_seconds,
        })
    })
    .await
}

/// Pad colour for a transition cue, by what it marks.
fn mark_color(label: &str) -> u8 {
    let l = label.to_lowercase();
    if l.starts_with("echo") {
        0x3c // purple: echo out
    } else if l.contains('→') || l.contains(": play") {
        0x2d // pink: hand over to the next track
    } else if l.starts_with("mix in") || l.starts_with("start") {
        0x16 // green: start here
    } else if l.contains("double") {
        0x05 // blue
    } else if l.starts_with("drop") {
        0x2a // red
    } else if l.contains("build") {
        0x22 // orange
    } else {
        0x2d
    }
}

/// Put the planned order into the set (tracks left out go to the end) and
/// write each track's transition cues. `marks` holds, per planned track,
/// the cues its transitions need (ms, label). Modes:
/// * `only`: the track keeps just these cues as hot cues A, B, … in time
///   order (its other hot cues are backed up and removed; memory cues stay);
/// * `hot`: added on the highest free pads (H, G, …);
/// * `memory`: memory cues.
///
/// Cues an earlier plan wrote (remembered per track) are always replaced.
#[tauri::command]
pub async fn apply_set_plan(
    app: AppHandle,
    set_id: i64,
    order: Vec<i64>,
    left_out: Vec<i64>,
    marks: Vec<(i64, Vec<(u32, String)>)>,
    mode: String,
) -> CmdResult<()> {
    blocking(move || {
        let mut ids = order.clone();
        ids.extend(left_out);
        lib(&app, |l| l.set_playlist_tracks(set_id, &ids))?;
        for (id, mut add) in marks {
            add.sort_by_key(|m| m.0);
            add.dedup_by_key(|m| m.0 / 50);
            let key = format!("plancues:{id}");
            let previous: Vec<(u8, u32, String)> = lib(&app, |l| l.meta(&key))?
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default();
            let mut cues = lib(&app, |l| l.cues(id))?;
            cues.retain(|c| {
                !previous
                    .iter()
                    .any(|p| p.0 == c.slot && p.1 == c.time_ms && p.2 == c.comment)
            });
            // Cues from the first planner version, which did not remember what it wrote.
            cues.retain(|c| {
                !(c.comment == "Mix in"
                    || c.comment.starts_with("Mix out →")
                    || c.comment == "Mix out (konec setu)")
            });
            let mut written = Vec::new();
            if mode == "only" {
                lib(&app, |l| l.backup_cues(id))?;
                cues.retain(|c| c.slot == 0);
            }
            for (time, label) in add {
                let slot = match mode.as_str() {
                    "only" => (1..=8u8).find(|s| !cues.iter().any(|c| c.slot == *s)),
                    "hot" => (1..=8u8).rev().find(|s| !cues.iter().any(|c| c.slot == *s)),
                    _ => None,
                };
                let cue = match slot {
                    Some(s) => CuePoint {
                        comment: label.clone(),
                        hot_color: mark_color(&label),
                        ..CuePoint::hot(s, time)
                    },
                    None => CuePoint {
                        comment: label.clone(),
                        ..CuePoint::memory(time)
                    },
                };
                written.push((cue.slot, cue.time_ms, cue.comment.clone()));
                cues.push(cue);
            }
            lib(&app, |l| {
                l.set_cues(id, &cues)?;
                l.set_meta(&key, &serde_json::to_string(&written).unwrap_or_default())
            })?;
        }
        Ok(())
    })
    .await
}

/// How many tracks of a set have cues backed up by a plan.
#[tauri::command]
pub fn set_cue_backups(app: AppHandle, set_id: i64) -> CmdResult<usize> {
    lib(&app, |l| {
        let ids: HashSet<i64> = l
            .playlists()?
            .into_iter()
            .find(|p| p.id == set_id)
            .map(|p| p.track_ids.into_iter().collect())
            .unwrap_or_default();
        let mut n = 0;
        for id in ids {
            if l.has_cue_backup(id)? {
                n += 1;
            }
        }
        Ok(n)
    })
}

/// Give the tracks of a set back the cues they had before a plan replaced them.
#[tauri::command]
pub fn restore_set_cues(app: AppHandle, set_id: i64) -> CmdResult<usize> {
    lib(&app, |l| {
        let ids: HashSet<i64> = l
            .playlists()?
            .into_iter()
            .find(|p| p.id == set_id)
            .map(|p| p.track_ids.into_iter().collect())
            .unwrap_or_default();
        let mut n = 0;
        for id in ids {
            if l.restore_cues(id)? {
                n += 1;
            }
        }
        Ok(n)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// `3.0.2`, when the stick has a Denon library.
    pub version: Option<String>,
    /// The oldest Engine OS that reads it; `None` for an unknown schema.
    pub min_firmware: Option<String>,
    pub tracks: usize,
}

/// The Denon library a stick carries, if any.
#[tauri::command]
pub async fn engine_status(stick: String) -> CmdResult<EngineStatus> {
    blocking(move || {
        let db = Path::new(&stick)
            .join(usb_tools::engine::DB_DIR)
            .join("m.db");
        if !db.is_file() {
            return Ok(EngineStatus {
                version: None,
                min_firmware: None,
                tracks: 0,
            });
        }
        let ((a, b, c), _) = engine_library::info(&db).map_err(err)?;
        let tracks = engine_library::read(&db)
            .map(|l| l.tracks.len())
            .unwrap_or(0);
        Ok(EngineStatus {
            version: Some(format!("{a}.{b}.{c}")),
            min_firmware: engine_library::Schema::from_version((a, b, c))
                .map(|s| s.min_firmware().to_string()),
            tracks,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(key: i64, parent: i64, name: &str, folder: bool, sort: i64) -> SyncPlaylist {
        SyncPlaylist {
            key,
            parent,
            name: name.into(),
            is_folder: folder,
            sort,
            tracks: vec![key],
        }
    }

    #[test]
    fn flatten_keeps_tree_order_and_names_repeats() {
        let out = flatten(vec![
            pl(1, 0, "16.2.2026", false, 2),
            pl(2, 0, "Imported Playlists", true, 0),
            pl(3, 2, "16.2.2026", false, 1),
            pl(4, 2, "SET 23.1.2026", false, 0),
            pl(5, 0, "CHATA", false, 1),
        ]);
        let names: Vec<_> = out.iter().map(|p| (p.name.as_str(), p.parent, p.sort)).collect();
        assert_eq!(
            names,
            vec![
                ("SET 23.1.2026", 0, 0),
                ("16.2.2026 (Imported Playlists)", 0, 1),
                ("CHATA", 0, 2),
                ("16.2.2026", 0, 3),
            ]
        );
        assert!(out.iter().all(|p| !p.is_folder && p.tracks == vec![p.key]));
    }
}
