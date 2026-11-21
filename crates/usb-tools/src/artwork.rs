//! Track artwork on the stick, the way a rekordbox export has it: two
//! thumbnails per picture in `/PIONEER/Artwork`, 80 and 240 pixels square,
//! named by the `artwork` row `export.pdb` points the track at.
//!
//! The picture comes from the library (rekordbox keeps `artwork_s.jpg` and
//! `artwork_m.jpg`, already the right size, next to `artwork.jpg`), else from
//! the picture embedded in the audio file. Thumbnails are made like
//! rekordbox's: scaled to fit, centred on black, JPEG at quality 85 (the
//! layout and sizes follow baken's measurements, see THIRD_PARTY.md).

use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;

pub const SMALL: u32 = 80;
pub const MEDIUM: u32 = 240;
const QUALITY: u8 = 85;

/// `/PIONEER/Artwork/00001/a1.jpg`, `…/a1_m.jpg` for the medium one. Folders
/// are numbered `id / 20 + 1`, as on rekordbox's sticks.
pub fn path(id: u32, medium: bool) -> String {
    let size = if medium { "_m" } else { "" };
    format!("/PIONEER/Artwork/{:05}/a{id}{size}.jpg", id / 20 + 1)
}

/// A picture's two thumbnails, JPEG encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnails {
    pub small: Vec<u8>,
    pub medium: Vec<u8>,
}

/// Where a track's picture comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// An image file of the library (rekordbox's `artwork.jpg`).
    Image(PathBuf),
    /// The picture embedded in the audio file.
    Embedded(PathBuf),
}

impl Source {
    /// The library's picture if it is there, else the audio file to look in.
    pub fn for_track(artwork: Option<&Path>, audio: &Path) -> Source {
        match artwork.filter(|p| p.is_file()) {
            Some(p) => Source::Image(p.to_path_buf()),
            None => Source::Embedded(audio.to_path_buf()),
        }
    }

    /// Changes when the file it reads does: path, size and modification time.
    pub fn fingerprint(&self) -> u64 {
        let (kind, p) = match self {
            Source::Image(p) => (1u8, p),
            Source::Embedded(p) => (2u8, p),
        };
        let meta = std::fs::metadata(p).ok();
        let size = meta.as_ref().map_or(0, |m| m.len());
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let mut h = crate::sync::fnv_bytes(0xcbf2_9ce4_8422_2325, &[kind]);
        h = crate::sync::fnv_bytes(h, p.to_string_lossy().as_bytes());
        h = crate::sync::fnv_bytes(h, &size.to_le_bytes());
        crate::sync::fnv_bytes(h, &mtime.to_le_bytes())
    }

    /// The thumbnails; `None` without a picture or one that cannot be read.
    pub fn thumbnails(&self) -> Option<Thumbnails> {
        match self {
            Source::Image(p) => {
                // rekordbox's own thumbnails beside the full picture.
                let stem = p.file_stem()?.to_string_lossy().into_owned();
                let sibling = |s: &str| std::fs::read(p.with_file_name(format!("{stem}_{s}.jpg"))).ok();
                if let (Some(small), Some(medium)) = (sibling("s"), sibling("m")) {
                    return Some(Thumbnails { small, medium });
                }
                thumbnails(&std::fs::read(p).ok()?)
            }
            Source::Embedded(audio) => thumbnails(&embedded(audio)?),
        }
    }
}

/// The picture embedded in `audio`, the front cover when there are several.
pub fn embedded(audio: &Path) -> Option<Vec<u8>> {
    let file = lofty::read_from_path(audio).ok()?;
    let pictures: Vec<_> = file.tags().iter().flat_map(|t| t.pictures()).collect();
    pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or(pictures.first())
        .map(|p| p.data().to_vec())
}

/// File extension for a picture's bytes: `png`, `bmp`, else `jpg`.
pub fn extension(picture: &[u8]) -> &'static str {
    match image::guess_format(picture) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::Bmp) => "bmp",
        _ => "jpg",
    }
}

/// `picture` (JPEG, PNG or BMP) as the two thumbnails.
pub fn thumbnails(picture: &[u8]) -> Option<Thumbnails> {
    let image = image::load_from_memory(picture).ok()?.to_rgb8();
    Some(Thumbnails {
        small: square(&image, SMALL)?,
        medium: square(&image, MEDIUM)?,
    })
}

fn square(image: &RgbImage, side: u32) -> Option<Vec<u8>> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = f64::from(side) / f64::from(w.max(h));
    let fit = |n: u32| ((f64::from(n) * scale).round() as u32).clamp(1, side);
    let scaled = imageops::resize(image, fit(w), fit(h), FilterType::Lanczos3);
    let mut canvas = RgbImage::from_pixel(side, side, Rgb([0, 0, 0]));
    let (x, y) = ((side - scaled.width()) / 2, (side - scaled.height()) / 2);
    imageops::overlay(&mut canvas, &scaled, i64::from(x), i64::from(y));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
        .encode_image(&canvas)
        .ok()?;
    Some(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_rekordbox_sticks() {
        assert_eq!(path(1, false), "/PIONEER/Artwork/00001/a1.jpg");
        assert_eq!(path(19, true), "/PIONEER/Artwork/00001/a19_m.jpg");
        assert_eq!(path(20, false), "/PIONEER/Artwork/00002/a20.jpg");
        assert_eq!(path(40, false), "/PIONEER/Artwork/00003/a40.jpg");
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([200, 40, 120]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_wide_picture_is_letterboxed_on_black() {
        let t = thumbnails(&png(500, 250)).unwrap();
        for (jpeg, side) in [(&t.small, SMALL), (&t.medium, MEDIUM)] {
            let img = image::load_from_memory(jpeg).unwrap().to_rgb8();
            assert_eq!(img.dimensions(), (side, side));
            let dark = |p: &Rgb<u8>| p.0.iter().all(|&c| c < 16);
            assert!(dark(img.get_pixel(side / 2, 1)), "black band above");
            assert!(dark(img.get_pixel(side / 2, side - 2)), "black band below");
            let mid = img.get_pixel(side / 2, side / 2).0;
            assert!(mid[0] > 150 && mid[1] < 90, "the picture in the middle: {mid:?}");
        }
        assert!(thumbnails(b"not a picture").is_none());
    }

    #[test]
    fn rekordbox_thumbnails_are_taken_as_they_are() {
        let d = tempfile::tempdir().unwrap();
        let full = d.path().join("artwork.jpg");
        std::fs::write(&full, png(800, 800)).unwrap();
        std::fs::write(d.path().join("artwork_s.jpg"), b"small").unwrap();
        std::fs::write(d.path().join("artwork_m.jpg"), b"medium").unwrap();
        let t = Source::for_track(Some(&full), Path::new("/nowhere.mp3")).thumbnails().unwrap();
        assert_eq!((t.small.as_slice(), t.medium.as_slice()), (&b"small"[..], &b"medium"[..]));
        // Without them the full picture is scaled.
        std::fs::remove_file(d.path().join("artwork_m.jpg")).unwrap();
        let t = Source::for_track(Some(&full), Path::new("/nowhere.mp3")).thumbnails().unwrap();
        assert_eq!(image::load_from_memory(&t.medium).unwrap().width(), MEDIUM);
    }
}
