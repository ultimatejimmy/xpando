use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// espanso's config directory: `%APPDATA%\espanso` on Windows,
/// `~/Library/Application Support/espanso` on macOS, `~/.config/espanso` on Linux.
fn espanso_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .config_dir()
        .map(|dir| dir.join("espanso"))
        .map_err(|e| e.to_string())
}

/// Lists espanso match files, relative to the config directory.
#[tauri::command]
fn list_match_files(app: AppHandle) -> Result<Vec<String>, String> {
    let dir = espanso_config_dir(&app)?;
    let files = xpando_core::discover_match_files(&dir).map_err(|e| e.to_string())?;
    Ok(files
        .iter()
        .filter_map(|p| p.strip_prefix(&dir).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_match_files])
        .run(tauri::generate_context!())
        .expect("error while running xpando");
}
