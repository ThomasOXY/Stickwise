//! Typed rows of `export.pdb`. Each `parse` takes the page bytes from the
//! start of the row to the end of the page (strings are addressed relative
//! to the row start).

use crate::devicesql;
use crate::error::{Bytes, Result};

fn string_at(row: &[u8], offset: usize) -> Result<String> {
    Ok(devicesql::decode(row, offset)?.0)
}

/// Artists, albums and tags store their name offset either as one byte or,
/// when bit 0x04 of the subtype is set, as a u16 at `far_at`.
fn name_offset(b: &Bytes, subtype: u16, near_at: usize, far_at: usize) -> Result<usize> {
    Ok(if subtype & 0x04 != 0 {
        b.u16le(far_at)? as usize
    } else {
        b.u8(near_at)? as usize
    })
}

/// genres, labels: `id, name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub id: u32,
    pub name: String,
}

impl Named {
    pub fn parse(row: &[u8]) -> Result<Self> {
        Ok(Named {
            id: Bytes(row).u32le(0)?,
            name: string_at(row, 4)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artist {
    pub id: u32,
    pub name: String,
}

impl Artist {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        let subtype = b.u16le(0)?;
        let at = name_offset(&b, subtype, 0x09, 0x0a)?;
        Ok(Artist {
            id: b.u32le(4)?,
            name: string_at(row, at)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Album {
    pub id: u32,
    pub artist_id: u32,
    pub name: String,
}

impl Album {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        let subtype = b.u16le(0)?;
        let at = name_offset(&b, subtype, 0x15, 0x16)?;
        Ok(Album {
            id: b.u32le(0x0c)?,
            artist_id: b.u32le(0x08)?,
            name: string_at(row, at)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub id: u32,
    pub id2: u32,
    pub name: String,
}

impl Key {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(Key {
            id: b.u32le(0)?,
            id2: b.u32le(4)?,
            name: string_at(row, 8)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Color {
    pub id: u16,
    pub name: String,
}

impl Color {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(Color {
            id: b.u16le(5)?,
            name: string_at(row, 8)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistNode {
    pub id: u32,
    /// 0 at the root.
    pub parent_id: u32,
    pub sort_order: u32,
    pub is_folder: bool,
    pub name: String,
}

impl PlaylistNode {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(PlaylistNode {
            parent_id: b.u32le(0)?,
            sort_order: b.u32le(8)?,
            id: b.u32le(12)?,
            is_folder: b.u32le(16)? != 0,
            name: string_at(row, 20)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistEntry {
    /// 1-based position in the playlist.
    pub entry_index: u32,
    pub track_id: u32,
    pub playlist_id: u32,
}

impl PlaylistEntry {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(PlaylistEntry {
            entry_index: b.u32le(0)?,
            track_id: b.u32le(4)?,
            playlist_id: b.u32le(8)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPlaylist {
    pub id: u32,
    pub name: String,
}

impl HistoryPlaylist {
    pub fn parse(row: &[u8]) -> Result<Self> {
        Ok(HistoryPlaylist {
            id: Bytes(row).u32le(0)?,
            name: string_at(row, 4)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryEntry {
    pub track_id: u32,
    pub playlist_id: u32,
    pub entry_index: u32,
}

impl HistoryEntry {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(HistoryEntry {
            track_id: b.u32le(0)?,
            playlist_id: b.u32le(4)?,
            entry_index: b.u32le(8)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    pub id: u32,
    pub path: String,
}

impl Artwork {
    pub fn parse(row: &[u8]) -> Result<Self> {
        Ok(Artwork {
            id: Bytes(row).u32le(0)?,
            path: string_at(row, 4)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub id: u16,
    pub code: u16,
    pub name: String,
}

impl Column {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        Ok(Column {
            id: b.u16le(0)?,
            code: b.u16le(2)?,
            name: string_at(row, 4)?,
        })
    }
}

/// File type codes stored in the track row.
pub mod file_type {
    pub const MP3: u16 = 1;
    pub const M4A: u16 = 4;
    pub const FLAC: u16 = 5;
    pub const ALAC: u16 = 6;
    pub const WAV: u16 = 11;
    pub const AIFF: u16 = 12;
}

/// One `tracks` row. Fields without a known meaning are kept so a row can be
/// reproduced exactly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    pub id: u32,
    pub bitmask: u32,
    pub sample_rate: u32,
    pub composer_id: u32,
    pub file_size: u32,
    pub unknown14: u32,
    pub unknown18: u32,
    pub artwork_id: u32,
    pub key_id: u32,
    pub original_artist_id: u32,
    pub label_id: u32,
    pub remixer_id: u32,
    pub bitrate: u32,
    pub track_number: u32,
    /// BPM × 100.
    pub tempo: u32,
    pub genre_id: u32,
    pub album_id: u32,
    pub artist_id: u32,
    pub disc_number: u16,
    pub play_count: u16,
    pub year: u16,
    pub sample_depth: u16,
    /// Seconds.
    pub duration: u16,
    pub unknown56: u16,
    pub color_id: u8,
    /// 0..=5 stars.
    pub rating: u8,
    pub file_type: u16,
    pub unknown5c: u16,
    pub isrc: String,
    pub lyricist: String,
    pub unknown_string2: String,
    pub unknown_string3: String,
    pub unknown_string4: String,
    pub message: String,
    pub kuvo_public: String,
    pub autoload_hot_cues: String,
    pub unknown_string5: String,
    pub unknown_string6: String,
    pub date_added: String,
    pub release_date: String,
    pub mix_name: String,
    pub unknown_string7: String,
    /// Path of `ANLZ0000.DAT` on the device, e.g. `/PIONEER/USBANLZ/P016/0000875E/ANLZ0000.DAT`.
    pub analyze_path: String,
    pub analyze_date: String,
    pub comment: String,
    pub title: String,
    pub unknown_string8: String,
    pub filename: String,
    /// Path of the audio file on the device, e.g. `/Contents/Artist/Album/track.mp3`.
    pub file_path: String,
}

/// Offset of the 21 string offsets inside a track row.
pub(crate) const TRACK_STRINGS_AT: usize = 0x5e;

impl Track {
    pub fn parse(row: &[u8]) -> Result<Self> {
        let b = Bytes(row);
        let s = |k: usize| -> Result<String> {
            string_at(row, b.u16le(TRACK_STRINGS_AT + 2 * k)? as usize)
        };
        Ok(Track {
            bitmask: b.u32le(0x04)?,
            sample_rate: b.u32le(0x08)?,
            composer_id: b.u32le(0x0c)?,
            file_size: b.u32le(0x10)?,
            unknown14: b.u32le(0x14)?,
            unknown18: b.u32le(0x18)?,
            artwork_id: b.u32le(0x1c)?,
            key_id: b.u32le(0x20)?,
            original_artist_id: b.u32le(0x24)?,
            label_id: b.u32le(0x28)?,
            remixer_id: b.u32le(0x2c)?,
            bitrate: b.u32le(0x30)?,
            track_number: b.u32le(0x34)?,
            tempo: b.u32le(0x38)?,
            genre_id: b.u32le(0x3c)?,
            album_id: b.u32le(0x40)?,
            artist_id: b.u32le(0x44)?,
            id: b.u32le(0x48)?,
            disc_number: b.u16le(0x4c)?,
            play_count: b.u16le(0x4e)?,
            year: b.u16le(0x50)?,
            sample_depth: b.u16le(0x52)?,
            duration: b.u16le(0x54)?,
            unknown56: b.u16le(0x56)?,
            color_id: b.u8(0x58)?,
            rating: b.u8(0x59)?,
            file_type: b.u16le(0x5a)?,
            unknown5c: b.u16le(0x5c)?,
            isrc: s(0)?,
            lyricist: s(1)?,
            unknown_string2: s(2)?,
            unknown_string3: s(3)?,
            unknown_string4: s(4)?,
            message: s(5)?,
            kuvo_public: s(6)?,
            autoload_hot_cues: s(7)?,
            unknown_string5: s(8)?,
            unknown_string6: s(9)?,
            date_added: s(10)?,
            release_date: s(11)?,
            mix_name: s(12)?,
            unknown_string7: s(13)?,
            analyze_path: s(14)?,
            analyze_date: s(15)?,
            comment: s(16)?,
            title: s(17)?,
            unknown_string8: s(18)?,
            filename: s(19)?,
            file_path: s(20)?,
        })
    }

    /// The 21 strings in on-disk order.
    pub fn strings(&self) -> [&str; 21] {
        [
            &self.isrc,
            &self.lyricist,
            &self.unknown_string2,
            &self.unknown_string3,
            &self.unknown_string4,
            &self.message,
            &self.kuvo_public,
            &self.autoload_hot_cues,
            &self.unknown_string5,
            &self.unknown_string6,
            &self.date_added,
            &self.release_date,
            &self.mix_name,
            &self.unknown_string7,
            &self.analyze_path,
            &self.analyze_date,
            &self.comment,
            &self.title,
            &self.unknown_string8,
            &self.filename,
            &self.file_path,
        ]
    }

    pub fn bpm(&self) -> f64 {
        self.tempo as f64 / 100.0
    }
}
