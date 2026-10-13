use std::path::{Path, PathBuf};

use pioneer_formats::pdb::{Database, Track};

#[derive(Debug, thiserror::Error)]
pub enum StickError {
    #[error("{0} has no PIONEER/rekordbox/export.pdb — not a rekordbox USB stick?")]
    NoLibrary(PathBuf),
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("export.pdb is unreadable: {0}")]
    Format(#[from] pioneer_formats::Error),
}

/// A mounted stick and its decoded library.
pub struct Stick {
    pub root: PathBuf,
    pub db: Database,
}

impl Stick {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StickError> {
        let root = root.into();
        let pdb = root.join("PIONEER/rekordbox/export.pdb");
        if !pdb.is_file() {
            return Err(StickError::NoLibrary(root));
        }
        let bytes = std::fs::read(&pdb).map_err(|source| StickError::Io {
            path: pdb.clone(),
            source,
        })?;
        Ok(Stick {
            db: Database::parse(&bytes)?,
            root,
        })
    }

    /// Absolute path of a device path such as `/Contents/A/B.mp3`.
    pub fn resolve(&self, device_path: &str) -> PathBuf {
        self.root.join(device_path.trim_start_matches('/'))
    }

    pub fn audio_path(&self, track: &Track) -> PathBuf {
        self.resolve(&track.file_path)
    }

    pub fn exists(&self, track: &Track) -> bool {
        Path::is_file(&self.audio_path(track))
    }
}
