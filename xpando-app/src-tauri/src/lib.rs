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

/// Reads every espanso match file into folders of phrases (read-only).
#[tauri::command]
fn list_phrases(app: AppHandle) -> Result<Vec<xpando_core::Folder>, String> {
    let dir = espanso_config_dir(&app)?;
    xpando_core::load_folders(&dir).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_phrases])
        .run(tauri::generate_context!())
        .expect("error while running xpando");
}
