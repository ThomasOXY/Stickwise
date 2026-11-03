//! Format a stick for the players: one MBR partition, FAT32 or exFAT, then
//! an empty rekordbox library so the stick shows up straight away.
//!
//! Formatting erases a whole disk, so a disk is refused unless all of this
//! holds: it is a USB disk or a memory card (never one of the Mac's own
//! disks, never a disk image), at most 2 TB (256 GB unless it is removable
//! media), and none of `protected` (the app's library, the
//! music on this Mac) lives on it. The caller also has to name the disk
//! (`confirm`) exactly as the DJ typed it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pioneer_formats::pdb::{self, Database, WriteOptions};
use serde::Serialize;
use serde_json::Value;

use crate::cue_write::replace_file;

const MAX_SIZE: u64 = 2_000_000_000_000;
/// External drives that are not removable media (SSDs, hard drives) are
/// offered only up to this size: anything bigger is a data drive.
const MAX_FIXED_SIZE: u64 = 256_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    Fat32,
    ExFat,
}

impl Filesystem {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fat32" => Some(Self::Fat32),
            "exfat" => Some(Self::ExFat),
            _ => None,
        }
    }

    fn diskutil(self) -> &'static str {
        match self {
            Self::Fat32 => "FAT32",
            Self::ExFat => "ExFAT",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub mount_point: Option<String>,
    /// `Windows_NTFS` (FAT/exFAT partitions), `Apple_HFS`, …
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    /// `disk6`.
    pub id: String,
    /// What the disk calls itself (`Samsung Flash Drive Media`).
    pub media: String,
    pub size: u64,
    pub bus: String,
    /// Removable media (a stick or card), not an external drive.
    pub removable: bool,
    pub volumes: Vec<Volume>,
    /// Why this disk may not be formatted.
    pub blocked: Option<String>,
}

impl Disk {
    /// The name the DJ types to confirm: the first named data volume's,
    /// else the id.
    pub fn confirm_name(&self) -> String {
        self.volumes
            .iter()
            .find(|v| !v.name.is_empty() && v.content != "EFI" && v.name != "EFI")
            .map(|v| v.name.clone())
            .unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{0}")]
    Refused(String),
    #[error("diskutil: {0}")]
    Diskutil(String),
    #[error("{0}")]
    Io(String),
}

fn plist(args: &[&str]) -> Result<Value, FormatError> {
    let out = Command::new("diskutil")
        .args(args)
        .output()
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    if !out.status.success() {
        return Err(FormatError::Diskutil(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let mut conv = Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    conv.stdin
        .take()
        .unwrap()
        .write_all(&out.stdout)
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    let json = conv
        .wait_with_output()
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    serde_json::from_slice(&json.stdout).map_err(|e| FormatError::Diskutil(e.to_string()))
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// `disk7s2` → `disk7`.
fn whole(id: &str) -> String {
    let rest = id.trim_start_matches("/dev/").trim_start_matches("disk");
    let n: String = rest.chars().take_while(char::is_ascii_digit).collect();
    format!("disk{n}")
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("disk")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
}

#[cfg(target_os = "macos")]
fn device_of(path: &Path) -> Option<String> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statfs fills the zeroed struct; f_mntfromname is NUL-terminated.
    unsafe {
        let mut st: libc::statfs = std::mem::zeroed();
        if libc::statfs(c.as_ptr(), &mut st) != 0 {
            return None;
        }
        Some(
            CStr::from_ptr(st.f_mntfromname.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(target_os = "macos"))]
fn device_of(_: &Path) -> Option<String> {
    None
}

/// Whole disks that hold any of `paths`, including the physical disks
/// under an APFS container.
fn disks_holding(paths: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for p in paths {
        // The deepest existing ancestor: a missing file still names its volume.
        let mut at = p.as_path();
        while !at.exists() {
            match at.parent() {
                Some(parent) => at = parent,
                None => break,
            }
        }
        let Some(dev) = device_of(at) else { continue };
        if !dev.starts_with("/dev/disk") {
            continue;
        }
        out.push(whole(&dev));
        if let Ok(info) = plist(&["info", "-plist", &dev]) {
            out.push(s(&info, "ParentWholeDisk"));
            for store in info
                .get("APFSPhysicalStores")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.push(whole(&s(store, "APFSPhysicalStore")));
            }
        }
    }
    out.retain(|d| !d.is_empty());
    out.sort();
    out.dedup();
    out
}

fn describe(
    id: &str,
    partitions: &[Value],
    protected_disks: &[String],
) -> Result<Option<Disk>, FormatError> {
    let info = plist(&["info", "-plist", id])?;
    let size = info.get("Size").and_then(Value::as_u64).unwrap_or(0);
    let bus = s(&info, "BusProtocol");
    let removable = b(&info, "RemovableMedia") || b(&info, "Removable");
    let card = bus.contains("Secure Digital") || bus == "SD";
    let image = bus == "Disk Image" && std::env::var_os("RBX_ALLOW_DISK_IMAGES").is_some();
    // Only sticks and cards are offered; the Mac's own disks never show up.
    let offered = bus == "USB" || card || image;
    let internal = b(&info, "Internal") && !card && !image;
    if !offered || internal {
        return Ok(None);
    }
    let blocked = if card && !removable {
        Some(i18n::t!("usb.builtInDisk"))
    } else if !b(&info, "WholeDisk") {
        Some(i18n::t!("usb.notWholeDisk"))
    } else if size > MAX_SIZE {
        Some(i18n::t!("usb.tooBig"))
    } else if !removable && !image && size > MAX_FIXED_SIZE {
        Some(i18n::t!("usb.dataDisk"))
    } else if protected_disks.iter().any(|d| d == id) {
        Some(i18n::t!("usb.hasLibrary"))
    } else {
        None
    };
    Ok(Some(Disk {
        id: id.to_string(),
        media: s(&info, "MediaName"),
        size,
        bus,
        removable,
        volumes: partitions
            .iter()
            .map(|p| Volume {
                name: s(p, "VolumeName"),
                mount_point: p
                    .get("MountPoint")
                    .and_then(Value::as_str)
                    .map(String::from),
                content: s(p, "Content"),
            })
            .collect(),
        blocked,
    }))
}

/// USB disks and memory cards, each with the reason it may not be formatted.
pub fn list(protected: &[PathBuf]) -> Result<Vec<Disk>, FormatError> {
    let all = plist(&["list", "-plist"])?;
    let protected_disks = disks_holding(protected);
    let mut out = Vec::new();
    for d in all
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = s(d, "DeviceIdentifier");
        if !valid_id(&id) || b(d, "OSInternal") {
            continue;
        }
        let parts = d
            .get("Partitions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        out.extend(describe(&id, &parts, &protected_disks)?);
    }
    Ok(out)
}

/// The name as it will be written: FAT32 wants upper case. At most 11
/// characters, letters, digits, space, `_` and `-`.
pub fn volume_name(name: &str, fs: Filesystem) -> Result<String, FormatError> {
    let name = name.trim();
    let name = match fs {
        Filesystem::Fat32 => name.to_uppercase(),
        Filesystem::ExFat => name.to_string(),
    };
    if name.is_empty() || name.chars().count() > 11 {
        return Err(FormatError::Refused(i18n::t!("usb.nameLength")));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
    {
        return Err(FormatError::Refused(i18n::t!("usb.nameChars")));
    }
    Ok(name)
}

/// Erase `disk` and make one `fs` partition named `name`. `progress` gets
/// diskutil's lines as it works. Returns where the new volume is mounted.
pub fn format(
    disk: &str,
    fs: Filesystem,
    name: &str,
    confirm: &str,
    protected: &[PathBuf],
    mut progress: impl FnMut(&str),
) -> Result<PathBuf, FormatError> {
    if !valid_id(disk) {
        return Err(FormatError::Refused(i18n::t!("usb.invalidDisk", disk = disk)));
    }
    let name = volume_name(name, fs)?;
    // Check again right before erasing: the list the DJ saw may be stale.
    let current = list(protected)?
        .into_iter()
        .find(|d| d.id == disk)
        .ok_or_else(|| FormatError::Refused(i18n::t!("usb.stickGone")))?;
    if let Some(why) = &current.blocked {
        return Err(FormatError::Refused(why.clone()));
    }
    if confirm != current.confirm_name() {
        return Err(FormatError::Refused(i18n::t!("usb.confirmMismatch")));
    }

    let mut child = Command::new("diskutil")
        .args([
            "eraseDisk",
            fs.diskutil(),
            &name,
            "MBRFormat",
            &format!("/dev/{disk}"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    let stdout = child.stdout.take().unwrap();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let line = line.trim();
        if !line.is_empty() {
            progress(line);
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| FormatError::Diskutil(e.to_string()))?;
    if !out.status.success() {
        return Err(FormatError::Diskutil(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }

    // The new partition: the disk's first, mounted by diskutil.
    let disks = plist(&["list", "-plist", disk])?;
    let mount = disks
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|d| d.get("Partitions"))
        .and_then(Value::as_array)
        .and_then(|p| p.first())
        .and_then(|p| p.get("MountPoint"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| FormatError::Io(i18n::t!("usb.notMounted")))?;
    Ok(mount)
}

/// Make a freshly formatted volume a stick the players (and this app)
/// recognise: keep Spotlight off it and write an empty library.
pub fn prepare(mount: &Path, device_name: &str, date: &str) -> std::io::Result<()> {
    std::fs::write(mount.join(".metadata_never_index"), b"")?;
    std::fs::create_dir_all(mount.join(".fseventsd"))?;
    std::fs::write(mount.join(".fseventsd/no_log"), b"")?;
    let rb = mount.join("PIONEER/rekordbox");
    std::fs::create_dir_all(&rb)?;
    std::fs::create_dir_all(mount.join("Contents"))?;
    let bytes = pdb::write(
        &Database::default(),
        &WriteOptions {
            device_name: device_name.to_string(),
            export_date: date.to_string(),
        },
    );
    if Database::parse(&bytes).map_or(true, |db| !db.problems.is_empty()) {
        return Err(std::io::Error::other("empty export.pdb does not parse"));
    }
    replace_file(&rb.join("export.pdb"), &bytes)?;
    remove_apple_double(mount);
    Ok(())
}

/// macOS keeps file metadata on FAT and exFAT in `._name` companion files,
/// which players list as broken tracks. `dot_clean -m` deletes them.
pub fn remove_apple_double(root: &Path) {
    let _ = Command::new("dot_clean").arg("-m").arg(root).status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(volume_name(" djset ", Filesystem::Fat32).unwrap(), "DJSET");
        assert_eq!(volume_name("djset", Filesystem::ExFat).unwrap(), "djset");
        assert!(volume_name("", Filesystem::Fat32).is_err());
        assert!(volume_name("TWELVECHARSX", Filesystem::Fat32).is_err());
        assert!(volume_name("ŠPALE", Filesystem::Fat32).is_err());
        assert!(volume_name("A/B", Filesystem::ExFat).is_err());
    }

    #[test]
    fn confirm_skips_efi() {
        let v = |name: &str, content: &str| Volume {
            name: name.into(),
            mount_point: None,
            content: content.into(),
        };
        let mut d = Disk {
            id: "disk9".into(),
            media: String::new(),
            size: 0,
            bus: "USB".into(),
            removable: false,
            volumes: vec![v("EFI", "EFI"), v("DATA", "Apple_HFS")],
            blocked: None,
        };
        assert_eq!(d.confirm_name(), "DATA");
        d.volumes.truncate(1);
        assert_eq!(d.confirm_name(), "disk9");
    }

    #[test]
    fn disk_ids() {
        assert_eq!(whole("/dev/disk7s2"), "disk7");
        assert_eq!(whole("disk12s1"), "disk12");
        assert!(valid_id("disk6"));
        assert!(!valid_id("disk6s1"));
        assert!(!valid_id("disk"));
        assert!(!valid_id("/dev/disk6"));
    }

    #[test]
    fn prepared_stick_has_an_empty_library() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), "TEST", "2026-10-14").unwrap();
        let db = Database::parse(
            &std::fs::read(dir.path().join("PIONEER/rekordbox/export.pdb")).unwrap(),
        )
        .unwrap();
        assert!(db.tracks.is_empty() && db.problems.is_empty());
        assert!(dir.path().join(".metadata_never_index").is_file());
    }
}
