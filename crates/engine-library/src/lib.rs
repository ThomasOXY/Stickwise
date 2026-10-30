//! Denon / Engine DJ device library (`Engine Library/Database2/m.db`).
//!
//! The database is SQLite. Tracks point at audio by a path relative to the
//! `Engine Library` folder, so a stick can carry one copy of the music in
//! `/Contents` for both rekordbox (`export.pdb`) and Engine
//! (`../Contents/…`).
//!
//! Engine migrates a library to its own newest schema the first time it
//! opens it and older firmware cannot read the result, so the schema is
//! chosen by the oldest player the stick has to work on ([`Schema`]).
//!
//! References: rb2engine and PenBridge (both MIT) for the schemas and blob
//! layouts, libdjinterop (LGPL, read as documentation only), and libraries
//! Engine DJ itself wrote.

pub mod blobs;
pub mod convert;
mod read;
mod write;

pub use read::{ReadLibrary, ReadPlaylist, ReadTrack, info, read};
pub use write::{BuildReport, build};

use blobs::{BeatData, Loops, Overview, QuickCues, TrackData};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SQLite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("Engine blob: {0}")]
    Blob(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Library schema, by the oldest firmware that reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Schema {
    /// Engine OS 3.1–3.4 wrote it; 4.x and 5.x still read it.
    V2_21_0,
    /// Engine OS 4.3 and newer.
    V3_0_1,
    /// Engine OS 5.0 and newer.
    V3_0_2,
}

impl Schema {
    pub const ALL: [Schema; 3] = [Self::V2_21_0, Self::V3_0_1, Self::V3_0_2];
    /// Works on Engine OS 4.3 up to 5.x.
    pub const DEFAULT: Schema = Self::V3_0_1;

    pub fn version(self) -> (i64, i64, i64) {
        match self {
            Self::V2_21_0 => (2, 21, 0),
            Self::V3_0_1 => (3, 0, 1),
            Self::V3_0_2 => (3, 0, 2),
        }
    }

    pub fn from_version(v: (i64, i64, i64)) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.version() == v)
    }

    /// `2.21.0`.
    pub fn name(self) -> String {
        let (a, b, c) = self.version();
        format!("{a}.{b}.{c}")
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.name() == s)
    }

    /// The oldest Engine OS that reads it.
    pub fn min_firmware(self) -> &'static str {
        match self {
            Self::V2_21_0 => "3.1",
            Self::V3_0_1 => "4.3",
            Self::V3_0_2 => "5.0",
        }
    }

    fn ddl(self) -> &'static str {
        match self {
            Self::V2_21_0 => include_str!("../ddl/schema_2_21_0.sql"),
            Self::V3_0_1 => include_str!("../ddl/schema_3_0_1.sql"),
            Self::V3_0_2 => include_str!("../ddl/schema_3_0_2.sql"),
        }
    }

    /// 3.x keeps the performance blobs in their own table; 2.x on `Track`.
    fn performance_table(self) -> bool {
        self != Self::V2_21_0
    }
}

/// A track for the library.
#[derive(Debug, Clone, Default)]
pub struct Track {
    /// Audio path relative to the `Engine Library` folder (`../Contents/…`).
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub comment: String,
    pub label: String,
    pub composer: String,
    pub remixer: String,
    pub year: u32,
    pub bpm: f64,
    /// Whole seconds.
    pub length: u32,
    /// kbps.
    pub bitrate: u32,
    pub file_bytes: u64,
    /// 0..=5 stars.
    pub rating: u8,
    /// Engine key ordinal, see [`convert::key_ordinal`].
    pub key: Option<i32>,
    /// Unix seconds.
    pub date_added: i64,
    pub track_data: TrackData,
    pub beats: BeatData,
    pub cues: QuickCues,
    pub loops: Loops,
    pub overview: Option<Overview>,
}

/// A playlist or folder. Engine keeps folders as playlists with children.
#[derive(Debug, Clone)]
pub struct Playlist {
    /// Caller's id, to link children to parents.
    pub key: i64,
    /// Parent's `key`; 0 at the top.
    pub parent: i64,
    pub title: String,
    pub sort: i64,
    /// Indexes into [`Library::tracks`], in order.
    pub tracks: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
}

/// A random version-4 UUID.
pub fn new_uuid() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b))
        .is_err()
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = (t ^ (std::process::id() as u128) << 64).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
