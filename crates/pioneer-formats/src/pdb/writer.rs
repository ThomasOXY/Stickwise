//! Assemble a whole `export.pdb`.
//!
//! The layout reproduces a fresh rekordbox 7 export (as measured by baken,
//! see THIRD_PARTY.md): page 0 is the file header; then for every table an
//! index page followed by its data pages; then one zeroed page per table as
//! its `empty_candidate`. Page sequence numbers only need to be consistent
//! (every data page below the header's sequence); colors and columns keep
//! rekordbox's fixed 2 and 3.

use super::encode::{align4, history_property};
use super::fixed::{CATEGORY_ROWS, COLORS, COLUMNS, SORT_ROWS};
use super::page::{GROUP_BYTES, HEAP_START};
use super::reader::Database;
use super::rows::{Color, Column};
use super::table;

pub const PAGE_LEN: usize = 4096;
pub const NUM_TABLES: u32 = 20;
/// "No page" marker used in index pages and as `next` of a lone data page.
const NONE: u32 = 0x03FF_FFFF;

/// Device-level values that are not part of the library itself.
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    /// Shown by players as the device name; may be empty.
    pub device_name: String,
    /// `YYYY-MM-DD`.
    pub export_date: String,
}

/// How the two "fixed" tables differ from everything else in the page header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderStyle {
    /// `transaction_row_count = 1`, `transaction_row_index = rows - 1`.
    Normal,
    /// colors and columns: `transaction_row_count = rows`, index 0.
    Fixed,
}

/// Bytes the row directory needs for `rows` rows.
fn dir_bytes(rows: usize) -> usize {
    let rest = rows % 16;
    (rows / 16) * GROUP_BYTES + if rest > 0 { rest * 2 + 4 } else { 0 }
}

struct DataPage {
    index: u32,
    ty: u32,
    next: u32,
    seq: u32,
    style: HeaderStyle,
    rows: Vec<Vec<u8>>,
    heap_used: usize,
}

impl DataPage {
    fn new(index: u32, ty: u32, style: HeaderStyle) -> Self {
        DataPage {
            index,
            ty,
            next: NONE,
            seq: 0,
            style,
            rows: Vec::new(),
            heap_used: 0,
        }
    }

    /// Append a row if it fits; rows are padded to 4 bytes like rekordbox does.
    fn try_push(&mut self, row: &[u8]) -> bool {
        let need = self.heap_used + align4(row.len()) + dir_bytes(self.rows.len() + 1);
        if need > PAGE_LEN - HEAP_START || self.rows.len() >= 0x7ff {
            return false;
        }
        self.heap_used += align4(row.len());
        self.rows.push(row.to_vec());
        true
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut p = vec![0u8; PAGE_LEN];
        let n = self.rows.len();
        let free = PAGE_LEN - HEAP_START - self.heap_used - dir_bytes(n);
        let (tx_count, tx_index) = match self.style {
            HeaderStyle::Normal => (1u16, n.saturating_sub(1) as u16),
            HeaderStyle::Fixed => (n as u16, 0),
        };
        p[0x04..0x08].copy_from_slice(&self.index.to_le_bytes());
        p[0x08..0x0c].copy_from_slice(&self.ty.to_le_bytes());
        p[0x0c..0x10].copy_from_slice(&self.next.to_le_bytes());
        p[0x10..0x14].copy_from_slice(&self.seq.to_le_bytes());
        // num_row_offsets (13 bits) and num_rows (11 bits), both n.
        let packed = (n as u32 & 0x1fff) | ((n as u32 & 0x7ff) << 13);
        p[0x18..0x1b].copy_from_slice(&packed.to_le_bytes()[..3]);
        p[0x1b] = 0x24;
        p[0x1c..0x1e].copy_from_slice(&(free as u16).to_le_bytes());
        p[0x1e..0x20].copy_from_slice(&(self.heap_used as u16).to_le_bytes());
        p[0x20..0x22].copy_from_slice(&tx_count.to_le_bytes());
        p[0x22..0x24].copy_from_slice(&tx_index.to_le_bytes());

        let mut off = 0usize;
        for (i, row) in self.rows.iter().enumerate() {
            let start = HEAP_START + off;
            p[start..start + row.len()].copy_from_slice(row);
            let base = PAGE_LEN - (i / 16) * GROUP_BYTES;
            let at = base - 6 - 2 * (i % 16);
            p[at..at + 2].copy_from_slice(&(off as u16).to_le_bytes());
            // present flags, then the "touched by last transaction" flags.
            let flags = base - 4;
            let bits = u16::from_le_bytes([p[flags], p[flags + 1]]) | 1 << (i % 16);
            p[flags..flags + 2].copy_from_slice(&bits.to_le_bytes());
            p[flags + 2..flags + 4].copy_from_slice(&bits.to_le_bytes());
            off += align4(row.len());
        }
        p
    }
}

/// The first page of every table. `next` is the first data page, or the
/// empty candidate when the table has no rows.
fn index_page(index: u32, ty: u32, next: u32, first_data: Option<u32>) -> Vec<u8> {
    let mut p = vec![0u8; PAGE_LEN];
    p[0x04..0x08].copy_from_slice(&index.to_le_bytes());
    p[0x08..0x0c].copy_from_slice(&ty.to_le_bytes());
    p[0x0c..0x10].copy_from_slice(&next.to_le_bytes());
    p[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
    p[0x1b] = 0x64;
    p[0x20..0x22].copy_from_slice(&0x1fffu16.to_le_bytes());
    p[0x22..0x24].copy_from_slice(&0x1fffu16.to_le_bytes());
    p[0x24..0x26].copy_from_slice(&0x03ecu16.to_le_bytes());
    let mut off = HEAP_START;
    for w in [index, first_data.unwrap_or(NONE), NONE, 0, 0x1FFF_0000] {
        p[off..off + 4].copy_from_slice(&w.to_le_bytes());
        off += 4;
    }
    while off < PAGE_LEN - 20 {
        p[off..off + 4].copy_from_slice(&0x1FFF_FFF8u32.to_le_bytes());
        off += 4;
    }
    p
}

fn header_page(next_unused: u32, sequence: u32, pointers: &[[u32; 4]]) -> Vec<u8> {
    let mut p = vec![0u8; PAGE_LEN];
    p[0x04..0x08].copy_from_slice(&(PAGE_LEN as u32).to_le_bytes());
    p[0x08..0x0c].copy_from_slice(&(pointers.len() as u32).to_le_bytes());
    p[0x0c..0x10].copy_from_slice(&next_unused.to_le_bytes());
    p[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
    p[0x14..0x18].copy_from_slice(&sequence.to_le_bytes());
    let mut off = 0x1c;
    for ptr in pointers {
        for w in ptr {
            p[off..off + 4].copy_from_slice(&w.to_le_bytes());
            off += 4;
        }
    }
    p
}

type RowFn<'a> = Box<dyn Fn(usize) -> Vec<u8> + 'a>;

struct Table<'a> {
    style: HeaderStyle,
    /// Fixed page sequence (colors, columns), otherwise from the counter.
    seq: Option<u32>,
    rows: Vec<RowFn<'a>>,
}

fn tables<'a>(
    db: &'a Database,
    opts: &'a WriteOptions,
    colors: &'a [Color],
    columns: &'a [Column],
) -> Vec<Table<'a>> {
    let mut t: Vec<Table> = (0..NUM_TABLES)
        .map(|_| Table {
            style: HeaderStyle::Normal,
            seq: None,
            rows: Vec::new(),
        })
        .collect();
    fn rows<'a, T>(
        items: &'a [T],
        f: impl Fn(&'a T, usize) -> Vec<u8> + Copy + 'a,
    ) -> Vec<RowFn<'a>> {
        items
            .iter()
            .map(|it| Box::new(move |pos| f(it, pos)) as RowFn<'a>)
            .collect()
    }
    t[table::TRACKS as usize].rows = rows(&db.tracks, |r, pos| r.encode(pos));
    t[table::GENRES as usize].rows = rows(&db.genres, |r, _| r.encode());
    t[table::ARTISTS as usize].rows = rows(&db.artists, |r, pos| r.encode(pos));
    t[table::ALBUMS as usize].rows = rows(&db.albums, |r, pos| r.encode(pos));
    t[table::LABELS as usize].rows = rows(&db.labels, |r, _| r.encode());
    t[table::KEYS as usize].rows = rows(&db.keys, |r, _| r.encode());
    let c = &mut t[table::COLORS as usize];
    c.style = HeaderStyle::Fixed;
    c.seq = Some(2);
    c.rows = rows(colors, |r, _| r.encode());
    t[table::PLAYLIST_TREE as usize].rows = rows(&db.playlists, |r, _| r.encode());
    t[table::PLAYLIST_ENTRIES as usize].rows = rows(&db.playlist_entries, |r, _| r.encode());
    t[table::ARTWORK as usize].rows = rows(&db.artwork, |r, _| r.encode());
    let c = &mut t[table::COLUMNS as usize];
    c.style = HeaderStyle::Fixed;
    c.seq = Some(3);
    c.rows = rows(columns, |r, _| r.encode());
    t[table::MENU as usize].rows = rows(&CATEGORY_ROWS, |r, _| r.to_vec());
    t[table::SORT as usize].rows = rows(&SORT_ROWS, |r, _| r.to_vec());
    t[table::HISTORY as usize].rows = vec![Box::new(move |_| {
        history_property(db.tracks.len() as u32, &opts.export_date, &opts.device_name)
    })];
    t
}

/// Serialise a library as a fresh `export.pdb` (a whole number of pages).
///
/// Uses `db.colors` when present, otherwise rekordbox's default palette;
/// columns, menu and sort tables are always rekordbox's defaults. History
/// tables are written empty, as in a fresh export.
pub fn write(db: &Database, opts: &WriteOptions) -> Vec<u8> {
    let colors: Vec<Color> = if db.colors.is_empty() {
        COLORS
            .iter()
            .map(|(id, name, _)| Color {
                id: *id as u16,
                name: name.to_string(),
            })
            .collect()
    } else {
        db.colors.clone()
    };
    let columns: Vec<Column> = COLUMNS
        .iter()
        .map(|(id, code, name)| Column {
            id: *id,
            code: *code,
            name: name.to_string(),
        })
        .collect();
    let tables = tables(db, opts, &colors, &columns);

    let mut pages: Vec<Vec<u8>> = vec![Vec::new()]; // page 0 is filled last
    let mut next_seq = 10u32;
    let mut max_seq = 3u32;
    let mut pending: Vec<(u32, usize, Vec<DataPage>)> = Vec::new();

    for (ty, t) in tables.iter().enumerate() {
        let ty = ty as u32;
        let index_pos = pages.len();
        pages.push(Vec::new());
        let mut data: Vec<DataPage> = Vec::new();
        let mut page = DataPage::new(pages.len() as u32, ty, t.style);
        for make in &t.rows {
            let row = make(page.rows.len());
            if !page.try_push(&row) {
                data.push(page);
                page = DataPage::new((pages.len() + data.len()) as u32, ty, t.style);
                assert!(page.try_push(&make(0)), "row larger than a page");
            }
        }
        if !page.rows.is_empty() {
            data.push(page);
        }
        for p in &mut data {
            p.seq = t.seq.unwrap_or_else(|| {
                next_seq += 1;
                next_seq
            });
            max_seq = max_seq.max(p.seq);
            pages.push(Vec::new());
        }
        pending.push((ty, index_pos, data));
    }

    let first_empty = pages.len() as u32;
    let mut pointers = Vec::new();
    for (i, (ty, index_pos, mut data)) in pending.into_iter().enumerate() {
        let empty = first_empty + i as u32;
        let first_data = data.first().map(|p| p.index);
        let last = data.last().map(|p| p.index).unwrap_or(index_pos as u32);
        pointers.push([ty, empty, index_pos as u32, last]);
        pages[index_pos] = index_page(
            index_pos as u32,
            ty,
            first_data.unwrap_or(empty),
            first_data,
        );
        let n = data.len();
        for (k, p) in data.iter_mut().enumerate() {
            p.next = if k + 1 < n { p.index + 1 } else { empty };
            pages[p.index as usize] = p.to_bytes();
        }
    }
    for _ in 0..NUM_TABLES {
        pages.push(vec![0u8; PAGE_LEN]);
    }
    pages[0] = header_page(pages.len() as u32, max_seq + 1, &pointers);
    pages.concat()
}
