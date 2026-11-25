mod advanced;
mod apps;
mod data;
mod edit;
mod engine;
mod engine_run;
mod folders;
mod richtext;
mod stats;
mod update;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            if std::env::args().any(|a| a == engine_run::BACKGROUND_FLAG) {
                // Start at sign-in: only the engine, no window.
                std::thread::spawn(move || {
                    engine_run::start_if_wanted(&handle);
                    handle.exit(0);
                });
            } else {
                // The window is not created from the config alone, so the sign-in start above
                // never loads a web view.
                if let Some(cfg) = app.config().app.windows.first() {
                    tauri::WebviewWindowBuilder::from_config(app.handle(), cfg)?.build()?;
                }
                std::thread::spawn(move || engine_run::start_if_wanted(&handle));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_phrases,
            data::data_info,
            data::import_offer,
            data::import_espanso,
            data::skip_import,
            data::seed_samples,
            data::check_data_dir,
            data::change_data_dir,
            data::pick_folder,
            update::update_state,
            update::set_update_prefs,
            update::skip_update,
            update::check_for_update,
            update::install_update,
            advanced::get_entry_source,
            advanced::list_conflicts,
            advanced::config_path,
            engine::engine_status,
            engine_run::engine_run_state,
            engine_run::engine_start,
            engine_run::engine_stop,
            engine_run::engine_restart,
            engine_run::engine_set_at_login,
            engine_run::search_shortcut_get,
            engine_run::search_shortcut_set,
            engine::preview_clipboard_hack,
            engine::convert_clipboard_hack,
            edit::save_phrase,
            edit::create_phrase,
            edit::set_phrases_folder,
            edit::folder_names_get,
            edit::folder_names_set,
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
            folders::preview_phrase_where,
            folders::set_phrase_where,
            apps::list_running_programs,
            apps::list_installed_programs,
            apps::app_icons,
            apps::pick_program,
            stats::stats_summary,
            stats::stats_save_settings,
            stats::stats_set_enabled,
            stats::stats_reset,
            richtext::save_rich_phrase,
            richtext::create_rich_phrase,
            richtext::sanitize_html,
            richtext::plain_to_html,
            richtext::html_to_plain
        ])
        .run(tauri::generate_context!())
        .expect("error while running xpando");
}
