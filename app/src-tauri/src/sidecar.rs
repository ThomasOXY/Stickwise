//! The local Python tools (rekordbox import, CUE-DETR) and the AI result
//! cache.
//!
//! The tools are a `uv` project. In the installed app its files ship as
//! resources and `uv` beside the app binary; on first use they are copied
//! to the app data folder and `uv sync` installs Python and the libraries
//! there (once, about 1 GB), so nothing is ever written inside the signed
//! app. A project checkout with a ready `sidecar/.venv` is used as is.
//! AI results are cached per track, so a track is only analysed once.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Candidate {
    /// Seconds.
    pub time: f64,
    /// 0..1, relative to the strongest candidate in the track.
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiCues {
    pub duration: f64,
    pub candidates: Vec<Candidate>,
}

pub struct Sidecar {
    /// The `uv` project the scripts run from.
    pub dir: PathBuf,
    pub data: PathBuf,
    /// Shipped project files, copied into `dir` before installing.
    bundled: Option<PathBuf>,
    /// The `uv` that installs the environment.
    uv: Option<PathBuf>,
    installing: Mutex<()>,
}

/// Files of the `uv` project the app ships.
const PROJECT_FILES: [&str; 4] = [
    "pyproject.toml",
    "uv.lock",
    "cue_detr.py",
    "rekordbox_dump.py",
];

impl Sidecar {
    /// `resources`: the app's resource folder (holding `sidecar/`).
    pub fn new(data: PathBuf, resources: Option<PathBuf>) -> Self {
        let installing = Mutex::new(());
        if let Some(dir) = std::env::var_os("RBX_SIDECAR").map(PathBuf::from) {
            return Sidecar {
                dir,
                data,
                bundled: None,
                uv: which_uv(),
                installing,
            };
        }
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sidecar");
        if checkout.join(".venv/bin/python").is_file() {
            return Sidecar {
                dir: checkout,
                data,
                bundled: None,
                uv: which_uv(),
                installing,
            };
        }
        let beside = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("uv")))
            .filter(|u| u.is_file());
        Sidecar {
            dir: data.join("sidecar"),
            bundled: resources
                .map(|r| r.join("sidecar"))
                .filter(|r| r.join("pyproject.toml").is_file()),
            uv: beside.or_else(which_uv),
            data,
            installing,
        }
    }

    fn python(&self) -> PathBuf {
        self.dir.join(".venv/bin/python")
    }

    pub fn script(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn installed(&self) -> bool {
        self.python().is_file() && self.dir.join("cue_detr.py").is_file() && !self.outdated()
    }

    /// Installed, or installable without the DJ doing anything.
    pub fn available(&self) -> bool {
        self.installed()
            || (self.uv.is_some()
                && (self.bundled.is_some() || self.dir.join("pyproject.toml").is_file()))
    }

    /// The shipped project differs from the installed one (app updated).
    fn outdated(&self) -> bool {
        let Some(src) = &self.bundled else {
            return false;
        };
        PROJECT_FILES
            .iter()
            .any(|f| std::fs::read(src.join(f)).ok() != std::fs::read(self.dir.join(f)).ok())
    }

    /// Install the tools if needed. `progress` gets `uv`'s lines.
    pub fn ensure(&self, mut progress: impl FnMut(&str)) -> Result<(), String> {
        let _guard = self.installing.lock().map_err(|e| e.to_string())?;
        if self.installed() {
            return Ok(());
        }
        let uv = self
            .uv
            .as_ref()
            .ok_or("Pomocné nástroje nejdou nainstalovat: chybí uv.")?;
        if let Some(src) = &self.bundled {
            std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
            for f in PROJECT_FILES {
                std::fs::copy(src.join(f), self.dir.join(f)).map_err(|e| format!("{f}: {e}"))?;
            }
        }
        let mut child = Command::new(uv)
            .args([
                "sync",
                "--frozen",
                "--no-progress",
                "--python-preference",
                "only-managed",
            ])
            .current_dir(&self.dir)
            .env("UV_PYTHON_INSTALL_DIR", self.data.join("python"))
            .env("UV_CACHE_DIR", self.data.join("uv-cache"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Instalaci nejde spustit: {e}"))?;
        let mut last = String::new();
        for line in BufReader::new(child.stderr.take().unwrap())
            .lines()
            .map_while(Result::ok)
        {
            let line = line.trim().to_string();
            if !line.is_empty() {
                progress(&line);
                last = line;
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() || !self.python().is_file() {
            return Err(format!("Instalace pomocných nástrojů selhala: {last}"));
        }
        // The environment has its own copies; the download cache is ~1 GB.
        let _ = std::fs::remove_dir_all(self.data.join("uv-cache"));
        Ok(())
    }

    fn cache_dir(&self) -> PathBuf {
        self.data.join("cue-detr")
    }

    /// Cache key: stick name, path on the stick and file size.
    pub fn key(stick: &str, file_path: &str, size: u64) -> String {
        // FNV-1a, enough to name a cache file.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{stick}|{file_path}|{size}").bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    }

    pub fn cached(&self, key: &str) -> Option<AiCues> {
        let b = std::fs::read(self.cache_dir().join(format!("{key}.json"))).ok()?;
        serde_json::from_slice(&b).ok()
    }

    fn store(&self, key: &str, cues: &AiCues) {
        let dir = self.cache_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(
                dir.join(format!("{key}.json")),
                serde_json::to_vec(cues).unwrap_or_default(),
            );
        }
    }

    /// Analyse `jobs` (cache key, audio path). `progress(done, current)` is
    /// called after each track. Returns the number analysed and the errors.
    pub fn run(
        &self,
        jobs: &[(String, PathBuf)],
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize, &str),
    ) -> Result<(usize, Vec<String>), String> {
        if !self.installed() {
            return Err("AI analýza není nainstalovaná.".into());
        }
        let by_path: HashMap<String, &String> = jobs
            .iter()
            .map(|(k, p)| (p.display().to_string(), k))
            .collect();
        let mut child = Command::new(self.python())
            .arg(self.dir.join("cue_detr.py"))
            .args(jobs.iter().map(|(_, p)| p))
            .env("HF_HOME", self.data.join("hf"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("AI analýzu nejde spustit: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let (mut done, mut errors) = (0, Vec::new());
        for line in BufReader::new(stdout).lines() {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let path = v["path"].as_str().unwrap_or_default().to_string();
            if let Some(e) = v["error"].as_str() {
                errors.push(format!("{path}: {e}"));
            } else if let (Some(key), Ok(cues)) = (
                by_path.get(&path),
                serde_json::from_value::<AiCues>(v.clone()),
            ) {
                self.store(key, &cues);
            }
            done += 1;
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            progress(done, &name);
        }
        let _ = child.wait();
        Ok((done, errors))
    }
}

fn which_uv() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    [
        home.join(".local/bin/uv"),
        home.join(".cargo/bin/uv"),
        PathBuf::from("/opt/homebrew/bin/uv"),
        PathBuf::from("/usr/local/bin/uv"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Install the tools if needed, showing it as a job in the sidebar.
pub fn ensure_installed(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    let sc = app.state::<Sidecar>();
    if sc.installed() {
        return Ok(());
    }
    let jobs = app.state::<crate::jobs::Jobs>();
    let (id, _) = jobs.start();
    let mut ev = crate::dto::JobEvent {
        id,
        label: "Instalace nástrojů (jednorázově, ~1 GB)".into(),
        state: "running",
        files_total: 1,
        bytes_total: 1,
        current: "Stahuji Python a knihovny".into(),
        phase: "install".into(),
        ..Default::default()
    };
    let _ = app.emit("job", ev.clone());
    let result = sc.ensure(|line| {
        let mut e = ev.clone();
        e.current = line.to_string();
        let _ = app.emit("job", e);
    });
    ev.state = if result.is_ok() { "done" } else { "error" };
    ev.files_done = 1;
    ev.bytes_done = 1;
    ev.message = match &result {
        Ok(()) => "Pomocné nástroje jsou připravené.".into(),
        Err(e) => e.clone(),
    };
    let _ = app.emit("job", ev);
    jobs.finish(id);
    result
}
