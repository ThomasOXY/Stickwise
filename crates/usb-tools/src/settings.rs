//! Player settings ("My Settings") on a stick: read them, and write them
//! the safe way (old files backed up, each new file replaced in one rename
//! and read back).

use std::path::Path;

use pioneer_formats::setting::{MySettings, SettingFile};

use crate::cue_write::replace_file;

/// What a stick carries.
#[derive(Debug, Clone)]
pub struct StickSettings {
    pub settings: MySettings,
    /// Files present and valid.
    pub found: Vec<SettingFile>,
    /// Files a player saved (its model, e.g. `CDJ-3000`), not rekordbox.
    pub player_saved: Vec<(SettingFile, String)>,
}

pub fn read(stick: &Path) -> StickSettings {
    let (settings, found) = MySettings::read_dir(&stick.join("PIONEER"));
    StickSettings {
        settings,
        player_saved: found
            .iter()
            .filter(|(_, p)| !p.software.is_empty() && p.software != "rekordbox")
            .map(|(f, p)| (*f, p.software.clone()))
            .collect(),
        found: found.into_iter().map(|(f, _)| f).collect(),
    }
}

/// Write all four files into `<stick>/PIONEER/`; the ones there before are
/// copied to `backup_dir` first.
pub fn write(stick: &Path, settings: &MySettings, backup_dir: &Path) -> std::io::Result<()> {
    let dir = stick.join("PIONEER");
    std::fs::create_dir_all(&dir)?;
    for f in SettingFile::ALL {
        let src = dir.join(f.file_name());
        if src.is_file() {
            std::fs::create_dir_all(backup_dir)?;
            std::fs::copy(&src, backup_dir.join(f.file_name()))?;
        }
    }
    for (name, bytes) in settings.files() {
        replace_file(&dir.join(name), &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_back() {
        let stick = tempfile::tempdir().unwrap();
        let backup = tempfile::tempdir().unwrap();
        let mut s = MySettings::default();
        s.set("needle_lock", 0x80).unwrap();
        write(stick.path(), &s, backup.path()).unwrap();
        let back = read(stick.path());
        assert_eq!(back.settings, s);
        assert_eq!(back.found.len(), 4);
        assert!(back.player_saved.is_empty());

        // Second write backs up the first.
        s.set("needle_lock", 0x81).unwrap();
        write(stick.path(), &s, backup.path()).unwrap();
        assert!(backup.path().join("MYSETTING.DAT").is_file());
        assert_eq!(read(stick.path()).settings.get("needle_lock"), Some(0x81));
    }
}
