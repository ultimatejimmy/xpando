//! The in-app updater: looks for a newer release on GitHub, then lets Tauri's updater plugin
//! download it, verify its minisign signature (public key in `tauri.conf.json`) and install it.
//! All decisions about WHICH release to offer (channel, skipped version, never a downgrade,
//! which addresses are acceptable) are made by `xpando_core::update` and tested there; this
//! file talks to the network and the system.
//!
//! The release list is read through the system's `curl` (Windows 10 and later, macOS and most
//! Linux systems have it) or, on a Windows without `curl.exe`, a few lines of PowerShell. Every
//! request sends only the User-Agent `xpando-updater`, nothing about the computer or the user.
//! Redirects are followed one hop at a time, and each hop must stay on GitHub's own hosts. The
//! plugin does its own download of the one release picked here.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use xpando_core::datadir::{self, Settings};
use xpando_core::serde::Serialize;
use xpando_core::update::{self, Channel, Os, Update, Version, RELEASES_API_URL, USER_AGENT};

/// The version this build was made as. Cargo, Tauri and npm only take a derived semver, so the
/// human version is mapped back from it. A development build (`0.1.0`) has no human version.
const BUILD_SEMVER: &str = env!("CARGO_PKG_VERSION");

const MAX_REDIRECTS: usize = 4;
const API_MAX_BYTES: u64 = 4 * 1024 * 1024;
const API_SECS: u64 = 30;

/// Only one install at a time.
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn current_version() -> Option<Version> {
    Version::from_semver(BUILD_SEMVER)
}

fn display_version() -> String {
    current_version().map_or_else(|| BUILD_SEMVER.to_string(), |v| v.to_string())
}

// ---------------------------------------------------------------------------------------
// Settings for the Settings view
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(crate = "xpando_core::serde")]
pub struct UpdateState {
    /// The version shown to the user, e.g. `2026.11.23`.
    current: String,
    /// A build that was not made by the release workflow; it never offers updates.
    dev_build: bool,
    beta: bool,
    auto_check: bool,
    last_check: Option<u64>,
    skipped: Option<String>,
    /// A sentence to show when the user is on a beta but follows stable releases.
    note: Option<String>,
    /// `true` when xpando can install an update by itself (see `can_self_update`).
    can_install: bool,
}

fn state_of(settings: &Settings) -> UpdateState {
    let current = current_version();
    UpdateState {
        current: display_version(),
        dev_build: current.is_none(),
        beta: settings.update_channel == Channel::Beta,
        auto_check: settings.auto_check,
        last_check: settings.last_update_check,
        skipped: settings.skipped_version.clone(),
        note: current
            .as_ref()
            .and_then(|v| update::downgrade_note(v, settings.update_channel))
            .map(String::from),
        can_install: can_self_update(),
    }
}

#[tauri::command]
pub fn update_state(app: AppHandle) -> Result<UpdateState, String> {
    Ok(state_of(&datadir::load_settings(&settings_dir(&app)?)?))
}

/// The two check boxes of the Updates section. Choosing the stable channel again also clears
/// a skipped version, so nothing stays hidden for no visible reason.
#[tauri::command]
pub fn set_update_prefs(
    app: AppHandle,
    auto_check: bool,
    beta: bool,
) -> Result<UpdateState, String> {
    let dir = settings_dir(&app)?;
    let mut s = datadir::load_settings(&dir)?;
    s.auto_check = auto_check;
    s.update_channel = if beta { Channel::Beta } else { Channel::Stable };
    datadir::save_settings(&dir, &s)?;
    Ok(state_of(&s))
}

/// "Skip this version": it is not offered again by the automatic check.
#[tauri::command]
pub fn skip_update(app: AppHandle, version: String) -> Result<(), String> {
    Version::parse(&version)?;
    let dir = settings_dir(&app)?;
    let mut s = datadir::load_settings(&dir)?;
    s.skipped_version = Some(version);
    datadir::save_settings(&dir, &s)
}

// ---------------------------------------------------------------------------------------
// Looking for an update
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(
    crate = "xpando_core::serde",
    tag = "status",
    rename_all = "snake_case"
)]
pub enum CheckOutcome {
    /// An automatic check that is not due yet (or automatic checks are off).
    NotDue,
    /// This build has no release version, so there is nothing to compare.
    DevBuild,
    UpToDate,
    Available {
        update: Update,
    },
}

/// Checks GitHub for a newer release. `manual` is the "Check for updates" button: it always runs
/// and ignores a skipped version. Otherwise the check only runs when it is due. A failure is an
/// error string; the automatic check ignores it.
#[tauri::command]
pub async fn check_for_update(app: AppHandle, manual: bool) -> Result<CheckOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || check_blocking(&app, manual))
        .await
        .map_err(|e| e.to_string())?
}

fn check_blocking(app: &AppHandle, manual: bool) -> Result<CheckOutcome, String> {
    let dir = settings_dir(app)?;
    let settings = datadir::load_settings(&dir)?;
    let Some(current) = current_version() else {
        return Ok(CheckOutcome::DevBuild);
    };
    if !manual && !update::is_check_due(settings.auto_check, settings.last_update_check, now_secs())
    {
        return Ok(CheckOutcome::NotDue);
    }
    let skipped = if manual {
        None
    } else {
        settings
            .skipped_version
            .as_deref()
            .and_then(|v| Version::parse(v).ok())
    };
    let found = find_update(&current, settings.update_channel, skipped.as_ref())?;
    // Remember the check (re-read the file: the user may have changed settings meanwhile).
    let mut latest = datadir::load_settings(&dir)?;
    latest.last_update_check = Some(now_secs());
    datadir::save_settings(&dir, &latest)?;
    Ok(match found {
        Some(update) => CheckOutcome::Available { update },
        None => CheckOutcome::UpToDate,
    })
}

fn find_update(
    current: &Version,
    channel: Channel,
    skipped: Option<&Version>,
) -> Result<Option<Update>, String> {
    let work = WorkDir::new()?;
    work.sweep_old();
    let file = work.path.join("releases.json");
    download(
        RELEASES_API_URL,
        &file,
        API_MAX_BYTES,
        API_SECS,
        Some("application/vnd.github+json"),
        &|_| {},
    )?;
    let text = fs::read_to_string(&file)
        .map_err(|_| "GitHub's answer was not readable text.".to_string())?;
    let releases = update::parse_releases(&text)?;
    Ok(update::pick_update(
        current,
        channel,
        skipped,
        &releases,
        Os::current(),
        std::env::consts::ARCH,
    ))
}

// ---------------------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(crate = "xpando_core::serde")]
struct Progress {
    /// `downloading`, `verifying` or `starting`.
    phase: &'static str,
    done: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(crate = "xpando_core::serde", tag = "kind", rename_all = "snake_case")]
// Not every variant is built on every platform.
#[allow(dead_code)]
pub enum InstallOutcome {
    /// The installer was started and xpando is about to close (Windows).
    Launched,
    /// The update was installed; xpando has to be started again to use it (macOS, Linux).
    Installed,
    /// This copy cannot update itself (a Linux install that is not an AppImage), so the
    /// release page was opened in the browser.
    OpenedPage,
}

/// Whether xpando can install an update by itself. Windows (the NSIS installer) and macOS
/// (the app bundle) always can; on Linux only the AppImage can replace itself.
fn can_self_update() -> bool {
    cfg!(windows) || cfg!(target_os = "macos") || std::env::var_os("APPIMAGE").is_some()
}

/// Downloads `version`, checks its signature and installs it. The version is the only thing
/// taken from the window: the release is looked up again here, and the update itself comes
/// from the release's signed `latest.json` through Tauri's updater, which refuses anything not
/// signed with xpando's key (the public half is built into the app).
#[tauri::command]
pub async fn install_update(app: AppHandle, version: String) -> Result<InstallOutcome, String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("An update is already being installed.".into());
    }
    let result = install(&app, &version).await;
    INSTALLING.store(false, Ordering::SeqCst);
    result
}

/// Picks the release again (same rules as the check) and returns it if it is `version`.
fn select_for_install(app: &AppHandle, version: &str) -> Result<Update, String> {
    let settings = datadir::load_settings(&settings_dir(app)?)?;
    let current =
        current_version().ok_or("This is a development build, which does not install updates.")?;
    let wanted = Version::parse(version)?;
    find_update(&current, settings.update_channel, None)?
        .filter(|u| Version::parse(&u.version).ok() == Some(wanted))
        .ok_or_else(|| {
            "That version is not the newest one any more. Check for updates again.".to_string()
        })
}

/// Words for an updater failure. A bad signature gets its own plain sentence.
fn explain_updater_error(e: &tauri_plugin_updater::Error) -> String {
    let text = e.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("signature") || lower.contains("minisign") {
        "The update did not pass its signature check, so it was discarded. Nothing was installed."
            .into()
    } else if lower.contains("error sending request") || lower.contains("connect") {
        "Could not reach GitHub. Check your internet connection and try again.".into()
    } else {
        format!("The update could not be installed: {text}")
    }
}

async fn install(app: &AppHandle, version: &str) -> Result<InstallOutcome, String> {
    let (picker_app, picker_version) = (app.clone(), version.to_string());
    let update = tauri::async_runtime::spawn_blocking(move || {
        select_for_install(&picker_app, &picker_version)
    })
    .await
    .map_err(|e| e.to_string())??;

    if !can_self_update() {
        open_release_page(&update.release_url)?;
        return Ok(InstallOutcome::OpenedPage);
    }

    let progress = |phase: &'static str, done: u64, total: u64| {
        let _ = app.emit("update-progress", Progress { phase, done, total });
    };

    // Point the plugin at this one release's manifest, nothing else. `pick_update` already
    // checked the address; it is checked again here because it is what gets fetched.
    update::validate_download_url(&update.manifest.url, &update.tag, &update.manifest.name)?;
    let endpoint = tauri::Url::parse(&update.manifest.url).map_err(|e| e.to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .and_then(|b| b.build())
        .map_err(|e| explain_updater_error(&e))?;
    let found = updater
        .check()
        .await
        .map_err(|e| explain_updater_error(&e))?
        .ok_or("The signed update list of this release has no update for this computer.")?;
    // The signed manifest must describe the version that was offered.
    let wanted = Version::parse(version)?;
    if found.version != wanted.semver() {
        return Err("The signed update list does not match the version that was offered, so nothing was installed.".into());
    }

    let total = update.installer.size;
    progress("downloading", 0, total);
    let mut done = 0u64;
    let bytes = found
        .download(
            |chunk, len| {
                done += chunk as u64;
                progress("downloading", done, len.unwrap_or(total));
            },
            || {},
        )
        .await
        .map_err(|e| explain_updater_error(&e))?;

    // `download` has already checked the minisign signature; `install` checks it again.
    progress("verifying", total, total);
    progress("starting", total, total);
    #[cfg(windows)]
    tauri::async_runtime::spawn_blocking(stop_engine)
        .await
        .map_err(|e| e.to_string())?;
    // Windows: starts the installer in passive mode and exits xpando. macOS and Linux: replaces
    // the app in place; it has to be started again.
    found
        .install(&bytes)
        .map_err(|e| explain_updater_error(&e))?;
    Ok(if cfg!(windows) {
        InstallOutcome::Launched
    } else {
        InstallOutcome::Installed
    })
}

/// Stops xpando's engine process so the installer can replace its file. It asks politely
/// first, then forces; the name is xpando's own, so nothing else is touched. The installer
/// repeats this itself (see installer-hooks.nsh) in case the engine was started meanwhile.
#[cfg(windows)]
fn stop_engine() {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    for force in [false, true] {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/IM", "xpando-engine.exe"])
            .creation_flags(NO_WINDOW);
        if force {
            cmd.arg("/F");
        }
        let _ = cmd.output();
        if !force {
            std::thread::sleep(Duration::from_millis(800));
        }
    }
}

/// Opens a GitHub release page in the browser (only used where xpando cannot update itself).
fn open_release_page(url: &str) -> Result<(), String> {
    if !url.starts_with("https://github.com/ultimatejimmy/xpando/releases/")
        || url.bytes().any(|b| b <= b' ' || b == 0x7f)
    {
        return Err("The release page address was not recognised.".into());
    }
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("The release page could not be opened: {e}"))
}

// ---------------------------------------------------------------------------------------
// A private folder for downloads
// ---------------------------------------------------------------------------------------

const WORK_PREFIX: &str = "xpando-update-";

/// A fresh folder under the system temp folder, removed again when this goes out of scope
/// always.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn new() -> Result<WorkDir, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = std::env::temp_dir().join(format!(
            "{WORK_PREFIX}{}-{}-{nanos}",
            std::process::id(),
            now_secs()
        ));
        // `create_dir` (not `create_dir_all`) fails if the name is already taken.
        fs::create_dir(&path)
            .map_err(|e| format!("Could not make a temporary folder for the download: {e}"))?;
        Ok(WorkDir { path })
    }

    /// Removes download folders left by earlier updates that are more than two days old.
    fn sweep_old(&self) {
        let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(2 * 24 * 3600));
            if name.to_string_lossy().starts_with(WORK_PREFIX) && old && entry.path() != self.path {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------------------
// HTTPS through the system
// ---------------------------------------------------------------------------------------

/// Downloads `url` to `dest`, following at most a few redirects, each of which must go to
/// one of GitHub's own hosts. `on_bytes` is told how much has arrived while it downloads.
/// `url` itself must already be an address the caller trusts (the fixed API address or one
/// that `validate_download_url` accepted).
fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    max_secs: u64,
    accept: Option<&str>,
    on_bytes: &dyn Fn(u64),
) -> Result<(), String> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let (status, location) =
            request_once(&current, dest, max_bytes, max_secs, accept, on_bytes)?;
        match status {
            200 => return Ok(()),
            301 | 302 | 303 | 307 | 308 => {
                let next = location.ok_or("GitHub sent a redirect without a destination.")?;
                update::validate_redirect_url(&next)?;
                current = next;
            }
            404 => {
                let _ = fs::remove_file(dest);
                return Err("That file was not found on GitHub.".into());
            }
            403 | 429 => {
                let _ = fs::remove_file(dest);
                return Err("GitHub is limiting requests right now. Try again later.".into());
            }
            other => {
                let _ = fs::remove_file(dest);
                return Err(format!("GitHub answered with an error ({other})."));
            }
        }
    }
    let _ = fs::remove_file(dest);
    Err("The download was redirected too many times.".into())
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
}

/// The system's `curl`, by full path where there is a fixed one so a program of the same name
/// in the current folder is never picked.
fn curl_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot")?;
        let p = PathBuf::from(root).join("System32").join("curl.exe");
        p.is_file().then_some(p)
    }
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from("/usr/bin/curl"))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        Some(PathBuf::from("curl"))
    }
}

/// One request, no redirect handling. Returns the status and `Location`, with the body (when
/// the status is 200) in `dest`.
fn request_once(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    max_secs: u64,
    accept: Option<&str>,
    on_bytes: &dyn Fn(u64),
) -> Result<(u16, Option<String>), String> {
    let _ = fs::remove_file(dest);
    let head_file = dest.with_extension("head");
    let mut cmd;
    let from_curl = if let Some(curl) = curl_path() {
        cmd = Command::new(curl);
        cmd.arg("-q") // ignore any .curlrc; must be the first argument
            .args(["--silent", "--show-error", "--proto", "=https", "--tlsv1.2"])
            .args(["--connect-timeout", "15"])
            .args(["--max-time", &max_secs.to_string()])
            .args(["--max-filesize", &max_bytes.to_string()])
            .args(["--user-agent", USER_AGENT])
            .arg("--dump-header")
            .arg(&head_file)
            .arg("--output")
            .arg(dest);
        if let Some(a) = accept {
            cmd.args(["--header", &format!("Accept: {a}")]);
        }
        cmd.arg("--").arg(url);
        true
    } else if cfg!(windows) {
        cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", POWERSHELL_GET])
            .env("XPANDO_URL", url)
            .env("XPANDO_DEST", dest)
            .env("XPANDO_MAX", max_bytes.to_string())
            .env("XPANDO_ACCEPT", accept.unwrap_or("*/*"))
            .env("XPANDO_UA", USER_AGENT);
        false
    } else {
        return Err(
            "xpando needs the curl program to check for updates, and it was not found.".into(),
        );
    };
    quiet(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start the download: {e}"))?;
    let outcome = wait_for(
        &mut child,
        Duration::from_secs(max_secs + 20),
        dest,
        on_bytes,
    );
    let (succeeded, stdout, stderr) = outcome?;
    if !succeeded {
        // Includes a file that was larger than allowed: its headers say 200 but it is not whole.
        let _ = fs::remove_file(dest);
        let _ = fs::remove_file(&head_file);
        return Err(explain_failure(&stderr));
    }

    let head = if from_curl {
        let text = fs::read_to_string(&head_file).unwrap_or_default();
        let _ = fs::remove_file(&head_file);
        text
    } else {
        stdout
    };
    if let Some(parsed) = update::parse_response_head(&head) {
        return Ok(parsed);
    }
    let _ = fs::remove_file(dest);
    Err(explain_failure(&stderr))
}

/// Waits for the child, up to `limit`, reporting the size of `dest` as it grows.
fn wait_for(
    child: &mut Child,
    limit: Duration,
    dest: &Path,
    on_bytes: &dyn Fn(u64),
) -> Result<(bool, String, String), String> {
    let start = Instant::now();
    let mut last = u64::MAX;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => return Err(format!("The download stopped unexpectedly: {e}")),
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(dest);
            return Err("The download took too long and was stopped.".into());
        }
        let size = fs::metadata(dest).map_or(0, |m| m.len());
        if size != last {
            last = size;
            on_bytes(size);
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let read = |pipe: Option<&mut dyn Read>| {
        let mut s = String::new();
        if let Some(p) = pipe {
            let _ = p.take(64 * 1024).read_to_string(&mut s);
        }
        s
    };
    let stdout = read(child.stdout.as_mut().map(|p| p as &mut dyn Read));
    let stderr = read(child.stderr.as_mut().map(|p| p as &mut dyn Read));
    on_bytes(fs::metadata(dest).map_or(0, |m| m.len()));
    Ok((status.success(), stdout, stderr))
}

fn explain_failure(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("could not resolve")
        || lower.contains("couldn't resolve")
        || lower.contains("failed to connect")
        || lower.contains("couldn't connect")
        || lower.contains("remote name could not be resolved")
        || lower.contains("unable to connect")
        || lower.contains("timed out")
    {
        "Could not reach GitHub. Check your internet connection and try again.".into()
    } else if lower.contains("maximum file size") || lower.contains("too large") {
        "The file was larger than expected, so it was stopped.".into()
    } else {
        "Could not get an answer from GitHub. Check your internet connection and try again.".into()
    }
}

/// Used only when `curl.exe` is missing (Windows before 10 version 1803): one request without
/// redirect handling, printing a header block in the form `parse_response_head` reads.
const POWERSHELL_GET: &str = r#"
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$r = [Net.HttpWebRequest]::Create($env:XPANDO_URL)
$r.AllowAutoRedirect = $false
$r.UserAgent = $env:XPANDO_UA
$r.Accept = $env:XPANDO_ACCEPT
$r.Timeout = 15000
$r.ReadWriteTimeout = 60000
try { $resp = $r.GetResponse() } catch [Net.WebException] {
  if ($_.Exception.Response) { $resp = $_.Exception.Response } else { throw }
}
$code = [int]$resp.StatusCode
if ($code -eq 200) {
  $in = $resp.GetResponseStream()
  $out = [IO.File]::Create($env:XPANDO_DEST)
  try {
    $buf = New-Object byte[] 65536
    $total = 0L
    while (($n = $in.Read($buf, 0, $buf.Length)) -gt 0) {
      $total += $n
      if ($total -gt [long]$env:XPANDO_MAX) { throw 'too large' }
      $out.Write($buf, 0, $n)
    }
  } finally { $out.Close(); $in.Close() }
}
[Console]::Out.WriteLine("HTTP/1.1 $code")
$loc = $resp.Headers['Location']
if ($loc) { [Console]::Out.WriteLine("Location: $loc") }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to GitHub: `cargo test -- --ignored live`.
    #[test]
    #[ignore = "needs the internet"]
    fn live_release_list_is_fetched_and_parsed() {
        let work = WorkDir::new().unwrap();
        let file = work.path.join("releases.json");
        let accept = Some("application/vnd.github+json");
        download(
            RELEASES_API_URL,
            &file,
            API_MAX_BYTES,
            API_SECS,
            accept,
            &|_| {},
        )
        .unwrap();
        let text = fs::read_to_string(&file).unwrap();
        update::parse_releases(&text).unwrap();
    }

    #[test]
    fn work_dir_is_removed_when_dropped() {
        let path = {
            let w = WorkDir::new().unwrap();
            assert!(w.path.is_dir());
            w.path.clone()
        };
        assert!(!path.exists());
    }

    #[test]
    fn failures_are_explained_in_plain_words() {
        assert!(
            explain_failure("curl: (6) Could not resolve host: api.github.com")
                .contains("internet")
        );
        assert!(explain_failure("curl: (63) Maximum file size exceeded").contains("larger"));
        assert!(!explain_failure("weird").is_empty());
    }
}
