//! The "My Settings" files we write must parse with rekordcrate, and the
//! fields we change must land where rekordcrate reads them.

use std::io::Cursor;

use binrw::BinRead;
use pioneer_formats::setting::{MySettings, SettingFile};
use rekordcrate::setting::{
    CrossfaderCurve, EjectLock, Setting, SettingData, SettingType, TempoRange, WaveformColor,
};

fn parse(name: &str, bytes: &[u8]) -> Setting {
    let ty = SettingType::from_filename(name).unwrap();
    Setting::read_args(&mut Cursor::new(bytes), (ty,)).unwrap()
}

#[test]
fn rekordcrate_reads_what_we_write() {
    let mut s = MySettings::default();
    s.set("eject_lock", 0x81).unwrap();
    s.set("tempo_range", 0x83).unwrap();
    s.set("waveform_color", 0x03).unwrap();
    s.set("crossfader_curve", 0x80).unwrap();
    for (name, bytes) in s.files() {
        let parsed = parse(name, &bytes);
        match (SettingFile::from_file_name(name).unwrap(), &parsed.data) {
            (SettingFile::My, SettingData::MySetting(m)) => {
                assert_eq!(m.eject_lock, EjectLock::Lock);
                assert_eq!(m.tempo_range, TempoRange::Wide);
            }
            (SettingFile::Dev, SettingData::DevSetting(d)) => {
                assert_eq!(d.waveform_color, WaveformColor::Rgb);
            }
            (SettingFile::Djm, SettingData::DJMMySetting(d)) => {
                assert_eq!(d.crossfader_curve, CrossfaderCurve::ConstantPower);
            }
            (SettingFile::My2, SettingData::MySetting2(_)) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn defaults_match_rekordcrate_defaults() {
    let s = MySettings::default();
    let ours = s.files();
    for (name, rc) in [
        ("MYSETTING.DAT", Setting::default_mysetting()),
        ("MYSETTING2.DAT", Setting::default_mysetting2()),
        ("DJMMYSETTING.DAT", Setting::default_djmmysetting()),
        ("DEVSETTING.DAT", Setting::default_devsetting()),
    ] {
        let mine = &ours.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(parse(name, mine).data, rc.data, "{name}");
    }
}
