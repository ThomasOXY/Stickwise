//! The sections rekordbox writes that analysis made elsewhere lacks.
//! rekordbox shows no grid or waveform at all for a track whose analysis
//! is missing them, so they are added, in rekordbox's order:
//!
//! - `.DAT`: `PPTH PVBR PQTZ PWAV PWV2 PCOB(hot) PCOB(memory)`
//! - `.EXT`: `PPTH PWV3 PCOB PCOB PCO2 PCO2 PQT2 PWV5 PWV4`
//! - `.2EX`: `PPTH PWV7 PWV6 PWVC`

use super::{AnlzFile, Section, tag};

const DAT_ORDER: &[[u8; 4]] = &[
    tag::PATH,
    tag::VBR,
    tag::BEAT_GRID,
    tag::WAVE_PREVIEW,
    tag::WAVE_TINY,
    tag::CUES,
];
const EXT_ORDER: &[[u8; 4]] = &[
    tag::PATH,
    tag::WAVE_SCROLL,
    tag::CUES,
    tag::CUES_2,
    tag::BEAT_GRID_2,
    tag::WAVE_COLOR_SCROLL,
    tag::WAVE_COLOR_PREVIEW,
];
const TWO_ORDER: &[[u8; 4]] = &[
    tag::PATH,
    tag::WAVE_3BAND_SCROLL,
    tag::WAVE_3BAND_PREVIEW,
    tag::WAVE_COLOR_2EX,
];

/// `PVBR`: 400 seek entries, all zero, then the length in samples (MP3; 0
/// for other formats), as rekordbox writes it for constant bit rate.
pub fn pvbr(mp3_samples: u32) -> Section {
    let mut body = vec![0u8; 400 * 4 + 4];
    body[1600..].copy_from_slice(&mp3_samples.to_be_bytes());
    Section::new(tag::VBR, vec![0; 4], body)
}

/// `PQT2` as rekordbox writes it without an extended grid.
pub fn pqt2_empty() -> Section {
    let mut header = vec![0u8; 44];
    header[4..8].copy_from_slice(&[0x01, 0x00, 0x00, 0x02]);
    Section::new(tag::BEAT_GRID_2, header, Vec::new())
}

/// `PWVC`: the three-band gains, rekordbox's most common ones.
fn pwvc() -> Section {
    let body = [80u16, 100, 100].iter().flat_map(|g| g.to_be_bytes()).collect();
    Section::new(tag::WAVE_COLOR_2EX, vec![0; 2], body)
}

/// `PWV4` (1200 columns of six bytes) from the three-band preview `PWV6`
/// (1200 columns of three): bytes 3..6 follow the bands, 0..3 their sum.
/// Fitted against rekordbox's own files.
fn pwv4(pwv6: &Section) -> Option<Section> {
    const SCALE: [f64; 3] = [2.4, 1.6, 1.1];
    if pwv6.body.len() != 3600 {
        return None;
    }
    // A louder three-band preview than rekordbox's would clip: the loud
    // parts (99th percentile) stay below the top.
    let scale: Vec<f64> = (0..3)
        .map(|j| {
            let mut v: Vec<u8> = pwv6.body.iter().skip(j).step_by(3).copied().collect();
            v.sort_unstable();
            let loud = v[v.len() * 99 / 100] as f64;
            if loud * SCALE[j] > 120.0 { 120.0 / loud } else { SCALE[j] }
        })
        .collect();
    let mut body = Vec::with_capacity(7200);
    for c in pwv6.body.chunks_exact(3) {
        let mut b = [0u8; 6];
        for j in 0..3 {
            b[3 + j] = (scale[j] * c[j] as f64).round().min(127.0) as u8;
        }
        let (r, g, bl) = (b[3] as f64, b[4] as f64, b[5] as f64);
        let max = r.max(g).max(bl);
        if max > 0.0 {
            b[2] = (0.91 * (r * r + g * g + bl * bl).sqrt()).round().min(127.0) as u8;
            b[0] = (0.88 * max + 38.0).round().min(127.0) as u8;
            b[1] = (203.0 - 0.49 * b[0] as f64).round().clamp(0.0, 255.0) as u8;
        }
        body.extend_from_slice(&b);
    }
    let header = [6u32, 1200, 0].iter().flat_map(|x| x.to_be_bytes()).collect();
    Some(Section::new(tag::WAVE_COLOR_PREVIEW, header, body))
}

/// Sections in `order`, cue lists hot before memory; others keep their place
/// after them.
fn reorder(f: &mut AnlzFile, order: &[[u8; 4]]) {
    let rank = |s: &Section| {
        let at = order.iter().position(|t| *t == s.tag).unwrap_or(order.len());
        let memory = (s.tag == tag::CUES || s.tag == tag::CUES_2) && s.header.get(..4) == Some(&[0, 0, 0, 0]);
        (at, memory)
    };
    f.sections.sort_by_key(rank);
}

/// Bring analysis that lacks rekordbox's `PVBR` up to what rekordbox
/// writes; `mp3_samples` is the track's length in samples for an MP3, else
/// 0. Analysis that has `PVBR` (rekordbox's own) is left alone. Returns
/// whether anything changed.
pub fn complete(dat: &mut AnlzFile, ext: Option<&mut AnlzFile>, two: Option<&mut AnlzFile>, mp3_samples: u32) -> bool {
    if dat.section(tag::VBR).is_some() {
        return false;
    }
    dat.sections.push(pvbr(mp3_samples));
    reorder(dat, DAT_ORDER);
    let preview = two.as_ref().and_then(|t| t.section(tag::WAVE_3BAND_PREVIEW)).and_then(pwv4);
    if let Some(ext) = ext {
        if ext.section(tag::BEAT_GRID_2).is_none() {
            ext.sections.push(pqt2_empty());
        }
        if let Some(p) = preview
            && ext.section(tag::WAVE_COLOR_PREVIEW).is_none()
        {
            ext.sections.push(p);
        }
        reorder(ext, EXT_ORDER);
    }
    if let Some(two) = two {
        if two.section(tag::WAVE_COLOR_2EX).is_none() {
            two.sections.push(pwvc());
        }
        reorder(two, TWO_ORDER);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anlz::{build_path, write_cue_points};

    fn tags(f: &AnlzFile) -> String {
        f.sections.iter().map(|s| s.tag_str()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn own_analysis_gets_rekordbox_sections_and_order() {
        let path = build_path("/Contents/a.mp3");
        let mut dat = AnlzFile::default();
        dat.sections.push(path.clone());
        dat.sections.push(Section::new(tag::BEAT_GRID, vec![0; 12], vec![]));
        dat.sections.push(Section::new(tag::WAVE_PREVIEW, vec![0; 8], vec![0; 400]));
        dat.sections.push(Section::new(tag::WAVE_TINY, vec![0; 8], vec![0; 100]));
        let mut ext = AnlzFile::default();
        ext.sections.push(path.clone());
        ext.sections.push(Section::new(tag::WAVE_SCROLL, vec![0; 12], vec![]));
        ext.sections.push(Section::new(tag::WAVE_COLOR_SCROLL, vec![0; 12], vec![]));
        write_cue_points(&mut dat, Some(&mut ext), &[]);
        let mut two = AnlzFile::default();
        two.sections.push(path);
        two.sections.push(Section::new(tag::WAVE_3BAND_SCROLL, vec![0; 12], vec![]));
        two.sections.push(Section::new(tag::WAVE_3BAND_PREVIEW, vec![0; 8], vec![20; 3600]));

        assert!(complete(&mut dat, Some(&mut ext), Some(&mut two), 1152 * 38));
        assert_eq!(tags(&dat), "PPTH PVBR PQTZ PWAV PWV2 PCOB PCOB");
        assert_eq!(tags(&ext), "PPTH PWV3 PCOB PCOB PCO2 PCO2 PQT2 PWV5 PWV4");
        assert_eq!(tags(&two), "PPTH PWV7 PWV6 PWVC");
        // Hot cues first, as rekordbox has them.
        let kinds: Vec<u8> = dat.sections_with(tag::CUES).map(|s| s.header[3]).collect();
        assert_eq!(kinds, [1, 0]);
        let kinds: Vec<u8> = ext.sections_with(tag::CUES_2).map(|s| s.header[3]).collect();
        assert_eq!(kinds, [1, 0]);
        // Lengths as in rekordbox's files.
        let bytes = |f: &AnlzFile, t| {
            let mut one = AnlzFile::default();
            one.sections.push(f.section(t).unwrap().clone());
            one.to_bytes().len() - 28
        };
        assert_eq!(bytes(&dat, tag::VBR), 1620);
        assert_eq!(bytes(&ext, tag::BEAT_GRID_2), 56);
        assert_eq!(bytes(&ext, tag::WAVE_COLOR_PREVIEW), 7224);
        assert_eq!(bytes(&two, tag::WAVE_COLOR_2EX), 20);
        assert_eq!(&dat.section(tag::VBR).unwrap().body[1600..], &(1152u32 * 38).to_be_bytes());
        // Round trip, and rekordbox's own analysis stays as it is.
        for f in [&dat, &ext, &two] {
            assert_eq!(AnlzFile::parse(&f.to_bytes()).unwrap().sections, f.sections);
        }
        assert!(!complete(&mut dat, Some(&mut ext), Some(&mut two), 0));
    }
}
