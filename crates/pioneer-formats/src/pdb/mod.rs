//! DeviceSQL `export.pdb`: the library database read by every CDJ/XDJ.
//!
//! The file is a sequence of fixed-size pages (4096 bytes in every known
//! export). Page 0 holds the file header with one pointer per table; every
//! table is a linked list of pages. Data pages carry rows in a heap growing
//! up from offset 0x28 and a row directory growing down from the end of the
//! page in groups of 16.

mod encode;
mod fixed;
mod page;
mod reader;
mod rows;
mod writer;

pub use encode::history_property;
pub use page::{HEAP_START, Header, PageHeader, TablePointer};
pub use reader::{Database, RawRow, read_table_rows};
pub use rows::*;
pub use writer::{NUM_TABLES, PAGE_LEN, WriteOptions, write};

/// Table type ids used in `export.pdb`.
pub mod table {
    pub const TRACKS: u32 = 0;
    pub const GENRES: u32 = 1;
    pub const ARTISTS: u32 = 2;
    pub const ALBUMS: u32 = 3;
    pub const LABELS: u32 = 4;
    pub const KEYS: u32 = 5;
    pub const COLORS: u32 = 6;
    pub const PLAYLIST_TREE: u32 = 7;
    pub const PLAYLIST_ENTRIES: u32 = 8;
    pub const HISTORY_PLAYLISTS: u32 = 11;
    pub const HISTORY_ENTRIES: u32 = 12;
    pub const ARTWORK: u32 = 13;
    pub const COLUMNS: u32 = 16;
    pub const MENU: u32 = 17;
    pub const SORT: u32 = 18;
    pub const HISTORY: u32 = 19;

    pub fn name(ty: u32) -> &'static str {
        match ty {
            TRACKS => "tracks",
            GENRES => "genres",
            ARTISTS => "artists",
            ALBUMS => "albums",
            LABELS => "labels",
            KEYS => "keys",
            COLORS => "colors",
            PLAYLIST_TREE => "playlist_tree",
            PLAYLIST_ENTRIES => "playlist_entries",
            HISTORY_PLAYLISTS => "history_playlists",
            HISTORY_ENTRIES => "history_entries",
            ARTWORK => "artwork",
            COLUMNS => "columns",
            MENU => "menu",
            SORT => "sort",
            HISTORY => "history",
            _ => "unknown",
        }
    }
}
