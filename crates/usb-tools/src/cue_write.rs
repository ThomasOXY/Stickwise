//! Write edited cue points into a stick's analysis files, safely:
//! back up the originals, write each file next to itself, flush it to the
//! device, swap it in with a rename, then read it back.
//!
//! Only the track's `ANLZ0000.DAT` / `.EXT` change; `export.pdb` is not
//! touched.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use pioneer_formats::anlz::{AnlzFile, CuePoint, write_cue_points};

use crate::Stick;

#[derive(Debug, Default)]
pub struct CueWriteReport {
    pub written: Vec<u32>,
    pub failed: Vec<(u32, String)>,
    /// Where the original analysis files were copied.
    pub backup_dir: PathBuf,
}

/// Flush file data all the way to the device (plain fsync on macOS only
/// reaches the drive's cache).
fn full_sync(f: &File) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: F_FULLFSYNC on a valid open descriptor.
        if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
            return Ok(());
        }
    }
    f.sync_all()
}

/// Replace `path` with `bytes` without ever leaving a half-written file.
pub fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.rbx-new"));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        full_sync(&f)?;
    }
    std::fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent()
        && let Ok(d) = File::open(dir) {
            let _ = full_sync(&d);
        }
    if std::fs::read(path)? != bytes {
        return Err(std::io::Error::other(
            "read-back after write does not match",
        ));
    }
    Ok(())
}

fn write_one(
    stick: &Stick,
    track_id: u32,
    cues: &[CuePoint],
    backup_dir: &Path,
) -> Result<(), String> {
    let t = stick
        .db
        .track(track_id)
        .ok_or("track is not on the stick")?;
    if t.analyze_path.is_empty() {
        return Err("track has no analysis file".into());
    }
    let dat_path = stick.resolve(&t.analyze_path);
    let ext_path = dat_path.with_extension("EXT");
    let dat_bytes = std::fs::read(&dat_path).map_err(|e| format!("{}: {e}", t.analyze_path))?;
    let ext_bytes = std::fs::read(&ext_path).ok();
    let mut dat = AnlzFile::parse(&dat_bytes).map_err(|e| format!("DAT: {e}"))?;
    let mut ext = match &ext_bytes {
        Some(b) => Some(AnlzFile::parse(b).map_err(|e| format!("EXT: {e}"))?),
        None => None,
    };
    write_cue_points(&mut dat, ext.as_mut(), cues);

    // Back up before touching the stick, mirroring the device path.
    let rel = t.analyze_path.trim_start_matches('/');
    let backup = backup_dir.join(rel);
    std::fs::create_dir_all(backup.parent().unwrap()).map_err(|e| format!("backup: {e}"))?;
    std::fs::write(&backup, &dat_bytes).map_err(|e| format!("backup: {e}"))?;
    if let Some(b) = &ext_bytes {
        std::fs::write(backup.with_extension("EXT"), b).map_err(|e| format!("backup: {e}"))?;
    }

    if let (Some(ext), true) = (&ext, ext_bytes.is_some()) {
        replace_file(&ext_path, &ext.to_bytes()).map_err(|e| format!("EXT: {e}"))?;
    }
    replace_file(&dat_path, &dat.to_bytes()).map_err(|e| format!("DAT: {e}"))?;
    Ok(())
}

/// Write each track's cue list; one failing track does not stop the rest.
pub fn write_cues(
    stick: &Stick,
    edits: &[(u32, Vec<CuePoint>)],
    backup_dir: &Path,
) -> CueWriteReport {
    let mut report = CueWriteReport {
        backup_dir: backup_dir.to_path_buf(),
        ..Default::default()
    };
    for (id, cues) in edits {
        match write_one(stick, *id, cues, backup_dir) {
            Ok(()) => report.written.push(*id),
            Err(e) => report.failed.push((*id, e)),
        }
    }
    report
}
