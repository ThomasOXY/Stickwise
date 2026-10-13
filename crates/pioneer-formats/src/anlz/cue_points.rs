//! A track's cue points as one list, read from and written to the `.DAT`
//! and `.EXT` analysis files the way rekordbox 6/7 lays them out:
//!
//! * `.DAT` `PCOB` hot list: hot cues A–C (all older players understand);
//!   `.DAT` `PCOB` memory list: memory cues and loops.
//! * `.EXT` `PCOB` hot list: hot cues D–H; `.EXT` `PCOB` memory list: empty.
//! * `.EXT` `PCO2` hot list: every hot cue (D–H first, then A–C) with colour
//!   and comment; `.EXT` `PCO2` memory list: memory cues with colour.
//!
//! Lists run from the highest slot / latest time down. Memory lists are a
//! doubly linked chain in list order; hot lists are not linked.

use super::cues::{Cue, CueList, CueListKind, CueType, ExtCue, ExtCueList};
use super::{AnlzFile, tag};
use crate::error::Result;

/// One cue point, independent of how it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuePoint {
    /// 0 = memory cue / memory loop, 1..=8 = hot cue A..H.
    pub slot: u8,
    pub time_ms: u32,
    /// End of the loop, `None` for a plain cue.
    pub loop_end_ms: Option<u32>,
    /// Loop length in beats as numerator/denominator when quantized.
    pub loop_beats: Option<(u16, u16)>,
    pub comment: String,
    /// Hot cue colour code (index into rekordbox's 64-colour table, see
    /// [`hot_cue_color`]); 0 = default.
    pub hot_color: u8,
    /// Pad LED colour as stored; `None` derives it from `hot_color`. Set to
    /// `None` whenever the colour is changed.
    pub hot_led: Option<[u8; 3]>,
    /// Memory cue colour (row of the `colors` table, 1..=8); 0 = none.
    pub memory_color: u8,
    /// The loop that is armed when the track loads (memory loops only).
    pub active_loop: bool,
}

impl CuePoint {
    pub fn hot(slot: u8, time_ms: u32) -> Self {
        CuePoint {
            slot,
            time_ms,
            loop_end_ms: None,
            loop_beats: None,
            comment: String::new(),
            hot_color: 0,
            hot_led: None,
            memory_color: 0,
            active_loop: false,
        }
    }

    pub fn memory(time_ms: u32) -> Self {
        CuePoint {
            slot: 0,
            ..CuePoint::hot(0, time_ms)
        }
    }

    pub fn is_loop(&self) -> bool {
        self.loop_end_ms.is_some()
    }
}

/// rekordbox's hot cue colour table: code → (display colour, LED colour the
/// players light the pad with). Display colours from Beat Link's
/// `CueList.findRekordboxColor`; LED colours as rekordbox writes them where
/// observed, otherwise the display colour.
pub fn hot_cue_color(code: u8) -> Option<([u8; 3], [u8; 3])> {
    const DISPLAY: [u32; 62] = [
        0x305aff, 0x5073ff, 0x508cff, 0x50a0ff, 0x50b4ff, 0x50b0f2, 0x50aee8, 0x45acdb, 0x00e0ff,
        0x19daf0, 0x32d2e6, 0x21b4b9, 0x20aaa0, 0x1fa392, 0x19a08c, 0x14a584, 0x14aa7d, 0x10b176,
        0x30d26e, 0x37de5a, 0x3ceb50, 0x28e214, 0x7dc13d, 0x8cc832, 0x9bd723, 0xa5e116, 0xa5dc0a,
        0xaad208, 0xb4c805, 0xb4be04, 0xbab404, 0xc3af04, 0xe1aa00, 0xffa000, 0xff9600, 0xff8c00,
        0xff7500, 0xe0641b, 0xe0461e, 0xe0301e, 0xe02823, 0xe62828, 0xff376f, 0xff2d6f, 0xff127b,
        0xf51e8c, 0xeb2da0, 0xe637b4, 0xde44cf, 0xde448d, 0xe630b4, 0xe619dc, 0xe600ff, 0xdc00ff,
        0xcc00ff, 0xb432ff, 0xb93cff, 0xc542ff, 0xaa5aff, 0xaa72ff, 0x8272ff, 0x6473ff,
    ];
    let rgb = |v: u32| [(v >> 16) as u8, (v >> 8) as u8, v as u8];
    let led = match code {
        0x01 => [0x00, 0x00, 0xff],
        0x05 => [0x00, 0x70, 0xff],
        0x09 => [0x00, 0xe0, 0xff],
        0x16 => [0x1a, 0xff, 0x00],
        0x26 => [0xff, 0x5e, 0x00],
        0x2a => [0xff, 0x00, 0x00],
        0x2d => [0xff, 0x00, 0x45],
        0x3c => [0x4d, 0x00, 0xff],
        c if (1..=62).contains(&c) => rgb(DISPLAY[c as usize - 1]),
        _ => return None,
    };
    Some((rgb(DISPLAY[code as usize - 1]), led))
}

/// The 16 colours of rekordbox's hot cue colour grid.
pub const HOT_CUE_PALETTE: [u8; 16] = [
    0x01, 0x05, 0x09, 0x0e, 0x12, 0x16, 0x1a, 0x1e, 0x22, 0x26, 0x2a, 0x2d, 0x31, 0x34, 0x38, 0x3c,
];

/// LED colours rekordbox uses when no colour is assigned: green for a hot
/// cue, orange for a hot loop.
const DEFAULT_LED: [u8; 3] = [0x1a, 0xff, 0x00];
const DEFAULT_LOOP_LED: [u8; 3] = [0xff, 0x8c, 0x00];
const NO_LOOP: u32 = 0xffff_ffff;
const PAD: [u8; 3] = [0x00, 0x03, 0xe8];

/// Read every cue point, preferring the `.EXT` `PCO2` lists (colours,
/// comments, hot cues D–H) and falling back to the `.DAT` `PCOB` lists.
pub fn read_cue_points(dat: &AnlzFile, ext: Option<&AnlzFile>) -> Result<Vec<CuePoint>> {
    let mut out = Vec::new();
    let ext_lists = match ext {
        Some(e) => e.ext_cue_lists()?,
        None => Vec::new(),
    };
    let active_times: Vec<u32> = dat
        .cue_lists()?
        .iter()
        .flat_map(|l| l.cues.iter())
        .filter(|c| c.status == 4)
        .map(|c| c.time_ms)
        .collect();
    if !ext_lists.is_empty() {
        for list in ext_lists {
            for c in list.cues {
                let is_loop = c.cue_type == CueType::Loop && c.loop_time_ms != NO_LOOP;
                out.push(CuePoint {
                    slot: c.hot_cue as u8,
                    time_ms: c.time_ms,
                    loop_end_ms: is_loop.then_some(c.loop_time_ms),
                    loop_beats: (c.loop_denominator != 0)
                        .then_some((c.loop_numerator, c.loop_denominator)),
                    comment: c.comment.unwrap_or_default(),
                    hot_color: c.color.map(|(code, _)| code).unwrap_or(0),
                    hot_led: c.color.filter(|_| c.hot_cue > 0).map(|(_, rgb)| rgb),
                    memory_color: c.color_id,
                    active_loop: c.hot_cue == 0 && is_loop && active_times.contains(&c.time_ms),
                });
            }
        }
    } else {
        for list in dat.cue_lists()? {
            for c in list.cues {
                let is_loop = c.cue_type == CueType::Loop && c.loop_time_ms != NO_LOOP;
                out.push(CuePoint {
                    slot: c.hot_cue as u8,
                    time_ms: c.time_ms,
                    loop_end_ms: is_loop.then_some(c.loop_time_ms),
                    loop_beats: None,
                    comment: String::new(),
                    hot_color: 0,
                    hot_led: None,
                    memory_color: 0,
                    active_loop: c.status == 4,
                });
            }
        }
    }
    Ok(out)
}

fn pcpt(c: &CuePoint, index: usize, count: usize, linked: bool) -> Cue {
    let (first, last) = if linked {
        (
            if index == 0 { 0xffff } else { index as u16 - 1 },
            if index + 1 == count {
                0xffff
            } else {
                index as u16 + 1
            },
        )
    } else {
        (0xffff, 0xffff)
    };
    Cue {
        hot_cue: c.slot as u32,
        status: if c.active_loop { 4 } else { 0 },
        unknown: 0x10000,
        order_first: first,
        order_last: last,
        cue_type: if c.is_loop() {
            CueType::Loop
        } else {
            CueType::Point
        },
        pad: PAD,
        time_ms: c.time_ms,
        loop_time_ms: c.loop_end_ms.unwrap_or(NO_LOOP),
        tail: vec![0; 16],
    }
}

fn pcob(kind: CueListKind, cues: &[&CuePoint]) -> CueList {
    let linked = kind == CueListKind::Memory;
    CueList {
        kind,
        unknown: 0,
        memory_count: if linked && !cues.is_empty() {
            cues.len() as u32 - 1
        } else {
            0xffff_ffff
        },
        cues: cues
            .iter()
            .enumerate()
            .map(|(i, c)| pcpt(c, i, cues.len(), linked))
            .collect(),
    }
}

fn pcp2(c: &CuePoint) -> ExtCue {
    let (numerator, denominator) = c.loop_beats.unwrap_or((0, 0));
    let color = if c.slot == 0 {
        (0, [0, 0, 0])
    } else {
        let default = if c.is_loop() {
            DEFAULT_LOOP_LED
        } else {
            DEFAULT_LED
        };
        let led = c
            .hot_led
            .or_else(|| hot_cue_color(c.hot_color).map(|(_, led)| led))
            .unwrap_or(default);
        (c.hot_color, led)
    };
    ExtCue {
        hot_cue: c.slot as u32,
        cue_type: if c.is_loop() {
            CueType::Loop
        } else {
            CueType::Point
        },
        pad1: PAD,
        time_ms: c.time_ms,
        loop_time_ms: c.loop_end_ms.unwrap_or(NO_LOOP),
        color_id: c.memory_color,
        pad2: [1, 0, 0, 0, 0, 0, 0],
        loop_numerator: numerator,
        loop_denominator: denominator,
        comment: Some(c.comment.clone()),
        color: Some(color),
        tail: vec![0; 40],
    }
}

/// Replace the list of `tag` and `kind` in `file`, keeping its position.
fn put(file: &mut AnlzFile, section_tag: [u8; 4], kind: CueListKind, section: super::Section) {
    let pos = file.sections.iter().position(|s| {
        s.tag == section_tag
            && s.header.get(..4)
                == Some(&(if kind == CueListKind::Hot { 1u32 } else { 0 }).to_be_bytes()[..])
    });
    match pos {
        Some(i) => file.sections[i] = section,
        None => {
            // New lists go after the last cue section, or after the path.
            let after = file
                .sections
                .iter()
                .rposition(|s| s.tag == section_tag || s.tag == tag::CUES)
                .or_else(|| file.sections.iter().position(|s| s.tag == tag::PATH))
                .map(|i| i + 1)
                .unwrap_or(file.sections.len());
            file.sections.insert(after, section);
        }
    }
}

/// Write `cues` into the analysis files, replacing whatever cues they held.
pub fn write_cue_points(dat: &mut AnlzFile, ext: Option<&mut AnlzFile>, cues: &[CuePoint]) {
    let mut hot: Vec<&CuePoint> = cues.iter().filter(|c| c.slot > 0).collect();
    hot.sort_by(|a, b| b.slot.cmp(&a.slot));
    let mut memory: Vec<&CuePoint> = cues.iter().filter(|c| c.slot == 0).collect();
    memory.sort_by(|a, b| b.time_ms.cmp(&a.time_ms));
    let low: Vec<&CuePoint> = hot.iter().copied().filter(|c| c.slot <= 3).collect();
    let high: Vec<&CuePoint> = hot.iter().copied().filter(|c| c.slot > 3).collect();

    put(
        dat,
        tag::CUES,
        CueListKind::Memory,
        pcob(CueListKind::Memory, &memory).to_section(),
    );
    put(
        dat,
        tag::CUES,
        CueListKind::Hot,
        pcob(CueListKind::Hot, &low).to_section(),
    );

    if let Some(ext) = ext {
        put(
            ext,
            tag::CUES,
            CueListKind::Memory,
            pcob(CueListKind::Memory, &[]).to_section(),
        );
        put(
            ext,
            tag::CUES,
            CueListKind::Hot,
            pcob(CueListKind::Hot, &high).to_section(),
        );
        let all_hot: Vec<ExtCue> = high.iter().chain(low.iter()).map(|c| pcp2(c)).collect();
        put(
            ext,
            tag::CUES_2,
            CueListKind::Memory,
            ExtCueList {
                kind: CueListKind::Memory,
                unknown: 0,
                cues: memory.iter().map(|c| pcp2(c)).collect(),
            }
            .to_section(),
        );
        put(
            ext,
            tag::CUES_2,
            CueListKind::Hot,
            ExtCueList {
                kind: CueListKind::Hot,
                unknown: 0,
                cues: all_hot,
            }
            .to_section(),
        );
    }
}
