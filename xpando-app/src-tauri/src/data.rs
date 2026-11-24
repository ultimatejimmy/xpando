//! Commands about where xpando keeps its data: the data folder, the first-run import of an
//! existing espanso setup, changing the folder (with an optional copy) and sync warnings.
//! The logic and its tests live in `xpando_core::{datadir, import, syncsafe}`; this file only
//! locates the settings folder and passes values through. Errors are plain text for the UI.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use tauri::{AppHandle, Manager};
use xpando_core::datadir::{self, ChangeResult, DataInfo, Env, PathCheck, Settings};
use xpando_core::import::{self, ImportPreview};
use xpando_core::CopyReport;

/// Where the settings file lives: the app's own config folder, never the data folder, so it
/// stays put when the data folder is changed.
fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn settings(app: &AppHandle) -> Result<Settings, String> {
    datadir::load_settings(&settings_dir(app)?)
}

/// The data folder with `match/` and `config/` ready. Every command that reads or writes
/// phrases starts here. If the folder cannot be reached this is an error saying so: there is
/// no fallback to another folder.
pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(datadir::prepare(&Env::current(), &settings(app)?)?.path)
}

/// Where the data is, whether it can be reached, and any sync conflict copies.
#[tauri::command]
pub fn data_info(app: AppHandle) -> Result<DataInfo, String> {
    datadir::data_info(&Env::current(), &settings(&app)?)
}

/// What importing the user's espanso setup would copy, or `None` when there is nothing to
/// offer. On first run (`force` false) that also means: already answered, or xpando already
/// has data. `force` is the Settings button, which looks again whatever happened before.
#[tauri::command]
pub fn import_offer(app: AppHandle, force: bool) -> Result<Option<ImportPreview>, String> {
    let env = Env::current();
    let settings = settings(&app)?;
    if !force && settings.import_handled {
        return Ok(None);
    }
    let dest = datadir::prepare(&env, &settings)?.path;
    if !force && datadir::has_data(&dest) {
        return Ok(None);
    }
    for source in import::find_espanso_dirs(&env, &dest) {
        let preview = import::preview_import(&source, &dest)?;
        if !preview.files.is_empty() {
            return Ok(Some(preview));
        }
    }
    Ok(None)
}

fn mark_import_handled(app: &AppHandle) -> Result<(), String> {
    let dir = settings_dir(app)?;
    let mut s = datadir::load_settings(&dir)?;
    s.import_handled = true;
    datadir::save_settings(&dir, &s)
}

/// Copies the espanso files into the data folder. `source` must be one of the espanso folders
/// found on this computer; nothing in it is changed.
#[tauri::command]
pub fn import_espanso(app: AppHandle, source: String) -> Result<CopyReport, String> {
    let env = Env::current();
    let dest = data_dir(&app)?;
    let source = PathBuf::from(source);
    if !import::find_espanso_dirs(&env, &dest).contains(&source) {
        return Err("That is not an espanso folder xpando found.".into());
    }
    let report = import::run_import(&source, &dest)?;
    mark_import_handled(&app)?;
    Ok(report)
}

/// "Not now" on the first-run offer: do not ask again.
#[tauri::command]
pub fn skip_import(app: AppHandle) -> Result<(), String> {
    mark_import_handled(&app)
}

/// Checks a folder the user typed or picked, without changing anything.
#[tauri::command]
pub fn check_data_dir(app: AppHandle, path: String) -> Result<PathCheck, String> {
    let current = datadir::resolve(&Env::current(), &settings(&app)?)?.path;
    Ok(datadir::check_choice(&current, &path))
}

/// Starts using another data folder (`None` goes back to the default). With `copy`, the current
/// data is copied there first; the old folder is never changed. A copy with failures does not
/// switch.
#[tauri::command]
pub fn change_data_dir(
    app: AppHandle,
    path: Option<String>,
    copy: bool,
) -> Result<ChangeResult, String> {
    datadir::change_data_dir(&Env::current(), &settings_dir(&app)?, path.as_deref(), copy)
}

fn run_picker(mut cmd: Command) -> Option<String> {
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW (the dialog itself still shows)
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The system's folder picker, started with what the OS already provides (PowerShell's folder
/// dialog on Windows, `osascript` on macOS, `zenity` or `kdialog` on Linux), so xpando needs no
/// extra package for it. Returns `None` when the user cancels or no picker is available; the
/// Settings view always has a text box as well.
#[tauri::command]
pub async fn pick_folder(start: Option<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || pick_folder_blocking(start))
        .await
        .ok()
        .flatten()
}

#[cfg(windows)]
fn pick_folder_blocking(start: Option<String>) -> Option<String> {
    // The start folder travels in an environment variable, never inside the script text.
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
        $d.Description = 'Choose the folder for your xpando data'; \
        $d.ShowNewFolderButton = $true; \
        if ($env:XPANDO_PICK_START -and (Test-Path -LiteralPath $env:XPANDO_PICK_START)) { $d.SelectedPath = $env:XPANDO_PICK_START }; \
        if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.SelectedPath) }";
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("XPANDO_PICK_START", start.unwrap_or_default());
    run_picker(cmd)
}

#[cfg(target_os = "macos")]
fn pick_folder_blocking(_start: Option<String>) -> Option<String> {
    let mut cmd = Command::new("osascript");
    cmd.args([
        "-e",
        "POSIX path of (choose folder with prompt \"Choose the folder for your xpando data\")",
    ]);
    run_picker(cmd).map(|p| p.trim_end_matches('/').to_string())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn pick_folder_blocking(start: Option<String>) -> Option<String> {
    let mut zenity = Command::new("zenity");
    zenity.args([
        "--file-selection",
        "--directory",
        "--title=Choose the folder for your xpando data",
    ]);
    if let Some(s) = &start {
        zenity.arg(format!("--filename={s}/"));
    }
    run_picker(zenity).or_else(|| {
        let mut kdialog = Command::new("kdialog");
        kdialog.args(["--getexistingdirectory", start.as_deref().unwrap_or(".")]);
        run_picker(kdialog)
    })
}
