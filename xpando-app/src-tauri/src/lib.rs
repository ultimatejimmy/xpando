mod advanced;
mod edit;
mod engine;
mod folders;
mod richtext;
mod vars;
mod yamledit;

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
        .invoke_handler(tauri::generate_handler![
            list_phrases,
            advanced::get_entry_source,
            advanced::list_conflicts,
            advanced::config_path,
            engine::engine_status,
            engine::preview_clipboard_hack,
            engine::convert_clipboard_hack,
            edit::save_phrase,
            edit::create_phrase,
            edit::delete_phrase,
            yamledit::validate_entry_yaml,
            yamledit::validate_file_yaml,
            yamledit::save_entry_yaml,
            yamledit::save_file_yaml,
            yamledit::get_file_source,
            vars::get_phrase_vars,
            vars::save_phrase_vars,
            vars::create_phrase_with_vars,
            folders::list_folder_info,
            folders::create_folder,
            folders::preview_rename_folder,
            folders::rename_folder,
            folders::preview_delete_folder,
            folders::delete_folder,
            folders::preview_folder_scope,
            folders::set_folder_scope,
            richtext::save_rich_phrase,
            richtext::create_rich_phrase,
            richtext::sanitize_html,
            richtext::plain_to_html,
            richtext::html_to_plain
        ])
        .run(tauri::generate_context!())
        .expect("error while running xpando");
}
