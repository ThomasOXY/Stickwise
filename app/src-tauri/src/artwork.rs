//! Artwork for library tracks that have none (added from folders, or without
//! a picture in rekordbox) from the picture embedded in the audio file.
//!
//! Runs in the background whenever the library is loaded. Each picture is
//! kept under `<app data>/artwork/<hash>/` the way rekordbox keeps its own:
//! `artwork.<ext>` as embedded, `artwork_m.jpg` (240 px) and `artwork_s.jpg`
//! (80 px) beside it, so a sync takes the thumbnails as they are.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};
use usb_tools::artwork::{self, Source};

use crate::library_cmds::lib;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Files that had no picture, by track id: their source fingerprint, so an
/// unchanged file is not read again.
const NONE_KEY: &str = "artwork_none";

/// How often the table is told to reload while pictures come in.
const RELOAD_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// Start filling in artwork, unless that is running already.
pub fn fill_in_background(app: &AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = fill(&app);
        RUNNING.store(false, Ordering::SeqCst);
    });
}

fn fill(app: &AppHandle) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("artwork");
    let (tracks, none) = lib(app, |l| Ok((l.tracks()?, l.meta(NONE_KEY)?)))?;
    let mut none: HashMap<i64, u64> = none.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let todo: Vec<_> = tracks
        .into_iter()
        .filter(|t| t.artwork.is_empty() || !Path::new(&t.artwork).is_file())
        .filter(|t| Path::new(&t.path).is_file())
        .collect();
    let mut unshown = 0;
    let mut shown = std::time::Instant::now();
    let mut checked = false;
    for t in todo {
        let src = Source::Embedded(PathBuf::from(&t.path));
        let fp = src.fingerprint();
        if none.get(&t.id) == Some(&fp) {
            continue;
        }
        checked = true;
        match save(&dir, Path::new(&t.path)) {
            Some(path) => {
                none.remove(&t.id);
                lib(app, |l| l.set_artwork(t.id, &path.display().to_string()))?;
                unshown += 1;
                if shown.elapsed() >= RELOAD_EVERY {
                    let _ = app.emit("library-changed", ());
                    (unshown, shown) = (0, std::time::Instant::now());
                }
            }
            None => {
                none.insert(t.id, fp);
            }
        }
    }
    if checked {
        let json = serde_json::to_string(&none).unwrap_or_default();
        lib(app, |l| l.set_meta(NONE_KEY, &json))?;
    }
    if unshown > 0 {
        let _ = app.emit("library-changed", ());
    }
    Ok(())
}

/// Keep the picture embedded in `audio` with its thumbnails; the path of
/// the full picture, or `None` without one.
fn save(dir: &Path, audio: &Path) -> Option<PathBuf> {
    let picture = artwork::embedded(audio)?;
    let thumbs = artwork::thumbnails(&picture)?;
    let ext = artwork::extension(&picture);
    // One folder per picture: an album's tracks share it.
    let hash = picture
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    let folder = dir.join(format!("{:03x}/{hash:016x}", hash >> 52));
    let full = folder.join(format!("artwork.{ext}"));
    if !full.is_file() {
        std::fs::create_dir_all(&folder).ok()?;
        std::fs::write(folder.join("artwork_s.jpg"), &thumbs.small).ok()?;
        std::fs::write(folder.join("artwork_m.jpg"), &thumbs.medium).ok()?;
        // Last: its presence means the folder is complete.
        std::fs::write(&full, &picture).ok()?;
    }
    Some(full)
}
