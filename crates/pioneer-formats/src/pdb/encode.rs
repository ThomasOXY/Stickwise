//! Row encoders: the inverse of the parsers in `rows.rs`, producing the
//! bytes rekordbox 7 allocates for each row (layout rules measured by baken,
//! see THIRD_PARTY.md).
//!
//! `pos` is the row's position on its page; artists, albums and tracks store
//! `pos * 0x20` in their `index_shift` field.

use super::rows::*;
use crate::devicesql::encode as string;

pub(crate) fn align4(n: usize) -> usize {
    n.next_multiple_of(4)
}

fn u32s(out: &mut Vec<u8>, vals: &[u32]) {
    for v in vals {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// rekordbox packs short ASCII strings but starts every long string (UTF-16
/// and long ASCII) on a 4-byte boundary of the row. A CDJ-2000NXS2 froze on
/// unaligned UTF-16 ones (baken #116).
fn string_at(at: usize, encoded: &[u8]) -> usize {
    if encoded[0] & 1 == 0 { align4(at) } else { at }
}

fn index_shift(pos: usize) -> [u8; 2] {
    ((pos as u16).wrapping_mul(0x20)).to_le_bytes()
}

/// The `0x03, name_offset, name` tail of artists and albums, allocated
/// `align4(header) + align4(name) + 4` bytes like rekordbox does.
fn with_name(mut r: Vec<u8>, name: &str) -> Vec<u8> {
    let name = string(name);
    let header = r.len() + 2;
    let at = string_at(header, &name);
    r.extend_from_slice(&[0x03, at as u8]);
    r.resize(at, 0);
    r.extend_from_slice(&name);
    r.resize(align4(header) + align4(name.len()) + 4, 0);
    r
}

impl Named {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::new();
        u32s(&mut r, &[self.id]);
        r.extend(string(&self.name));
        r
    }
}

impl Artist {
    pub fn encode(&self, pos: usize) -> Vec<u8> {
        let mut r = vec![0x60, 0x00];
        r.extend_from_slice(&index_shift(pos));
        u32s(&mut r, &[self.id]);
        with_name(r, &self.name)
    }
}

impl Album {
    pub fn encode(&self, pos: usize) -> Vec<u8> {
        let mut r = vec![0x80, 0x00];
        r.extend_from_slice(&index_shift(pos));
        u32s(&mut r, &[0, self.artist_id, self.id, 0]);
        with_name(r, &self.name)
    }
}

impl Key {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::new();
        u32s(&mut r, &[self.id, self.id2]);
        r.extend(string(&self.name));
        r
    }
}

impl Color {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = vec![0, 0, 0, 0, self.id as u8];
        r.extend_from_slice(&self.id.to_le_bytes());
        r.push(0);
        r.extend(string(&self.name));
        r
    }
}

impl PlaylistNode {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::new();
        u32s(
            &mut r,
            &[
                self.parent_id,
                0,
                self.sort_order,
                self.id,
                self.is_folder as u32,
            ],
        );
        r.extend(string(&self.name));
        r
    }
}

impl PlaylistEntry {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::with_capacity(12);
        u32s(&mut r, &[self.entry_index, self.track_id, self.playlist_id]);
        r
    }
}

impl Artwork {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::new();
        u32s(&mut r, &[self.id]);
        r.extend(string(&self.path));
        r
    }
}

impl Column {
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&self.id.to_le_bytes());
        r.extend_from_slice(&self.code.to_le_bytes());
        r.extend(string(&self.name));
        r
    }
}

/// history (0x13): the single "property" row of an export:
/// `0x0280, track_count, 0, export_date, 0x19, 0x1e, "1000", device_name`,
/// zero-padded to 40 bytes. A CDJ-3000 shows the count as "Songs".
pub fn history_property(track_count: u32, export_date: &str, device_name: &str) -> Vec<u8> {
    let mut r = Vec::new();
    u32s(&mut r, &[0x0280, track_count, 0]);
    r.extend(string(export_date));
    r.extend_from_slice(&[0x19, 0x1e]);
    r.extend(string("1000"));
    let name = string(device_name);
    r.resize(string_at(r.len(), &name), 0);
    r.extend(name);
    if r.len() < 40 {
        r.resize(40, 0);
    }
    r
}

/// Constants rekordbox 7 writes into every track row (meaning unknown).
pub const TRACK_BITMASK: u32 = 0xC0700;
pub const TRACK_MASTER_DB_ID: u32 = 3_933_607_398;
pub const TRACK_ANALYSED_BITS: u16 = 41;

impl Track {
    /// A track row with the constants a fresh rekordbox 7 export uses.
    pub fn new_export(id: u32) -> Self {
        Track {
            id,
            bitmask: TRACK_BITMASK,
            unknown14: id + 20,
            unknown18: TRACK_MASTER_DB_ID,
            unknown56: TRACK_ANALYSED_BITS,
            unknown5c: 3,
            unknown_string2: "1".into(),
            unknown_string3: "1".into(),
            unknown_string4: "1".into(),
            kuvo_public: "ON".into(),
            autoload_hot_cues: "ON".into(),
            ..Default::default()
        }
    }

    pub fn encode(&self, pos: usize) -> Vec<u8> {
        let mut r = Vec::with_capacity(0x88 + 256);
        r.extend_from_slice(&0x24u16.to_le_bytes());
        r.extend_from_slice(&index_shift(pos));
        u32s(
            &mut r,
            &[
                self.bitmask,
                self.sample_rate,
                self.composer_id,
                self.file_size,
                self.unknown14,
                self.unknown18,
                self.artwork_id,
                self.key_id,
                self.original_artist_id,
                self.label_id,
                self.remixer_id,
                self.bitrate,
                self.track_number,
                self.tempo,
                self.genre_id,
                self.album_id,
                self.artist_id,
                self.id,
            ],
        );
        for v in [
            self.disc_number,
            self.play_count,
            self.year,
            self.sample_depth,
            self.duration,
            self.unknown56,
        ] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r.push(self.color_id);
        r.push(self.rating);
        r.extend_from_slice(&self.file_type.to_le_bytes());
        r.extend_from_slice(&self.unknown5c.to_le_bytes());
        debug_assert_eq!(r.len(), 0x5e);

        let encoded: Vec<Vec<u8>> = self.strings().iter().map(|s| string(s)).collect();
        let mut offsets = Vec::with_capacity(21);
        let mut off = 0x88;
        for e in &encoded {
            off = string_at(off, e);
            r.extend_from_slice(&(off as u16).to_le_bytes());
            offsets.push(off);
            off += e.len();
        }
        // rekordbox allocates 0x88 + sum(align4(string)) + 4 per track row,
        // which always covers the padding before UTF-16 strings.
        let alloc = 0x88 + encoded.iter().map(|e| align4(e.len())).sum::<usize>() + 4;
        for (e, at) in encoded.iter().zip(offsets) {
            r.resize(at, 0);
            r.extend_from_slice(e);
        }
        r.resize(alloc, 0);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Byte strings from a real rekordbox 7 export (via baken's fixtures).
    #[test]
    fn rows_match_rekordbox_bytes() {
        let key = Key {
            id: 1,
            id2: 1,
            name: "1A".into(),
        };
        assert_eq!(
            key.encode(),
            hex("01000000010000000731 41".replace(' ', "").as_str())
        );
        assert_eq!(
            Color {
                id: 1,
                name: "Pink".into()
            }
            .encode(),
            hex("00000000010100000b50696e6b")
        );
        assert_eq!(
            Named {
                id: 1,
                name: "Techno".into()
            }
            .encode(),
            hex("010000000f546563686e6f")
        );
        let a = Artist {
            id: 2,
            name: "tk_elektron".into(),
        }
        .encode(1);
        assert_eq!(&a[..10], &hex("6000200002000000030a")[..]);
        assert_eq!(a.len(), 28);
        assert_eq!(
            Artist {
                id: 11,
                name: "Rødhåd".into()
            }
            .encode(10),
            hex("600040010b000000030c0000901000005200f80064006800e500640000000000")
        );
        assert_eq!(
            Album {
                id: 153,
                artist_id: 0,
                name: "atöm.04".into()
            }
            .encode(2),
            hex(concat!(
                "800040000000000000000000990000000000000003180000",
                "9012000061007400f6006d002e0030003400000000000000"
            ))
        );
        let h = history_property(0, "2024-11-16", "");
        assert_eq!(h.len(), 40);
        assert_eq!(
            &h[..31],
            &hex("80020000000000000000000017323032342d31312d3136191e0b3130303003")[..]
        );
    }

    #[test]
    fn rows_parse_back() {
        let t = Track {
            title: "Příliš žluťoučký kůň 🅴".into(),
            file_path: "/Contents/Beyoncé/Crazy In Love.mp3".into(),
            comment: "2008".into(),
            tempo: 12800,
            ..Track::new_export(352)
        };
        let row = t.encode(3);
        assert_eq!(Track::parse(&row).unwrap(), t);

        let a = Artist {
            id: 7,
            name: "Ø".repeat(200),
        };
        assert_eq!(Artist::parse(&a.encode(0)).unwrap(), a);
        let al = Album {
            id: 9,
            artist_id: 7,
            name: "Ünïcode Album".into(),
        };
        assert_eq!(Album::parse(&al.encode(5)).unwrap(), al);

        let p = PlaylistNode {
            id: 4,
            parent_id: 2,
            sort_order: 1,
            is_folder: false,
            name: "Gig".into(),
        };
        assert_eq!(PlaylistNode::parse(&p.encode()).unwrap(), p);
        let c = Column {
            id: 1,
            code: 0x80,
            name: "\u{fffa}GENRE\u{fffb}".into(),
        };
        assert_eq!(Column::parse(&c.encode()).unwrap(), c);
        let c = Color {
            id: 3,
            name: "Orange".into(),
        };
        assert_eq!(Color::parse(&c.encode()).unwrap(), c);
    }
}
