//! Player settings ("My Settings") the DJ keeps in the app and puts on
//! sticks, and formatting sticks for the players.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pioneer_formats::setting::{FIELDS, MySettings, SettingFile};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use usb_tools::format::{self, Disk, Filesystem};

use crate::commands::{CmdResult, blocking, err, timestamp};
use crate::dto;
use crate::jobs::Jobs;
use crate::library_cmds::{emit_job, lib, rekordbox_sources};

const PROFILE_KEY: &str = "player_settings";

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Where rekordbox keeps the settings it puts on sticks.
fn rekordbox_dir() -> PathBuf {
    home().join("Library/Application Support/Pioneer/rekordbox6")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect()
}

/// The DJ's settings: saved in the app, else rekordbox's, else defaults.
/// The second value says which.
pub(crate) fn profile(app: &AppHandle) -> CmdResult<(MySettings, &'static str)> {
    if let Some(json) = lib(app, |l| l.meta(PROFILE_KEY))?
        && let Ok(map) = serde_json::from_str::<BTreeMap<String, String>>(&json)
    {
        let mut s = MySettings::default();
        for f in SettingFile::ALL {
            if let Some(data) = map.get(f.file_name()).and_then(|h| unhex(h)) {
                let _ = s.set_data(f, data);
            }
        }
        return Ok((s, "profile"));
    }
    let (s, found) = MySettings::read_dir(&rekordbox_dir());
    Ok((
        s,
        if found.is_empty() {
            "default"
        } else {
            "rekordbox"
        },
    ))
}

fn save_profile(app: &AppHandle, s: &MySettings) -> CmdResult<()> {
    let map: BTreeMap<&str, String> = SettingFile::ALL
        .iter()
        .map(|&f| (f.file_name(), hex(s.data(f))))
        .collect();
    let json = serde_json::to_string(&map).map_err(err)?;
    lib(app, |l| l.set_meta(PROFILE_KEY, &json))
}

fn apply(s: &mut MySettings, values: &BTreeMap<String, u8>) -> CmdResult<()> {
    for (k, v) in values {
        s.set(k, *v).map_err(err)?;
    }
    Ok(())
}

fn values(s: &MySettings) -> BTreeMap<String, u8> {
    s.values()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn stick_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "flashka".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingOption {
    pub value: u8,
    pub label: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingField {
    pub key: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub options: Vec<SettingOption>,
    pub default: u8,
}

#[tauri::command]
pub fn settings_schema() -> Vec<SettingField> {
    FIELDS
        .iter()
        .map(|f| SettingField {
            key: f.key,
            group: f.group,
            label: f.label,
            options: f
                .options
                .iter()
                .map(|&(value, label)| SettingOption { value, label })
                .collect(),
            default: f.default,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub values: BTreeMap<String, u8>,
    /// `stick`, `profile` (saved in the app), `rekordbox`, `default`.
    pub source: &'static str,
    /// The DJ's profile, to compare a stick with.
    pub profile: BTreeMap<String, u8>,
    /// Settings files a player saved on the stick (`DEVSETTING.DAT (CDJ-3000)`).
    pub player_saved: Vec<String>,
}

/// The settings on `stick`, or the DJ's profile without a stick (or when
/// the stick has none).
#[tauri::command]
pub async fn settings_get(app: AppHandle, stick: Option<String>) -> CmdResult<SettingsDto> {
    blocking(move || {
        let (prof, source) = profile(&app)?;
        let mut dto = SettingsDto {
            values: values(&prof),
            source,
            profile: values(&prof),
            player_saved: Vec::new(),
        };
        if let Some(stick) = stick {
            let st = usb_tools::settings::read(Path::new(&stick));
            if !st.found.is_empty() {
                let mut s = prof.clone();
                for f in &st.found {
                    s.set_data(*f, st.settings.data(*f).to_vec()).map_err(err)?;
                }
                dto.values = values(&s);
                dto.source = "stick";
            }
            dto.player_saved = st
                .player_saved
                .iter()
                .map(|(f, model)| format!("{} ({model})", f.file_name()))
                .collect();
        }
        Ok(dto)
    })
    .await
}

/// Remember the settings as the DJ's profile.
#[tauri::command]
pub async fn settings_save(app: AppHandle, values: BTreeMap<String, u8>) -> CmdResult<()> {
    blocking(move || {
        let (mut s, _) = profile(&app)?;
        apply(&mut s, &values)?;
        save_profile(&app, &s)
    })
    .await
}

/// Write the settings onto a stick (old files go to the backups first) and
/// remember them as the profile.
#[tauri::command]
pub async fn settings_write(
    app: AppHandle,
    stick: String,
    values: BTreeMap<String, u8>,
) -> CmdResult<String> {
    blocking(move || {
        let stick = PathBuf::from(&stick);
        let (mut prof, _) = profile(&app)?;
        apply(&mut prof, &values)?;
        save_profile(&app, &prof)?;
        // Bytes we do not understand stay as the stick had them.
        let st = usb_tools::settings::read(&stick);
        let mut s = prof.clone();
        for f in &st.found {
            s.set_data(*f, st.settings.data(*f).to_vec()).map_err(err)?;
        }
        apply(&mut s, &values)?;
        let backup = home()
            .join("Stickwise/backups")
            .join(stick_name(&stick))
            .join(format!("settings-{}", timestamp()));
        usb_tools::settings::write(&stick, &s, &backup).map_err(err)?;
        let back = usb_tools::settings::read(&stick);
        if back.found.len() != SettingFile::ALL.len() || back.settings != s {
            return Err("Nastavení na flashce po zápisu nesedí, zkontroluj flashku.".into());
        }
        Ok(format!(
            "Nastavení je na flashce {}. Přehrávač ho nabídne po vložení flashky (MENU → MY SETTINGS).",
            stick_name(&stick)
        ))
    })
    .await
}

// ---- Formatting ---------------------------------------------------------

/// Disks formatting must never touch: the app's data, the library's music
/// and analysis, rekordbox's own library.
fn protected(app: &AppHandle) -> Vec<PathBuf> {
    // RB-EXPORT is the app's old name; older backups stay there.
    let mut out = vec![home().join("Stickwise"), home().join("RB-EXPORT")];
    if let Ok(d) = app.path().app_data_dir() {
        out.push(d);
    }
    out.extend(rekordbox_sources().into_iter().map(PathBuf::from));
    if let Ok(tracks) = lib(app, |l| l.tracks()) {
        let mut roots: Vec<PathBuf> = Vec::new();
        for p in tracks.iter().flat_map(|t| [&t.path, &t.anlz]) {
            let p = Path::new(p);
            // One path per volume is enough.
            let root: PathBuf = if p.starts_with("/Volumes") {
                p.components().take(3).collect()
            } else {
                p.components().take(2).collect()
            };
            if !root.as_os_str().is_empty() && !roots.contains(&root) {
                roots.push(root);
            }
        }
        out.extend(roots);
    }
    out
}

#[tauri::command]
pub async fn list_disks(app: AppHandle) -> CmdResult<Vec<Disk>> {
    blocking(move || format::list(&protected(&app)).map_err(err)).await
}

/// diskutil's progress lines, in Czech, with how far the erase is.
fn erase_step(line: &str) -> (String, f64) {
    let steps: [(&str, &str, f64); 7] = [
        ("Started erase", "Začínám", 0.10),
        ("Unmounting", "Odpojuji flashku", 0.15),
        ("Creating the partition map", "Vytvářím oddíl", 0.25),
        ("Waiting for partitions", "Čekám na oddíl", 0.35),
        ("Formatting", "Formátuji", 0.45),
        ("Mounting", "Připojuji flashku", 0.80),
        ("Finished erase", "Smazáno", 0.85),
    ];
    steps
        .iter()
        .find(|s| line.starts_with(s.0))
        .map_or((line.to_string(), -1.0), |s| (s.1.to_string(), s.2))
}

/// Erase a stick as FAT32 or exFAT and prepare it for the players. The
/// stick's old database and settings are backed up first.
#[tauri::command]
pub fn format_start(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    disk: String,
    filesystem: String,
    name: String,
    confirm: String,
    write_settings: bool,
) -> CmdResult<u32> {
    let fs = Filesystem::parse(&filesystem).ok_or("Neznámý souborový systém.")?;
    let name = format::volume_name(&name, fs).map_err(err)?;
    let settings = if write_settings {
        Some(profile(&app)?.0)
    } else {
        None
    };
    let (id, _cancel) = jobs.start();
    std::thread::spawn(move || {
        let mut ev = dto::JobEvent {
            id,
            label: format!("Formátování {name}"),
            state: "running",
            files_total: 1,
            bytes_total: 1,
            current: "Zálohuji databázi a nastavení".into(),
            phase: "backup".into(),
            fraction: Some(0.02),
            ..Default::default()
        };
        emit_job(&app, &ev);
        let result = (|| -> Result<(PathBuf, Option<PathBuf>), String> {
            let protected = protected(&app);
            let current = format::list(&protected)
                .map_err(err)?
                .into_iter()
                .find(|d| d.id == disk)
                .ok_or("Flashka už není připojená.")?;
            let backup = backup_before_format(&current);
            ev.phase = "erase".into();
            let mount = format::format(&disk, fs, &name, &confirm, &protected, |line| {
                let (text, f) = erase_step(line);
                let mut e = ev.clone();
                e.current = text;
                if f >= 0.0 {
                    e.fraction = Some(f);
                    ev.fraction = Some(f);
                }
                emit_job(&app, &e);
            })
            .map_err(err)?;
            ev.phase = "prepare".into();
            ev.current = "Připravuji flashku pro přehrávače".into();
            ev.fraction = Some(0.9);
            emit_job(&app, &ev);
            format::prepare(&mount, &name, &timestamp()[..10]).map_err(err)?;
            if let Some(s) = &settings {
                ev.phase = "settings".into();
                ev.current = "Zapisuji nastavení přehrávače".into();
                ev.fraction = Some(0.95);
                emit_job(&app, &ev);
                let tmp = std::env::temp_dir().join(format!("rbx-format-{id}"));
                usb_tools::settings::write(&mount, s, &tmp).map_err(err)?;
            }
            Ok((mount, backup))
        })();
        match result {
            Ok((mount, backup)) => {
                ev.state = "done";
                ev.fraction = Some(1.0);
                ev.files_done = 1;
                ev.bytes_done = 1;
                ev.dest = mount.display().to_string();
                let mut msg = format!(
                    "Flashka {name} je naformátovaná ({}) a připravená, můžeš na ni synchronizovat.",
                    match fs {
                        Filesystem::Fat32 => "FAT32",
                        Filesystem::ExFat => "exFAT",
                    }
                );
                if settings.is_some() {
                    msg += " Nastavení přehrávače je na ní.";
                }
                if let Some(b) = backup {
                    msg += &format!(" Stará databáze je v záloze {}.", b.display());
                }
                ev.message = msg;
            }
            Err(e) => {
                ev.state = "error";
                ev.message = e;
            }
        }
        emit_job(&app, &ev);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

/// Copy a stick's library database and settings to the backups, so a
/// stick formatted by mistake can at least be rebuilt.
fn backup_before_format(disk: &Disk) -> Option<PathBuf> {
    let mut saved = None;
    for v in &disk.volumes {
        let Some(mount) = v.mount_point.as_deref().map(Path::new) else {
            continue;
        };
        let files: Vec<PathBuf> = [
            "PIONEER/rekordbox/export.pdb",
            "PIONEER/rekordbox/exportExt.pdb",
            "PIONEER/rekordbox/exportLibrary.db",
            ".rbx/manifest.json",
        ]
        .iter()
        .map(PathBuf::from)
        .chain(
            SettingFile::ALL
                .iter()
                .map(|f| Path::new("PIONEER").join(f.file_name())),
        )
        .filter(|f| mount.join(f).is_file())
        .collect();
        if files.is_empty() {
            continue;
        }
        let dir = home()
            .join("Stickwise/backups")
            .join(stick_name(mount))
            .join(format!("format-{}", timestamp()));
        for f in files {
            let dst = dir.join(&f);
            if let Some(p) = dst.parent()
                && std::fs::create_dir_all(p).is_ok()
            {
                let _ = std::fs::copy(mount.join(&f), dst);
            }
        }
        saved = Some(dir);
    }
    saved
}
