//! Pioneer / AlphaTheta device export formats.
//!
//! * [`pdb`] — the DeviceSQL `export.pdb` database every CDJ/XDJ reads.
//! * [`anlz`] — the `ANLZ0000.DAT/.EXT/.2EX` analysis files (beat grid,
//!   cues, loops, waveforms).
//!
//! Layouts follow the reverse-engineering work of Deep Symmetry
//! (crate-digger / djl-analysis), Henry Betts and Fabian Lesniak, and the
//! byte-level measurements of rekordbox 7 exports published by the MIT
//! licensed `baken` project.

pub mod anlz;
pub mod devicesql;
mod error;
pub mod pdb;

pub use error::{Error, Result};
