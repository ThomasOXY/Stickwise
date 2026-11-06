//! Share a playlist to the DJ's own server (the PHP app in `cloud/`), where
//! people with the link can listen to it. Tracks are converted to something
//! small and uploaded once; the playlist goes live only after all its audio
//! is there, so a link keeps working while it is updated or if a share fails.
//!
//! The server's token stays on this side, in a file only the DJ can read.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::blocking::{Body, Client, RequestBuilder, Response};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::commands::{CmdResult, blocking, err};
use crate::dto;
use crate::jobs::Jobs;
use crate::library_cmds::{emit_job, lib};

const AFCONVERT: &str = "/usr/bin/afconvert";
/// What listeners hear. Part of every track's key: changing it uploads all anew.
const CONVERSION: &str = "aac@44100,192k";
const BITRATE_KBPS: u32 = 192;
const TOKEN_HEADER: &str = "X-Stickwise-Token";

#[derive(Debug)]
enum CloudError {
    /// The server does not accept the token any more.
    SignedOut,
    Failed(String),
}

impl From<String> for CloudError {
    fn from(message: String) -> Self {
        CloudError::Failed(message)
    }
}

impl From<io::Error> for CloudError {
    fn from(e: io::Error) -> Self {
        CloudError::Failed(e.to_string())
    }
}

type Result<T> = std::result::Result<T, CloudError>;

#[derive(Serialize, Deserialize, Clone)]
struct Account {
    url: String,
    username: String,
    token: String,
}

/// What the Share sheet shows about the sign-in.
#[derive(Serialize)]
pub struct CloudAccount {
    url: String,
    username: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudPlaylist {
    url: String,
    has_password: bool,
    tracks: usize,
}

/// The server's address as it should be used: no trailing slash or file, and
/// https, as a password and a token travel over it. Plain http only to this Mac.
fn normalize_url(input: &str) -> std::result::Result<String, String> {
    let mut url = input.trim().trim_end_matches('/').to_string();
    for file in ["/index.php", "/api.php", "/install.php"] {
        if let Some(rest) = url.strip_suffix(file) {
            url = rest.to_string();
        }
    }
    if url.is_empty() {
        return Err(i18n::t!("cloud.badAddress"));
    }
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let parsed = reqwest::Url::parse(&url).map_err(|_| i18n::t!("cloud.badAddress"))?;
    let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1"));
    match parsed.scheme() {
        "https" => Ok(url),
        "http" if local => Ok(url),
        "http" => Err(i18n::t!("cloud.needsHttps")),
        _ => Err(i18n::t!("cloud.badAddress")),
    }
}

fn client(timeout: Option<Duration>) -> Result<Client> {
    // The TLS library wants its crypto chosen once per process; a second call is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        // A redirect could carry the token to another address: report it instead.
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(timeout)
        .build()
        .map_err(|e| CloudError::Failed(e.to_string()))
}

fn send_failed(e: reqwest::Error) -> CloudError {
    let e = e.without_url();
    let mut text = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        text = format!("{text}: {s}");
        source = s.source();
    }
    CloudError::Failed(i18n::t!("cloud.unreachable", error = text))
}

fn send(request: RequestBuilder) -> Result<Value> {
    read_reply(request.send().map_err(send_failed)?)
}

fn read_reply(response: Response) -> Result<Value> {
    let status = response.status();
    if status.is_redirection() {
        let to = response.headers().get("location").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        return Err(CloudError::Failed(i18n::t!("cloud.redirected", to = to)));
    }
    let text = response.text().map_err(send_failed)?;
    let body: Option<Value> = serde_json::from_str(&text).ok();
    let code = body.as_ref().and_then(|b| b["error"].as_str()).map(str::to_string);
    if status.is_success() && code.is_none() {
        return body.ok_or_else(|| CloudError::Failed(i18n::t!("cloud.notStickwise")));
    }
    Err(match code.as_deref() {
        Some("unauthorized") => CloudError::SignedOut,
        Some("bad_login") => CloudError::Failed(i18n::t!("cloud.badLogin")),
        Some("too_many_attempts") => CloudError::Failed(i18n::t!("cloud.tooManyAttempts")),
        Some("too_large") => CloudError::Failed(i18n::t!("cloud.tooLargeOnServer")),
        Some("storage_not_writable") => CloudError::Failed(i18n::t!("cloud.storageNotWritable")),
        Some("upload_damaged") => CloudError::Failed(i18n::t!("cloud.uploadDamaged")),
        Some("track_missing") => CloudError::Failed(i18n::t!("cloud.trackMissing")),
        Some("bad_password") => CloudError::Failed(i18n::t!("cloud.badPassword")),
        _ if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE => CloudError::Failed(i18n::t!("cloud.tooLargeOnServer")),
        _ if body.is_none() && status.is_success() => CloudError::Failed(i18n::t!("cloud.notStickwise")),
        _ => CloudError::Failed(i18n::t!("cloud.serverError", status = status.as_u16())),
    })
}

fn call(account: &Account, action: &str, body: Value) -> Result<Value> {
    send(
        client(Some(Duration::from_secs(60)))?
            .post(format!("{}/api.php?action={action}", account.url))
            .header(TOKEN_HEADER, &account.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string()),
    )
}

fn playlist_from(account: &Account, reply: &Value) -> Result<Option<CloudPlaylist>> {
    let p = &reply["playlist"];
    if p.is_null() {
        return Ok(None);
    }
    let slug = p["slug"].as_str().ok_or_else(|| CloudError::Failed(i18n::t!("cloud.notStickwise")))?;
    Ok(Some(CloudPlaylist {
        url: format!("{}/index.php?p={slug}", account.url),
        has_password: p["hasPassword"].as_bool().unwrap_or(false),
        tracks: p["tracks"].as_u64().unwrap_or(0) as usize,
    }))
}

fn saved_playlist(account: &Account, reply: &Value) -> Result<CloudPlaylist> {
    playlist_from(account, reply)?.ok_or_else(|| CloudError::Failed(i18n::t!("cloud.notStickwise")))
}

fn sign_in(url: &str, username: &str, password: &str) -> Result<Account> {
    let url = normalize_url(url)?;
    let client = client(Some(Duration::from_secs(60)))?;
    // First ask if this is the app at all, for a clearer error than a failed sign-in.
    let ping = send(client.get(format!("{url}/api.php?action=ping")))?;
    if ping["app"] != "stickwise-cloud" {
        return Err(CloudError::Failed(i18n::t!("cloud.notStickwise")));
    }
    let reply = send(
        client
            .post(format!("{url}/api.php?action=login"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(json!({ "username": username, "password": password, "label": "Stickwise" }).to_string()),
    )?;
    let token = reply["token"].as_str().ok_or_else(|| CloudError::Failed(i18n::t!("cloud.notStickwise")))?;
    Ok(Account { url, username: username.to_string(), token: token.to_string() })
}

/// One track to share.
struct Source {
    path: PathBuf,
    title: String,
    artist: String,
    duration: f64,
    bitrate: u32,
}

struct ShareRequest {
    client_ref: String,
    title: String,
    /// `None` keeps what the playlist has.
    password: Option<String>,
    sources: Vec<Source>,
}

struct Shared {
    playlist: CloudPlaylist,
    /// Tracks left out: file gone or impossible to convert.
    skipped: Vec<String>,
}

/// Progress: the whole job 0 to 1, tracks done, what is going on.
type Progress = Arc<dyn Fn(f64, usize, String) + Send + Sync>;

fn sha256_of(path: &Path, prefix: &str, cancel: &AtomicBool) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CloudError::Failed(i18n::t!("cloud.stopped")));
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// A file small enough already is sent as it is: converting it again only loses quality.
fn keeps_original(path: &Path, bitrate_kbps: u32) -> Option<&'static str> {
    if bitrate_kbps == 0 || bitrate_kbps > BITRATE_KBPS {
        return None;
    }
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp3" => Some("mp3"),
        "m4a" => Some("m4a"),
        _ => None,
    }
}

/// Run afconvert until done; `false` if it failed. Stopped by `cancel`.
fn convert(source: &Path, out: &Path, cancel: &AtomicBool) -> Result<bool> {
    let mut child = Command::new(AFCONVERT)
        .args(["-f", "m4af", "-d", "aac@44100", "-b", &(BITRATE_KBPS * 1000).to_string()])
        .arg(source)
        .arg(out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CloudError::Failed(i18n::t!("cloud.stopped")));
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status.success() && out.metadata().is_ok_and(|m| m.len() > 0));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Reads the file for the upload, reporting how far it is and stopping on `cancel`.
struct UploadReader<F> {
    file: fs::File,
    sent: u64,
    cancel: Arc<AtomicBool>,
    on_read: F,
}

impl<F: FnMut(u64)> Read for UploadReader<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other(i18n::t!("cloud.stopped")));
        }
        let n = self.file.read(buf)?;
        self.sent += n as u64;
        (self.on_read)(self.sent);
        Ok(n)
    }
}

fn upload(
    account: &Account,
    key: &str,
    format: &str,
    path: &Path,
    cancel: &Arc<AtomicBool>,
    on_read: impl FnMut(u64) + Send + 'static,
) -> Result<()> {
    let sha256 = sha256_of(path, "", cancel)?;
    let file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let reader = UploadReader { file, sent: 0, cancel: cancel.clone(), on_read };
    send(
        // No overall timeout: a big file on a slow line takes what it takes.
        client(None)?
            .post(format!("{}/api.php?action=upload&key={key}&format={format}&sha256={sha256}", account.url))
            .header(TOKEN_HEADER, &account.token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::sized(reader, len)),
    )?;
    Ok(())
}

/// Bring the server's audio up to date and publish the playlist.
fn share(account: &Account, request: ShareRequest, cancel: &Arc<AtomicBool>, progress: &Progress) -> Result<Shared> {
    let total = request.sources.len();
    let mut skipped = Vec::new();
    // Each playable track with its key.
    let mut tracks = Vec::new();
    for (i, s) in request.sources.iter().enumerate() {
        progress(0.1 * i as f64 / total as f64, 0, i18n::t!("cloud.checking", name = s.title));
        if !s.path.is_file() {
            skipped.push(s.title.clone());
            continue;
        }
        tracks.push((s, sha256_of(&s.path, &format!("{CONVERSION}\n"), cancel)?));
    }
    if tracks.is_empty() {
        return Err(CloudError::Failed(i18n::t!("cloud.noAudio")));
    }

    let keys: Vec<&String> = tracks.iter().map(|(_, k)| k).collect();
    let missing = call(account, "missing", json!({ "keys": keys }))?;
    let mut missing: HashSet<String> = missing["missing"]
        .as_array()
        .map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let max_upload = send(client(Some(Duration::from_secs(60)))?.get(format!("{}/api.php?action=ping", account.url)))?["maxUpload"]
        .as_u64()
        .unwrap_or(0);

    let dir = tempfile::tempdir()?;
    let span = 0.85 / tracks.len() as f64;
    let mut failed: HashSet<&str> = HashSet::new();
    for (i, (s, key)) in tracks.iter().enumerate() {
        let base = 0.1 + span * i as f64;
        if !missing.remove(key.as_str()) {
            progress(base + span, i + 1, i18n::t!("cloud.alreadyThere", name = s.title));
            continue;
        }
        let converted = dir.path().join(format!("{i}.m4a"));
        let (path, format): (&Path, &str) = match keeps_original(&s.path, s.bitrate) {
            Some(format) => (s.path.as_path(), format),
            None => {
                progress(base, i, i18n::t!("cloud.converting", name = s.title));
                if !convert(&s.path, &converted, cancel)? {
                    skipped.push(s.title.clone());
                    failed.insert(key);
                    continue;
                }
                (converted.as_path(), "m4a")
            }
        };
        let len = path.metadata()?.len();
        if max_upload > 0 && len > max_upload {
            return Err(CloudError::Failed(i18n::t!(
                "cloud.tooLarge",
                name = s.title,
                size = len / 1_000_000 + 1,
                limit = max_upload / 1_000_000
            )));
        }
        let (progress_now, text) = (progress.clone(), i18n::t!("cloud.uploading", name = s.title));
        let mut reported = Instant::now();
        upload(account, key, format, path, cancel, move |sent| {
            if reported.elapsed() > Duration::from_millis(150) {
                reported = Instant::now();
                let part = (sent as f64 / len.max(1) as f64).min(1.0);
                progress_now(base + span * (0.3 + 0.7 * part), i, text.clone());
            }
        })?;
        let _ = fs::remove_file(&converted);
        progress(base + span, i + 1, i18n::t!("cloud.uploading", name = s.title));
    }

    let items: Vec<Value> = tracks
        .iter()
        .filter(|(_, key)| !failed.contains(key.as_str()))
        .map(|(s, key)| json!({ "key": key, "title": s.title, "artist": s.artist, "duration": s.duration.round() as u64 }))
        .collect();
    if items.is_empty() {
        return Err(CloudError::Failed(i18n::t!("cloud.noAudio")));
    }
    progress(0.97, tracks.len(), i18n::t!("cloud.publishing"));
    let mut body = json!({ "ref": request.client_ref, "title": request.title, "tracks": items });
    if let Some(password) = &request.password {
        body["password"] = json!(password);
    }
    let reply = call(account, "playlist_save", body)?;
    Ok(Shared { playlist: saved_playlist(account, &reply)?, skipped })
}

fn account_file(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_data_dir().map_err(err)?.join("cloud.json"))
}

fn load_account(app: &AppHandle) -> CmdResult<Option<Account>> {
    match fs::read(account_file(app)?) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(e)),
    }
}

fn save_account(app: &AppHandle, account: &Account) -> CmdResult<()> {
    let path = account_file(app)?;
    // A file from before keeps its mode: this one holds the token, so start clean.
    let _ = fs::remove_file(&path);
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(err)?;
    file.write_all(&serde_json::to_vec(account).map_err(err)?).map_err(err)
}

fn require_account(app: &AppHandle) -> CmdResult<Account> {
    load_account(app)?.ok_or_else(|| i18n::t!("cloud.signedOut"))
}

/// The message for the DJ. A refused token also ends the sign-in, so the sheet asks for a new one.
fn message(app: &AppHandle, e: CloudError) -> String {
    match e {
        CloudError::SignedOut => {
            if let Ok(path) = account_file(app) {
                let _ = fs::remove_file(path);
            }
            i18n::t!("cloud.signedOut")
        }
        CloudError::Failed(text) => text,
    }
}

#[tauri::command]
pub fn cloud_status(app: AppHandle) -> CmdResult<Option<CloudAccount>> {
    Ok(load_account(&app)?.map(|a| CloudAccount { url: a.url, username: a.username }))
}

#[tauri::command]
pub async fn cloud_login(app: AppHandle, url: String, username: String, password: String) -> CmdResult<CloudAccount> {
    blocking(move || {
        let account = sign_in(&url, &username, &password).map_err(|e| message(&app, e))?;
        save_account(&app, &account)?;
        Ok(CloudAccount { url: account.url, username: account.username })
    })
    .await
}

#[tauri::command]
pub async fn cloud_logout(app: AppHandle) -> CmdResult<()> {
    blocking(move || {
        if let Some(account) = load_account(&app)? {
            // The server forgets the token if it can be reached; the sign-in ends here either way.
            let _ = call(&account, "logout", json!({}));
        }
        match fs::remove_file(account_file(&app)?) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(err(e)),
            _ => Ok(()),
        }
    })
    .await
}

/// How the playlist stands on the server, `None` if it is not shared.
#[tauri::command]
pub async fn cloud_playlist(app: AppHandle, playlist_id: i64) -> CmdResult<Option<CloudPlaylist>> {
    blocking(move || {
        let account = require_account(&app)?;
        call(&account, "playlist_get", json!({ "ref": playlist_id.to_string() }))
            .and_then(|reply| playlist_from(&account, &reply))
            .map_err(|e| message(&app, e))
    })
    .await
}

/// `password: None` removes it.
#[tauri::command]
pub async fn cloud_set_password(app: AppHandle, playlist_id: i64, password: Option<String>) -> CmdResult<CloudPlaylist> {
    blocking(move || {
        let account = require_account(&app)?;
        call(&account, "playlist_password", json!({ "ref": playlist_id.to_string(), "password": password }))
            .and_then(|reply| saved_playlist(&account, &reply))
            .map_err(|e| message(&app, e))
    })
    .await
}

/// Take the playlist off the server, with the audio nothing else there plays.
#[tauri::command]
pub async fn cloud_unshare(app: AppHandle, playlist_id: i64) -> CmdResult<()> {
    blocking(move || {
        let account = require_account(&app)?;
        call(&account, "playlist_delete", json!({ "ref": playlist_id.to_string() }))
            .map(|_| ())
            .map_err(|e| message(&app, e))
    })
    .await
}

/// Share or update the playlist as a job. `password: None` keeps the current one.
#[tauri::command]
pub fn cloud_share(app: AppHandle, jobs: State<'_, Jobs>, playlist_id: i64, password: Option<String>) -> CmdResult<u32> {
    let account = require_account(&app)?;
    let (title, sources) = lib(&app, |l| {
        let list = l.playlists()?.into_iter().find(|p| p.id == playlist_id);
        let Some(list) = list else { return Ok((String::new(), Vec::new())) };
        let mut sources = Vec::new();
        for id in &list.track_ids {
            if let Some(t) = l.track(*id)? {
                sources.push(Source {
                    path: PathBuf::from(t.path),
                    title: t.title,
                    artist: t.artist,
                    duration: t.duration,
                    bitrate: t.bitrate,
                });
            }
        }
        Ok((list.name, sources))
    })?;
    if sources.is_empty() {
        return Err(i18n::t!("cloud.emptyPlaylist"));
    }
    let (id, cancel) = jobs.start();
    let event = dto::JobEvent {
        id,
        label: i18n::t!("cloud.jobLabel", name = title),
        state: "running",
        files_total: sources.len(),
        phase: "cloud".into(),
        fraction: Some(0.0),
        ..Default::default()
    };
    emit_job(&app, &event);
    std::thread::spawn(move || {
        let progress: Progress = {
            let (app, event) = (app.clone(), event.clone());
            Arc::new(move |fraction, done, current| {
                emit_job(&app, &dto::JobEvent { fraction: Some(fraction), files_done: done, current, ..event.clone() })
            })
        };
        let request = ShareRequest { client_ref: playlist_id.to_string(), title, password, sources };
        let result = share(&account, request, &cancel, &progress);
        let mut last = event;
        (last.state, last.message) = match result {
            _ if cancel.load(Ordering::Relaxed) => ("cancelled", i18n::t!("cloud.stopped")),
            Ok(s) if s.skipped.is_empty() => ("done", i18n::t!("cloud.done", count = s.playlist.tracks)),
            Ok(s) => (
                "done",
                i18n::t!("cloud.doneSkipped", count = s.playlist.tracks, skipped = s.skipped.len(), names = s.skipped.join(", ")),
            ),
            Err(e) => ("error", message(&app, e)),
        };
        last.fraction = Some(1.0);
        emit_job(&app, &last);
        app.state::<Jobs>().finish(id);
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_is_cleaned_up() {
        assert_eq!(normalize_url(" domena.cz/share/ ").unwrap(), "https://domena.cz/share");
        assert_eq!(normalize_url("https://domena.cz/share/index.php").unwrap(), "https://domena.cz/share");
        assert_eq!(normalize_url("http://localhost:8000").unwrap(), "http://localhost:8000");
    }

    #[test]
    fn plain_http_is_refused_except_on_this_mac() {
        assert!(normalize_url("http://domena.cz").is_err());
        assert!(normalize_url("http://localhost.domena.cz").is_err());
        assert!(normalize_url("ftp://domena.cz").is_err());
        assert!(normalize_url("").is_err());
    }

    #[test]
    fn only_small_mp3_and_m4a_skip_the_conversion() {
        assert_eq!(keeps_original(Path::new("/a/b.MP3"), 192), Some("mp3"));
        assert_eq!(keeps_original(Path::new("/a/b.m4a"), 128), Some("m4a"));
        assert_eq!(keeps_original(Path::new("/a/b.mp3"), 320), None);
        // Unknown bitrate: could be anything.
        assert_eq!(keeps_original(Path::new("/a/b.mp3"), 0), None);
        assert_eq!(keeps_original(Path::new("/a/b.wav"), 128), None);
        assert_eq!(keeps_original(Path::new("/a/b"), 128), None);
    }

    /// The whole share against a real server. `scripts/test-cloud.sh` runs it:
    /// CLOUD_TEST_URL, CLOUD_TEST_USER and CLOUD_TEST_PASSWORD say which.
    #[test]
    #[ignore]
    fn shares_against_a_real_server() {
        let var = |k: &str| std::env::var(k).unwrap();
        let account = sign_in(&var("CLOUD_TEST_URL"), &var("CLOUD_TEST_USER"), &var("CLOUD_TEST_PASSWORD")).unwrap();

        // Twelve seconds of noise as a 24-bit, 96 kHz WAV: conversion has work to do.
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("loud.wav");
        write_noise_wav(&wav, 12);
        let source = |path: PathBuf, title: &str| Source { path, title: title.into(), artist: "Ž".into(), duration: 12.4, bitrate: 0 };
        let request = |sources| ShareRequest { client_ref: "rust-test".into(), title: "Rust test".into(), password: Some("heslo".into()), sources };

        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress: Progress = {
            let log = log.clone();
            Arc::new(move |fraction, _, text| log.lock().unwrap().push((fraction, text)))
        };
        let cancel = Arc::new(AtomicBool::new(false));

        let shared = share(
            &account,
            request(vec![source(wav.clone(), "Loud"), source(dir.path().join("gone.wav"), "Gone")]),
            &cancel,
            &progress,
        )
        .map_err(|e| format!("{e:?}"))
        .unwrap();
        assert_eq!(shared.skipped, ["Gone"]);
        assert_eq!(shared.playlist.tracks, 1);
        assert!(shared.playlist.has_password);
        let fractions: Vec<f64> = log.lock().unwrap().iter().map(|(f, _)| *f).collect();
        assert!(fractions.windows(2).all(|w| w[0] <= w[1] + 1e-9), "progress went back: {fractions:?}");

        // Shared again: the same link, and nothing to convert or upload.
        log.lock().unwrap().clear();
        let again = share(&account, request(vec![source(wav, "Loud")]), &cancel, &progress).map_err(|e| format!("{e:?}")).unwrap();
        assert_eq!(again.playlist.url, shared.playlist.url);
        assert!(!log.lock().unwrap().iter().any(|(_, t)| *t == i18n::t!("cloud.converting", name = "Loud")));

        // Left on the server: the script checks the audio it got.
        let refused = Account { token: "wrong".into(), ..account };
        assert!(matches!(call(&refused, "playlist_get", json!({ "ref": "x" })), Err(CloudError::SignedOut)));
    }

    fn write_noise_wav(path: &Path, seconds: u32) {
        const RATE: u32 = 96_000;
        let bytes = RATE * seconds * 2 * 3;
        let mut wav = Vec::with_capacity(bytes as usize + 44);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 6).to_le_bytes());
        wav.extend_from_slice(&6u16.to_le_bytes());
        wav.extend_from_slice(&24u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&bytes.to_le_bytes());
        let mut x = 0x2545_f491u32;
        for _ in 0..bytes {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            wav.push(x as u8);
        }
        fs::write(path, wav).unwrap();
    }
}
