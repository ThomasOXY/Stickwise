use std::collections::HashSet;

use super::page::{HEAP_START, Header, PageHeader};
use super::rows::*;
use super::table;
use crate::error::{Error, Result};

/// A row as found on disk, before decoding.
#[derive(Debug, Clone, Copy)]
pub struct RawRow<'a> {
    pub page: u32,
    /// Offset of the row from the start of its page.
    pub offset: usize,
    /// Page bytes from the row start to the end of the page.
    pub data: &'a [u8],
}

/// Walk the page chain of one table and return every present row.
///
/// Damaged chains (loops, pointers past the end, foreign pages) end the walk
/// and are reported in `problems` instead of failing, so a broken stick can
/// still be salvaged.
pub fn read_table_rows<'a>(
    file: &'a [u8],
    header: &Header,
    ty: u32,
    problems: &mut Vec<String>,
) -> Vec<RawRow<'a>> {
    let mut rows = Vec::new();
    let Some(ptr) = header.table(ty) else {
        return rows;
    };
    let page_len = header.page_len as usize;
    let num_pages = file.len() / page_len;
    let mut seen = HashSet::new();
    let mut index = ptr.first_page;
    loop {
        if index as usize >= num_pages {
            problems.push(format!(
                "{}: page {index} is past the end of the file",
                table::name(ty)
            ));
            break;
        }
        if !seen.insert(index) {
            problems.push(format!("{}: page chain loops at {index}", table::name(ty)));
            break;
        }
        let page = &file[index as usize * page_len..(index as usize + 1) * page_len];
        let ph = match PageHeader::parse(page) {
            Ok(ph) => ph,
            Err(e) => {
                problems.push(format!("{}: page {index}: {e}", table::name(ty)));
                break;
            }
        };
        if ph.ty != ty {
            // rekordbox chains can run into a page of another table; that ends the list.
            break;
        }
        if ph.is_data() {
            match ph.row_offsets(page) {
                Ok(offsets) => {
                    for ofs in offsets {
                        let offset = HEAP_START + ofs as usize;
                        if offset >= page_len {
                            problems.push(format!(
                                "{}: page {index}: row offset {offset:#x} outside page",
                                table::name(ty)
                            ));
                            continue;
                        }
                        rows.push(RawRow {
                            page: index,
                            offset,
                            data: &page[offset..],
                        });
                    }
                }
                Err(e) => problems.push(format!("{}: page {index}: {e}", table::name(ty))),
            }
        }
        if index == ptr.last_page {
            break;
        }
        index = ph.next;
    }
    rows
}

/// Every table of an `export.pdb`, decoded.
#[derive(Debug, Clone, Default)]
pub struct Database {
    pub header: Option<Header>,
    pub tracks: Vec<Track>,
    pub genres: Vec<Named>,
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub labels: Vec<Named>,
    pub keys: Vec<Key>,
    pub colors: Vec<Color>,
    pub playlists: Vec<PlaylistNode>,
    pub playlist_entries: Vec<PlaylistEntry>,
    pub history_playlists: Vec<HistoryPlaylist>,
    pub history_entries: Vec<HistoryEntry>,
    pub artwork: Vec<Artwork>,
    pub columns: Vec<Column>,
    /// Everything that could not be read. Empty for a healthy file.
    pub problems: Vec<String>,
}

impl Database {
    /// Decode a whole file. Only an unreadable header is an error; damaged
    /// pages and rows are skipped and listed in [`Database::problems`].
    pub fn parse(file: &[u8]) -> Result<Self> {
        let header = Header::parse(file)?;
        if file.len() < header.page_len as usize {
            return Err(Error::Invalid("file shorter than one page".into()));
        }
        let mut db = Database::default();

        macro_rules! load {
            ($field:ident, $ty:expr, $parse:path) => {
                for row in read_table_rows(file, &header, $ty, &mut db.problems) {
                    match $parse(row.data) {
                        Ok(v) => db.$field.push(v),
                        Err(e) => db.problems.push(format!(
                            "{}: row at page {} offset {:#x}: {e}",
                            table::name($ty),
                            row.page,
                            row.offset
                        )),
                    }
                }
            };
        }

        load!(tracks, table::TRACKS, Track::parse);
        load!(genres, table::GENRES, Named::parse);
        load!(artists, table::ARTISTS, Artist::parse);
        load!(albums, table::ALBUMS, Album::parse);
        load!(labels, table::LABELS, Named::parse);
        load!(keys, table::KEYS, Key::parse);
        load!(colors, table::COLORS, Color::parse);
        load!(playlists, table::PLAYLIST_TREE, PlaylistNode::parse);
        load!(
            playlist_entries,
            table::PLAYLIST_ENTRIES,
            PlaylistEntry::parse
        );
        load!(
            history_playlists,
            table::HISTORY_PLAYLISTS,
            HistoryPlaylist::parse
        );
        load!(history_entries, table::HISTORY_ENTRIES, HistoryEntry::parse);
        load!(artwork, table::ARTWORK, Artwork::parse);
        load!(columns, table::COLUMNS, Column::parse);

        db.header = Some(header);
        Ok(db)
    }

    pub fn track(&self, id: u32) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn artist_name(&self, id: u32) -> Option<&str> {
        self.artists
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.as_str())
    }

    pub fn album_name(&self, id: u32) -> Option<&str> {
        self.albums
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.as_str())
    }

    pub fn genre_name(&self, id: u32) -> Option<&str> {
        self.genres
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.as_str())
    }

    pub fn key_name(&self, id: u32) -> Option<&str> {
        self.keys
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.as_str())
    }

    /// Track ids of a playlist in play order.
    pub fn playlist_tracks(&self, playlist_id: u32) -> Vec<u32> {
        let mut entries: Vec<_> = self
            .playlist_entries
            .iter()
            .filter(|e| e.playlist_id == playlist_id)
            .collect();
        entries.sort_by_key(|e| e.entry_index);
        entries.iter().map(|e| e.track_id).collect()
    }

    /// Children of a folder (0 = root) in display order.
    pub fn playlist_children(&self, parent_id: u32) -> Vec<&PlaylistNode> {
        let mut nodes: Vec<_> = self
            .playlists
            .iter()
            .filter(|p| p.parent_id == parent_id)
            .collect();
        nodes.sort_by_key(|p| p.sort_order);
        nodes
    }

    /// Folder names from the root down to (and including) `playlist_id`.
    pub fn playlist_path(&self, playlist_id: u32) -> Vec<&str> {
        let mut path = Vec::new();
        let mut id = playlist_id;
        let mut guard = 0;
        while let Some(node) = self.playlists.iter().find(|p| p.id == id) {
            path.push(node.name.as_str());
            id = node.parent_id;
            guard += 1;
            if id == 0 || guard > 64 {
                break;
            }
        }
        path.reverse();
        path
    }
}
