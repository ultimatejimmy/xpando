mod advanced;
mod data;
mod edit;
mod engine;
mod folders;
mod richtext;
mod vars;
mod yamledit;

use tauri::AppHandle;

/// Reads every match file in xpando's data folder into folders of phrases (read-only).
#[tauri::command]
fn list_phrases(app: AppHandle) -> Result<Vec<xpando_core::Folder>, String> {
    let dir = data::data_dir(&app)?;
    xpando_core::load_folders(&dir).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_phrases,
            data::data_info,
            data::import_offer,
            data::import_espanso,
            data::skip_import,
            data::check_data_dir,
            data::change_data_dir,
            data::pick_folder,
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
