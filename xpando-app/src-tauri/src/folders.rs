//! Commands that manage folders (match files) and the apps they apply to. The work is done by
//! `xpando_core::folders`, which only touches `match/*.yml` and `config/*.yml` inside the
//! espanso config directory: every file name is checked against the files found there, and new
//! names are validated (no separators, no `..`). Errors are plain text the UI shows as is.

use tauri::AppHandle;
use xpando_core::folders::{self, Change, FolderInfo, Outcome, ScopeSpec};

use crate::espanso_config_dir;

/// Scope, lock reasons and config references for every match file.
#[tauri::command]
pub fn list_folder_info(app: AppHandle) -> Result<Vec<FolderInfo>, String> {
    folders::list_folder_info(&espanso_config_dir(&app)?)
}

/// Creates an empty folder `match/<name>.yml`.
#[tauri::command]
pub fn create_folder(app: AppHandle, name: String) -> Result<Outcome, String> {
    folders::create_folder(&espanso_config_dir(&app)?, &name)
}

/// The files a rename would touch, without changing anything.
#[tauri::command]
pub fn preview_rename_folder(
    app: AppHandle,
    file: String,
    new_name: String,
) -> Result<Vec<Change>, String> {
    folders::preview_rename(&espanso_config_dir(&app)?, &file, &new_name)
}

#[tauri::command]
pub fn rename_folder(
    app: AppHandle,
    file: String,
    new_name: String,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::rename_folder(&espanso_config_dir(&app)?, &file, &new_name, &expected_hash)
}

/// The files a delete would touch and the backups it would keep.
#[tauri::command]
pub fn preview_delete_folder(app: AppHandle, file: String) -> Result<Vec<Change>, String> {
    folders::preview_delete(&espanso_config_dir(&app)?, &file)
}

/// Deletes a folder; `confirm` must be true.
#[tauri::command]
pub fn delete_folder(
    app: AppHandle,
    file: String,
    confirm: bool,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::delete_folder(&espanso_config_dir(&app)?, &file, confirm, &expected_hash)
}

/// The files a change of apps would touch, without changing anything.
#[tauri::command]
pub fn preview_folder_scope(
    app: AppHandle,
    file: String,
    spec: ScopeSpec,
) -> Result<Vec<Change>, String> {
    folders::preview_scope(&espanso_config_dir(&app)?, &file, &spec)
}

#[tauri::command]
pub fn set_folder_scope(
    app: AppHandle,
    file: String,
    spec: ScopeSpec,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::set_scope(&espanso_config_dir(&app)?, &file, &spec, &expected_hash)
}
