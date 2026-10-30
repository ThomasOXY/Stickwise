//! Tools that work on a mounted USB stick: open its library, extract
//! playlists into folders, and (later) clone and verify sticks.
//!
//! Writing goes through [`sync`] (with [`engine`] for Denon), [`cue_write`],
//! [`settings`] and [`format`], each with its own safety rules.

pub mod compat;
pub mod cue_write;
pub mod engine;
pub mod extract;
pub mod format;
pub mod names;
pub mod probe;
pub mod settings;
mod stick;
pub mod sync;
pub mod verify;

pub use stick::{Stick, StickError};
