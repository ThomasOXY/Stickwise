//! Tools that work on a mounted USB stick: open its library, extract
//! playlists into folders, and (later) clone and verify sticks.
//!
//! Everything here only **reads** from the stick.

pub mod compat;
pub mod extract;
pub mod names;
pub mod probe;
mod stick;
pub mod verify;

pub use stick::{Stick, StickError};
