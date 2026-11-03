//! Which player can play which file and read which filesystem.
//!
//! Rules follow Pioneer DJ / AlphaTheta spec sheets; anything not listed
//! there is treated as unsupported. To be confirmed on hardware.

use crate::probe::{AudioInfo, Codec, Container};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Player {
    /// CDJ-2000NXS, CDJ-900NXS and older.
    Cdj2000Nxs,
    /// CDJ-2000NXS2, XDJ-1000MK2, XDJ-XZ, XDJ-RX3.
    Cdj2000Nxs2,
    Cdj3000,
    /// CDJ-3000X, XDJ-AZ, OPUS-QUAD, OMNIS-DUO.
    Cdj3000X,
}

impl Player {
    pub const ALL: [Player; 4] = [
        Player::Cdj2000Nxs,
        Player::Cdj2000Nxs2,
        Player::Cdj3000,
        Player::Cdj3000X,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Player::Cdj2000Nxs => "CDJ-2000NXS",
            Player::Cdj2000Nxs2 => "CDJ-2000NXS2",
            Player::Cdj3000 => "CDJ-3000",
            Player::Cdj3000X => "CDJ-3000X",
        }
    }

    fn hi_res(self) -> bool {
        self != Player::Cdj2000Nxs
    }
}

/// `Err(reason)` when `player` cannot play a file like `info`.
pub fn can_play(player: Player, info: &AudioInfo) -> Result<(), String> {
    let rates: &[u32] = if player.hi_res() {
        &[44100, 48000, 88200, 96000]
    } else {
        &[44100, 48000]
    };
    let pcm_ok = |info: &AudioInfo| -> Result<(), String> {
        match info.bits {
            Some(16 | 24) => {}
            Some(b) => return Err(i18n::t!("usb.compat.bits", bits = b)),
            None => return Err(i18n::t!("usb.compat.bitsUnknown")),
        }
        match info.sample_rate {
            Some(r) if rates.contains(&r) => Ok(()),
            Some(r) => Err(i18n::t!("usb.compat.rate", rate = format!("{:.1}", r as f64 / 1000.0))),
            None => Err(i18n::t!("usb.compat.rateUnknown")),
        }
    };
    match (info.container, info.codec) {
        (Container::Mp3, _) | (Container::Mp4, _) => Ok(()),
        (Container::Wav | Container::Aiff, Codec::Pcm) => pcm_ok(info),
        (Container::Wav | Container::Aiff, Codec::Float) => Err(i18n::t!("usb.compat.float", bits = info.bits.unwrap_or(32))),
        (Container::Flac, _) if !player.hi_res() => Err(i18n::t!("usb.compat.flac")),
        (Container::Flac, _) => pcm_ok(info),
        (_, Codec::Other(t)) => Err(i18n::t!("usb.compat.codec", codec = format!("{t:#x}"))),
        _ => Err(i18n::t!("usb.compat.unknownFormat")),
    }
}

/// `Err(reason)` when `player` cannot mount a stick with this filesystem
/// (`statfs` type name: `msdos`, `exfat`, `hfs`, `apfs`, `ntfs`).
pub fn can_mount(player: Player, fs: &str) -> Result<(), String> {
    match (player, fs) {
        (_, "msdos") => Ok(()),
        (_, "hfs") => Ok(()),
        (Player::Cdj3000 | Player::Cdj3000X, "exfat") => Ok(()),
        (_, "exfat") => Err(i18n::t!("usb.compat.filesystem", fs = "exFAT")),
        (_, other) => Err(i18n::t!("usb.compat.filesystem", fs = other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(container: Container, codec: Codec, bits: u16, rate: u32) -> AudioInfo {
        AudioInfo {
            container,
            codec,
            bits: Some(bits),
            sample_rate: Some(rate),
            channels: Some(2),
            junk_after_tag: 0,
        }
    }

    #[test]
    fn float_wav_is_rejected_everywhere() {
        let f = info(Container::Wav, Codec::Float, 32, 44100);
        for p in Player::ALL {
            assert!(can_play(p, &f).is_err());
        }
    }

    #[test]
    fn hi_res_and_flac() {
        let w96 = info(Container::Wav, Codec::Pcm, 24, 96000);
        assert!(can_play(Player::Cdj3000, &w96).is_ok());
        assert!(can_play(Player::Cdj2000Nxs, &w96).is_err());
        let flac = info(Container::Flac, Codec::Flac, 24, 48000);
        assert!(can_play(Player::Cdj2000Nxs2, &flac).is_ok());
        assert!(can_play(Player::Cdj2000Nxs, &flac).is_err());
        let flac192 = info(Container::Flac, Codec::Flac, 24, 192000);
        assert!(can_play(Player::Cdj3000, &flac192).is_err());
    }

    #[test]
    fn filesystems() {
        assert!(can_mount(Player::Cdj3000, "exfat").is_ok());
        assert!(can_mount(Player::Cdj2000Nxs2, "exfat").is_err());
        assert!(can_mount(Player::Cdj2000Nxs2, "msdos").is_ok());
    }
}
