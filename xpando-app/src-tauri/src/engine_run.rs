//! Starting, stopping and checking xpando's engine, and starting it at sign-in.
//!
//! The engine (`xpando-engine`) is a separate program that watches the keyboard and types the
//! expansions. It is given xpando's own data folder and its own runtime folder, so it never
//! reads espanso's files or collides with a stock espanso that is also installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use xpando_core::engine::EngineKind;
use xpando_core::serde::Serialize;
use xpando_core::shortcut;

use crate::data::data_dir;
use crate::engine::engine_status;

/// Argument that makes the app start the engine and quit, without opening a window. Used by
/// "start at sign-in".
pub const BACKGROUND_FLAG: &str = "--background";

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "xpando_core::serde")]
pub struct EngineRun {
    pub running: bool,
    /// The user stopped the engine on purpose, so the app does not start it by itself.
    pub stopped_by_user: bool,
    /// Whether the engine starts when the user signs in.
    pub at_login: bool,
    /// Why it is not running, in plain words, when that is known.
    pub problem: Option<String>,
}

fn runtime_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Could not find a place for the engine's files: {e}"))?
        .join("engine");
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

fn stopped_flag(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Could not find the settings folder: {e}"))?;
    Ok(dir.join("engine-stopped"))
}

fn stopped_by_user(app: &AppHandle) -> bool {
    stopped_flag(app).map(|p| p.exists()).unwrap_or(false)
}

fn set_stopped_by_user(app: &AppHandle, stopped: bool) {
    let Ok(flag) = stopped_flag(app) else { return };
    if stopped {
        if let Some(parent) = flag.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(flag, b"");
    } else {
        let _ = fs::remove_file(flag);
    }
}

/// xpando's own engine, never a stock espanso.
fn engine_path(app: &AppHandle) -> Result<PathBuf, String> {
    let status = engine_status(app.clone());
    match (status.kind, status.path) {
        (EngineKind::Xpando, Some(path)) => Ok(PathBuf::from(path)),
        _ => Err(
            "xpando's engine was not found next to the app. Reinstall xpando to fix this."
                .to_string(),
        ),
    }
}

fn base_command(app: &AppHandle, engine: &Path) -> Result<Command, String> {
    let data = data_dir(app)?;
    let runtime = runtime_dir(app)?;
    let mut cmd = Command::new(engine);
    cmd.env("ESPANSO_CONFIG_DIR", data)
        .env("ESPANSO_RUNTIME_DIR", runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(cmd)
}

/// Runs an engine command and waits a few seconds for it.
fn run_quick(mut cmd: Command, wait: Duration) -> Option<ExitStatus> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.status());
    });
    rx.recv_timeout(wait).ok()?.ok()
}

fn is_running(app: &AppHandle) -> bool {
    let Ok(engine) = engine_path(app) else {
        return false;
    };
    let Ok(mut cmd) = base_command(app, &engine) else {
        return false;
    };
    cmd.args(["service", "status"]);
    run_quick(cmd, Duration::from_secs(5)).is_some_and(|s| s.success())
}

/// Starts the engine and waits until it answers. It keeps running after xpando quits.
pub fn start(app: &AppHandle) -> Result<(), String> {
    if is_running(app) {
        return Ok(());
    }
    let engine = engine_path(app)?;
    let mut cmd = base_command(app, &engine)?;
    cmd.arg("daemon");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP: no console, and not tied to ours.
        cmd.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| format!("Could not start the engine: {e}"))?;
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(300));
        if is_running(app) {
            return Ok(());
        }
    }
    Err(format!(
        "The engine did not start. Its log is in {}.",
        runtime_dir(app)?.join("espanso.log").display()
    ))
}

fn stop(app: &AppHandle) -> Result<(), String> {
    if !is_running(app) {
        return Ok(());
    }
    let engine = engine_path(app)?;
    let mut cmd = base_command(app, &engine)?;
    cmd.args(["service", "stop"]);
    let _ = run_quick(cmd, Duration::from_secs(8));
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(6) {
        if !is_running(app) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("The engine did not stop.".to_string())
}

/// Called when the app opens: starts the engine unless the user stopped it on purpose.
pub fn start_if_wanted(app: &AppHandle) {
    if stopped_by_user(app) {
        return;
    }
    if let Err(e) = start(app) {
        // Kept for support: the Settings page shows the same text when asked to start again.
        if let Ok(dir) = app.path().app_config_dir() {
            let _ = fs::write(dir.join("engine-start-error.txt"), e);
        }
    }
}

// ---- start at sign-in -------------------------------------------------------------------

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn app_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Could not find the app: {e}"))
}

#[cfg(windows)]
fn at_login() -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", "xpando"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn set_at_login(on: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("reg");
    if on {
        let value = format!("\"{}\" {BACKGROUND_FLAG}", app_exe()?.display());
        cmd.args([
            "add", RUN_KEY, "/v", "xpando", "/t", "REG_SZ", "/d", &value, "/f",
        ]);
    } else {
        cmd.args(["delete", RUN_KEY, "/v", "xpando", "/f"]);
    }
    let out = cmd
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("Could not change the sign-in setting: {e}"))?;
    if out.status.success() || !on {
        Ok(())
    } else {
        Err("Windows did not allow xpando to start at sign-in.".to_string())
    }
}

#[cfg(target_os = "linux")]
fn autostart_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart").join("xpando.desktop"))
}

#[cfg(target_os = "linux")]
fn at_login() -> bool {
    autostart_file().is_some_and(|p| p.is_file())
}

#[cfg(target_os = "linux")]
fn set_at_login(on: bool) -> Result<(), String> {
    let file = autostart_file().ok_or("Could not find your autostart folder.")?;
    if !on {
        let _ = fs::remove_file(file);
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=xpando\nExec=\"{}\" {BACKGROUND_FLAG}\nX-GNOME-Autostart-enabled=true\n",
        app_exe()?.display()
    );
    fs::write(file, text).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn agent_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents/app.xpando.desktop.plist"))
}

#[cfg(target_os = "macos")]
fn at_login() -> bool {
    agent_file().is_some_and(|p| p.is_file())
}

#[cfg(target_os = "macos")]
fn set_at_login(on: bool) -> Result<(), String> {
    let file = agent_file().ok_or("Could not find your LaunchAgents folder.")?;
    if !on {
        let _ = fs::remove_file(file);
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>app.xpando.desktop</string><key>ProgramArguments</key><array><string>{}</string><string>{BACKGROUND_FLAG}</string></array><key>RunAtLoad</key><true/></dict></plist>\n",
        app_exe()?.display()
    );
    fs::write(file, text).map_err(|e| e.to_string())
}

// ---- commands ----------------------------------------------------------------------------

fn snapshot(app: &AppHandle, problem: Option<String>) -> EngineRun {
    EngineRun {
        running: is_running(app),
        stopped_by_user: stopped_by_user(app),
        at_login: at_login(),
        problem,
    }
}

#[tauri::command]
pub async fn engine_run_state(app: AppHandle) -> EngineRun {
    tauri::async_runtime::spawn_blocking(move || {
        let problem = engine_path(&app).err();
        snapshot(&app, problem)
    })
    .await
    .unwrap_or(EngineRun {
        running: false,
        stopped_by_user: false,
        at_login: false,
        problem: Some("Could not check the engine.".into()),
    })
}

#[tauri::command]
pub async fn engine_start(app: AppHandle) -> Result<EngineRun, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_stopped_by_user(&app, false);
        let result = start(&app);
        match result {
            Ok(()) => Ok(snapshot(&app, None)),
            Err(e) => Ok(snapshot(&app, Some(e))),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn engine_stop(app: AppHandle) -> Result<EngineRun, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_stopped_by_user(&app, true);
        let result = stop(&app);
        match result {
            Ok(()) => Ok(snapshot(&app, None)),
            Err(e) => Ok(snapshot(&app, Some(e))),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn engine_restart(app: AppHandle) -> Result<EngineRun, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_stopped_by_user(&app, false);
        let result = stop(&app).and_then(|()| start(&app));
        match result {
            Ok(()) => Ok(snapshot(&app, None)),
            Err(e) => Ok(snapshot(&app, Some(e))),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn engine_set_at_login(app: AppHandle, on: bool) -> Result<EngineRun, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_at_login(on)?;
        Ok(snapshot(&app, None))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- search shortcut ---------------------------------------------------------------------

fn default_yml(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("config").join("default.yml"))
}

/// The shortcut that opens the search box (`OFF` when it is turned off).
#[tauri::command]
pub fn search_shortcut_get(app: AppHandle) -> Result<String, String> {
    let path = default_yml(&app)?;
    let text = fs::read_to_string(&path).unwrap_or_default();
    Ok(shortcut::read_search_shortcut(&text))
}

/// Saves a new search shortcut and restarts the engine (if it is on) so it takes effect.
#[tauri::command]
pub async fn search_shortcut_set(app: AppHandle, value: String) -> Result<String, String> {
    let value = shortcut::validate_shortcut(&value)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = default_yml(&app)?;
        let text = fs::read_to_string(&path).unwrap_or_default();
        let new_text = shortcut::set_search_shortcut(&text, &value);
        if new_text != text {
            fs::write(&path, new_text)
                .map_err(|e| format!("Could not save {}: {e}", path.display()))?;
            if is_running(&app) {
                stop(&app)?;
                start(&app)?;
            }
        }
        Ok(value)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    #[test]
    fn background_flag_is_a_plain_switch() {
        assert!(super::BACKGROUND_FLAG.starts_with("--"));
    }
}
