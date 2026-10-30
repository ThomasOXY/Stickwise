//! The "My Settings" files a stick carries in `PIONEER/`: player and mixer
//! preferences (`MYSETTING.DAT`, `MYSETTING2.DAT`, `DJMMYSETTING.DAT`) and
//! display preferences (`DEVSETTING.DAT`). A player offers to load them when
//! the stick goes in.
//!
//! Layout (all four files): `0x60` u32, brand / software / version as
//! NUL-padded 32-byte strings, payload length u32, payload, CRC16-XMODEM
//! u16, two zero bytes. The checksum covers the payload, except in
//! `DJMMYSETTING.DAT` where it covers everything before it.
//!
//! Field offsets follow rekordcrate's `setting.rs` (Holzhaus et al.) and were
//! checked against files rekordbox 7 and a CDJ-3000 wrote. Bytes whose
//! meaning is not known are carried over untouched: newer rekordbox versions
//! fill some of them.

use crate::{Error, Result};

const HEADER: usize = 0x68;

/// One of the four settings files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingFile {
    My,
    My2,
    Djm,
    Dev,
}

impl SettingFile {
    pub const ALL: [SettingFile; 4] = [Self::My, Self::My2, Self::Djm, Self::Dev];

    pub fn file_name(self) -> &'static str {
        match self {
            Self::My => "MYSETTING.DAT",
            Self::My2 => "MYSETTING2.DAT",
            Self::Djm => "DJMMYSETTING.DAT",
            Self::Dev => "DEVSETTING.DAT",
        }
    }

    pub fn from_file_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.file_name() == name)
    }

    /// Payload size in bytes.
    pub fn data_len(self) -> usize {
        match self {
            Self::My | Self::My2 => 40,
            Self::Djm => 52,
            Self::Dev => 32,
        }
    }

    fn brand(self) -> &'static str {
        match self {
            Self::My | Self::My2 => "PIONEER",
            Self::Djm => "PioneerDJ",
            Self::Dev => "PIONEER DJ",
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Djm => "1.000",
            _ => "0.001",
        }
    }

    /// The payload rekordbox writes for a fresh install.
    pub fn default_data(self) -> Vec<u8> {
        let mut d = vec![0u8; self.data_len()];
        match self {
            Self::My => {
                d[..8].copy_from_slice(&[0x78, 0x56, 0x34, 0x12, 0x02, 0, 0, 0]);
                d[13] = 0x01;
                d[17..20].copy_from_slice(&[0x01; 3]);
            }
            Self::My2 => d[11] = 0x81,
            Self::Djm => {
                d[..12].copy_from_slice(&[0x78, 0x56, 0x34, 0x12, 0x01, 0, 0, 0, 0x20, 0, 0, 0]);
            }
            Self::Dev => {
                d[..9].copy_from_slice(&[0x78, 0x56, 0x34, 0x12, 0x01, 0, 0, 0, 0x01]);
                d[11] = 0x01;
            }
        }
        for f in FIELDS.iter().filter(|f| f.file == self) {
            d[f.offset] = f.default;
        }
        d
    }
}

pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn header_string(b: &[u8], at: usize) -> String {
    let field = &b[at..at + 32];
    let end = field.iter().position(|&c| c == 0).unwrap_or(32);
    String::from_utf8_lossy(&field[..end]).trim().to_string()
}

/// A parsed settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub data: Vec<u8>,
    /// `rekordbox`, or the model of the player that saved the file.
    pub software: String,
    pub version: String,
}

/// Check the layout and checksum and return the payload.
pub fn parse(file: SettingFile, b: &[u8]) -> Result<Parsed> {
    let len = file.data_len();
    if b.len() != HEADER + len + 4 {
        return Err(Error::Invalid(format!(
            "{}: {} bytes, expected {}",
            file.file_name(),
            b.len(),
            HEADER + len + 4
        )));
    }
    if b[..4] != [0x60, 0, 0, 0]
        || u32::from_le_bytes(b[0x64..0x68].try_into().unwrap()) as usize != len
    {
        return Err(Error::Invalid(format!(
            "{}: unexpected header",
            file.file_name()
        )));
    }
    let stored = u16::from_le_bytes([b[HEADER + len], b[HEADER + len + 1]]);
    if stored != checksum(file, &b[..HEADER + len]) {
        return Err(Error::Invalid(format!(
            "{}: checksum mismatch",
            file.file_name()
        )));
    }
    Ok(Parsed {
        data: b[HEADER..HEADER + len].to_vec(),
        software: header_string(b, 0x24),
        version: header_string(b, 0x44),
    })
}

fn checksum(file: SettingFile, upto_data_end: &[u8]) -> u16 {
    match file {
        SettingFile::Djm => crc16_xmodem(upto_data_end),
        _ => crc16_xmodem(&upto_data_end[HEADER..]),
    }
}

/// The whole file for a payload, written the way rekordbox writes it.
pub fn build(file: SettingFile, data: &[u8]) -> Vec<u8> {
    assert_eq!(data.len(), file.data_len());
    let mut b = vec![0u8; HEADER];
    b[0] = 0x60;
    for (at, s) in [
        (0x04, file.brand()),
        (0x24, "rekordbox"),
        (0x44, file.version()),
    ] {
        b[at..at + s.len()].copy_from_slice(s.as_bytes());
    }
    b[0x64..0x68].copy_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(data);
    let crc = checksum(file, &b);
    b.extend_from_slice(&crc.to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    b
}

/// A setting the player menu offers, with the byte values it is stored as.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub file: SettingFile,
    /// Offset in the payload.
    pub offset: usize,
    pub group: &'static str,
    /// The name the players and rekordbox use.
    pub label: &'static str,
    pub options: &'static [(u8, &'static str)],
    pub default: u8,
}

const OFF_ON: &[(u8, &str)] = &[(0x80, "OFF"), (0x81, "ON")];
const UNLOCK_LOCK: &[(u8, &str)] = &[(0x80, "UNLOCK"), (0x81, "LOCK")];
const ONE_TO_FIVE: &[(u8, &str)] = &[
    (0x81, "1"),
    (0x82, "2"),
    (0x83, "3"),
    (0x84, "4"),
    (0x85, "5"),
];

pub const GROUP_PLAY: &str = "player";
pub const GROUP_DISPLAY: &str = "display";
pub const GROUP_LIGHT: &str = "light";
pub const GROUP_WAVE: &str = "waveform";
pub const GROUP_MIXER: &str = "mixer";

macro_rules! field {
    ($key:literal, $file:ident, $off:literal, $group:ident, $label:literal, $opts:expr, $def:literal) => {
        Field {
            key: $key,
            file: SettingFile::$file,
            offset: $off,
            group: $group,
            label: $label,
            options: $opts,
            default: $def,
        }
    };
}

pub const FIELDS: &[Field] = &[
    // Player: playback
    field!(
        "play_mode",
        My,
        23,
        GROUP_PLAY,
        "PLAY MODE / AUTO PLAY MODE",
        &[(0x80, "CONTINUE / ON"), (0x81, "SINGLE / OFF")],
        0x81
    ),
    field!(
        "eject_lock",
        My,
        21,
        GROUP_PLAY,
        "EJECT/LOAD LOCK",
        UNLOCK_LOCK,
        0x80
    ),
    field!(
        "needle_lock",
        My,
        29,
        GROUP_PLAY,
        "NEEDLE LOCK",
        UNLOCK_LOCK,
        0x81
    ),
    field!("quantize", My, 10, GROUP_PLAY, "QUANTIZE", OFF_ON, 0x81),
    field!(
        "quantize_beat_value",
        My,
        24,
        GROUP_PLAY,
        "QUANTIZE BEAT VALUE",
        &[(0x83, "1/8"), (0x82, "1/4"), (0x81, "1/2"), (0x80, "1")],
        0x80
    ),
    field!("sync", My, 22, GROUP_PLAY, "SYNC", OFF_ON, 0x80),
    field!(
        "master_tempo",
        My,
        35,
        GROUP_PLAY,
        "MASTER TEMPO",
        OFF_ON,
        0x80
    ),
    field!(
        "tempo_range",
        My,
        36,
        GROUP_PLAY,
        "TEMPO RANGE",
        &[
            (0x80, "±6%"),
            (0x81, "±10%"),
            (0x82, "±16%"),
            (0x83, "WIDE")
        ],
        0x81
    ),
    field!("auto_cue", My, 34, GROUP_PLAY, "AUTO CUE", OFF_ON, 0x81),
    field!(
        "auto_cue_level",
        My,
        11,
        GROUP_PLAY,
        "AUTO CUE LEVEL",
        &[
            (0x88, "MEMORY"),
            (0x80, "-36 dB"),
            (0x81, "-42 dB"),
            (0x82, "-48 dB"),
            (0x83, "-54 dB"),
            (0x84, "-60 dB"),
            (0x85, "-66 dB"),
            (0x86, "-72 dB"),
            (0x87, "-78 dB")
        ],
        0x88
    ),
    field!(
        "hotcue_autoload",
        My,
        25,
        GROUP_PLAY,
        "HOT CUE AUTO LOAD",
        &[(0x80, "OFF"), (0x81, "ON"), (0x82, "rekordbox SETTING")],
        0x81
    ),
    field!(
        "hotcue_color",
        My,
        26,
        GROUP_PLAY,
        "HOT CUE COLOR",
        OFF_ON,
        0x80
    ),
    field!(
        "beat_jump_beat_value",
        My2,
        12,
        GROUP_PLAY,
        "BEAT JUMP BEAT VALUE",
        &[
            (0x80, "1/2"),
            (0x81, "1"),
            (0x82, "2"),
            (0x83, "4"),
            (0x84, "8"),
            (0x85, "16"),
            (0x86, "32"),
            (0x87, "64")
        ],
        0x85
    ),
    field!(
        "jog_mode",
        My,
        33,
        GROUP_PLAY,
        "JOG MODE",
        &[(0x81, "VINYL"), (0x80, "CDJ")],
        0x81
    ),
    field!(
        "vinyl_speed_adjust",
        My2,
        0,
        GROUP_PLAY,
        "VINYL SPEED ADJUST",
        &[
            (0x80, "TOUCH & RELEASE"),
            (0x81, "TOUCH"),
            (0x82, "RELEASE")
        ],
        0x81
    ),
    // Player: display
    field!(
        "time_mode",
        My,
        32,
        GROUP_DISPLAY,
        "TIME MODE",
        &[(0x80, "ELAPSED"), (0x81, "REMAIN")],
        0x81
    ),
    field!(
        "language",
        My,
        12,
        GROUP_DISPLAY,
        "LANGUAGE",
        &[
            (0x81, "English"),
            (0x82, "Français"),
            (0x83, "Deutsch"),
            (0x84, "Italiano"),
            (0x85, "Nederlands"),
            (0x86, "Español"),
            (0x87, "Русский"),
            (0x88, "한국어"),
            (0x89, "简体中文"),
            (0x8a, "繁體中文"),
            (0x8b, "日本語"),
            (0x8c, "Português"),
            (0x8d, "Svenska"),
            (0x8e, "Čeština"),
            (0x8f, "Magyar"),
            (0x90, "Dansk"),
            (0x91, "Ελληνικά"),
            (0x92, "Türkçe"),
        ],
        0x81
    ),
    field!(
        "lcd_brightness",
        My,
        9,
        GROUP_DISPLAY,
        "LCD BRIGHTNESS",
        ONE_TO_FIVE,
        0x83
    ),
    field!(
        "jog_lcd_brightness",
        My2,
        3,
        GROUP_DISPLAY,
        "JOG LCD BRIGHTNESS",
        ONE_TO_FIVE,
        0x83
    ),
    field!(
        "jog_display_mode",
        My2,
        1,
        GROUP_DISPLAY,
        "JOG DISPLAY MODE",
        &[
            (0x80, "AUTO"),
            (0x81, "INFO"),
            (0x82, "SIMPLE"),
            (0x83, "ARTWORK")
        ],
        0x80
    ),
    // Player: lights
    field!(
        "on_air_display",
        My,
        8,
        GROUP_LIGHT,
        "ON AIR DISPLAY",
        OFF_ON,
        0x81
    ),
    field!(
        "jog_ring_brightness",
        My,
        14,
        GROUP_LIGHT,
        "JOG RING BRIGHTNESS",
        &[(0x80, "OFF"), (0x81, "1 (Dark)"), (0x82, "2 (Bright)")],
        0x82
    ),
    field!(
        "jog_ring_indicator",
        My,
        15,
        GROUP_LIGHT,
        "JOG RING INDICATOR",
        OFF_ON,
        0x81
    ),
    field!(
        "slip_flashing",
        My,
        16,
        GROUP_LIGHT,
        "SLIP FLASHING",
        OFF_ON,
        0x81
    ),
    field!(
        "disc_slot_illumination",
        My,
        20,
        GROUP_LIGHT,
        "DISC SLOT ILLUMINATION",
        &[(0x80, "OFF"), (0x81, "1 (Dark)"), (0x82, "2 (Bright)")],
        0x82
    ),
    field!(
        "pad_button_brightness",
        My2,
        2,
        GROUP_LIGHT,
        "PAD/BUTTON BRIGHTNESS",
        &[(0x81, "1"), (0x82, "2"), (0x83, "3"), (0x84, "4")],
        0x83
    ),
    // Waveform
    field!(
        "waveform_color",
        Dev,
        10,
        GROUP_WAVE,
        "WAVEFORM COLOR",
        &[(0x01, "BLUE"), (0x03, "RGB"), (0x04, "3BAND")],
        0x01
    ),
    field!(
        "waveform_position",
        Dev,
        13,
        GROUP_WAVE,
        "WAVEFORM CURRENT POSITION",
        &[(0x01, "CENTER"), (0x02, "LEFT")],
        0x01
    ),
    field!(
        "overview_waveform",
        Dev,
        9,
        GROUP_WAVE,
        "OVERVIEW WAVEFORM TYPE",
        &[(0x01, "HALF WAVEFORM"), (0x02, "FULL WAVEFORM")],
        0x01
    ),
    field!(
        "waveform_phase_meter",
        My2,
        10,
        GROUP_WAVE,
        "WAVEFORM / PHASE METER",
        &[(0x80, "WAVEFORM"), (0x81, "PHASE METER")],
        0x80
    ),
    field!(
        "waveform_divisions",
        My2,
        4,
        GROUP_WAVE,
        "WAVEFORM DIVISIONS",
        &[(0x80, "TIME SCALE"), (0x81, "PHRASE")],
        0x81
    ),
    field!(
        "phase_meter",
        My,
        37,
        GROUP_WAVE,
        "PHASE METER",
        &[(0x80, "TYPE 1"), (0x81, "TYPE 2")],
        0x80
    ),
    field!(
        "key_display",
        Dev,
        12,
        GROUP_WAVE,
        "KEY DISPLAY FORMAT",
        &[(0x01, "CLASSIC"), (0x02, "ALPHANUMERIC")],
        0x01
    ),
    // Mixer
    field!(
        "crossfader_curve",
        Djm,
        13,
        GROUP_MIXER,
        "CROSSFADER CURVE",
        &[
            (0x80, "CONSTANT POWER"),
            (0x81, "SLOW CUT"),
            (0x82, "FAST CUT")
        ],
        0x82
    ),
    field!(
        "ch_fader_curve",
        Djm,
        12,
        GROUP_MIXER,
        "CH FADER CURVE",
        &[
            (0x80, "STEEP TOP"),
            (0x81, "LINEAR"),
            (0x82, "STEEP BOTTOM")
        ],
        0x81
    ),
    field!(
        "ch_fader_curve_long",
        Djm,
        24,
        GROUP_MIXER,
        "CH FADER CURVE (LONG FADER)",
        &[(0x80, "EXPONENTIAL"), (0x81, "SMOOTH"), (0x82, "LINEAR")],
        0x80
    ),
    field!(
        "headphones_pre_eq",
        Djm,
        14,
        GROUP_MIXER,
        "HEADPHONES PRE EQ",
        &[(0x80, "POST EQ"), (0x81, "PRE EQ")],
        0x80
    ),
    field!(
        "headphones_mono_split",
        Djm,
        15,
        GROUP_MIXER,
        "HEADPHONES MONO SPLIT",
        &[(0x80, "STEREO"), (0x81, "MONO SPLIT")],
        0x80
    ),
    field!(
        "beat_fx_quantize",
        Djm,
        16,
        GROUP_MIXER,
        "BEAT FX QUANTIZE",
        OFF_ON,
        0x81
    ),
    field!(
        "mic_low_cut",
        Djm,
        17,
        GROUP_MIXER,
        "MIC LOW CUT",
        &[(0x80, "OFF"), (0x81, "ON (for MC)")],
        0x81
    ),
    field!(
        "talk_over_mode",
        Djm,
        18,
        GROUP_MIXER,
        "TALK OVER MODE",
        &[(0x80, "ADVANCED"), (0x81, "NORMAL")],
        0x80
    ),
    field!(
        "talk_over_level",
        Djm,
        19,
        GROUP_MIXER,
        "TALK OVER LEVEL",
        &[
            (0x80, "-24 dB"),
            (0x81, "-18 dB"),
            (0x82, "-12 dB"),
            (0x83, "-6 dB")
        ],
        0x81
    ),
    field!(
        "display_brightness",
        Djm,
        22,
        GROUP_MIXER,
        "BRIGHTNESS > DISPLAY",
        &[
            (0x80, "WHITE"),
            (0x81, "1"),
            (0x82, "2"),
            (0x83, "3"),
            (0x84, "4"),
            (0x85, "5")
        ],
        0x85
    ),
    field!(
        "indicator_brightness",
        Djm,
        23,
        GROUP_MIXER,
        "BRIGHTNESS > INDICATOR",
        &[(0x80, "1"), (0x81, "2"), (0x82, "3")],
        0x82
    ),
    field!(
        "midi_channel",
        Djm,
        20,
        GROUP_MIXER,
        "MIDI CH",
        &[
            (0x80, "1"),
            (0x81, "2"),
            (0x82, "3"),
            (0x83, "4"),
            (0x84, "5"),
            (0x85, "6"),
            (0x86, "7"),
            (0x87, "8"),
            (0x88, "9"),
            (0x89, "10"),
            (0x8a, "11"),
            (0x8b, "12"),
            (0x8c, "13"),
            (0x8d, "14"),
            (0x8e, "15"),
            (0x8f, "16"),
        ],
        0x80
    ),
    field!(
        "midi_button_type",
        Djm,
        21,
        GROUP_MIXER,
        "MIDI BUTTON TYPE",
        &[(0x80, "TOGGLE"), (0x81, "TRIGGER")],
        0x80
    ),
];

pub fn field(key: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.key == key)
}

/// The payloads of all four files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MySettings {
    data: [Vec<u8>; 4],
}

impl Default for MySettings {
    fn default() -> Self {
        Self {
            data: SettingFile::ALL.map(SettingFile::default_data),
        }
    }
}

fn index(file: SettingFile) -> usize {
    SettingFile::ALL.iter().position(|&f| f == file).unwrap()
}

impl MySettings {
    pub fn data(&self, file: SettingFile) -> &[u8] {
        &self.data[index(file)]
    }

    /// Replace one payload; wrong sizes are refused.
    pub fn set_data(&mut self, file: SettingFile, data: Vec<u8>) -> Result<()> {
        if data.len() != file.data_len() {
            return Err(Error::Invalid(format!(
                "{}: payload size",
                file.file_name()
            )));
        }
        self.data[index(file)] = data;
        Ok(())
    }

    /// The stored byte, or `None` for an unknown key.
    pub fn get(&self, key: &str) -> Option<u8> {
        let f = field(key)?;
        Some(self.data(f.file)[f.offset])
    }

    /// Set a field; values the field does not offer are refused.
    pub fn set(&mut self, key: &str, value: u8) -> Result<()> {
        let f = field(key).ok_or_else(|| Error::Invalid(format!("unknown setting {key}")))?;
        if !f.options.iter().any(|(v, _)| *v == value) {
            return Err(Error::Invalid(format!(
                "{key}: value {value:#04x} not allowed"
            )));
        }
        self.data[index(f.file)][f.offset] = value;
        Ok(())
    }

    /// Every known field with its value.
    pub fn values(&self) -> Vec<(&'static str, u8)> {
        FIELDS
            .iter()
            .map(|f| (f.key, self.data(f.file)[f.offset]))
            .collect()
    }

    /// The four files as they go into `PIONEER/`.
    pub fn files(&self) -> Vec<(&'static str, Vec<u8>)> {
        SettingFile::ALL
            .iter()
            .map(|&f| (f.file_name(), build(f, self.data(f))))
            .collect()
    }

    /// Read whatever valid settings files `dir` holds; the rest keeps the
    /// defaults. Returns the settings and the files that were read.
    pub fn read_dir(dir: &std::path::Path) -> (Self, Vec<(SettingFile, Parsed)>) {
        let mut s = Self::default();
        let mut found = Vec::new();
        for f in SettingFile::ALL {
            if let Ok(b) = std::fs::read(dir.join(f.file_name()))
                && let Ok(p) = parse(f, &b)
            {
                s.data[index(f)] = p.data.clone();
                found.push((f, p));
            }
        }
        (s, found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parse_roundtrip_with_valid_checksums() {
        let mut s = MySettings::default();
        s.set("eject_lock", 0x81).unwrap();
        s.set("waveform_color", 0x04).unwrap();
        s.set("crossfader_curve", 0x80).unwrap();
        for (name, bytes) in s.files() {
            let f = SettingFile::from_file_name(name).unwrap();
            let p = parse(f, &bytes).unwrap();
            assert_eq!(p.data, s.data(f));
            assert_eq!(p.software, "rekordbox");
        }
        assert_eq!(s.get("eject_lock"), Some(0x81));
        assert!(s.set("eject_lock", 0x05).is_err());
        assert!(s.set("nope", 0x80).is_err());
    }

    #[test]
    fn defaults_offer_their_own_values() {
        for f in FIELDS {
            assert!(f.offset < f.file.data_len(), "{}", f.key);
            assert!(f.options.iter().any(|(v, _)| *v == f.default), "{}", f.key);
        }
        let mut keys: Vec<_> = FIELDS
            .iter()
            .map(|f| (f.file.file_name(), f.offset))
            .collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), FIELDS.len(), "two fields share a byte");
    }

    #[test]
    fn checksum_matches_rekordbox() {
        // MYSETTING.DAT payload rekordbox 7 wrote; stored CRC 0x8d6f.
        let data: [u8; 40] = [
            0x78, 0x56, 0x34, 0x12, 0x02, 0, 0, 0, 0x81, 0x83, 0x81, 0x88, 0x81, 0x01, 0x82, 0x81,
            0x81, 0x01, 0x01, 0x01, 0x82, 0x81, 0x80, 0x81, 0x82, 0x81, 0x81, 0, 0, 0x81, 0, 0,
            0x81, 0x80, 0x81, 0x81, 0x81, 0x80, 0, 0,
        ];
        let b = build(SettingFile::My, &data);
        assert_eq!(&b[b.len() - 4..], &[0x6f, 0x8d, 0, 0]);

        // DJMMYSETTING.DAT: the checksum also covers the header; stored 0x6c38.
        let mut djm = vec![0u8; 52];
        djm[..25].copy_from_slice(&[
            0x78, 0x56, 0x34, 0x12, 0x01, 0, 0, 0, 0x20, 0, 0, 0, 0x81, 0x82, 0x80, 0x80, 0x81,
            0x81, 0x80, 0x81, 0x80, 0x80, 0x85, 0x82, 0x80,
        ]);
        let b = build(SettingFile::Djm, &djm);
        assert_eq!(&b[b.len() - 4..], &[0x38, 0x6c, 0, 0]);
    }
}
