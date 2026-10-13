//! Tools that work on a mounted USB stick: open its library, extract
//! playlists into folders, and (later) clone and verify sticks.
//!
//! Everything here only **reads** from the stick.

pub mod extract;
pub mod names;
mod stick;

pub use stick::{Stick, StickError};
