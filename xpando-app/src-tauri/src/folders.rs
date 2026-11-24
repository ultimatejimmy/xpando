//! Commands that manage folders (match files) and the apps they apply to. The work is done by
//! `xpando_core::folders`, which only touches `match/*.yml` and `config/*.yml` inside the
//! espanso config directory: every file name is checked against the files found there, and new
//! names are validated (no separators, no `..`). Errors are plain text the UI shows as is.

use tauri::AppHandle;
use xpando_core::folders::{
    self, Change, FolderInfo, MoveOutcome, MovePreview, Outcome, PhraseWhere, ScopeSpec,
};

use crate::data::data_dir;

/// Scope, lock reasons and config references for every match file.
#[tauri::command]
pub fn list_folder_info(app: AppHandle) -> Result<Vec<FolderInfo>, String> {
    folders::list_folder_info(&data_dir(&app)?)
}

/// Creates an empty folder `match/<name>.yml`.
#[tauri::command]
pub fn create_folder(app: AppHandle, name: String) -> Result<Outcome, String> {
    folders::create_folder(&data_dir(&app)?, &name)
}

/// The files a rename would touch, without changing anything.
#[tauri::command]
pub fn preview_rename_folder(
    app: AppHandle,
    file: String,
    new_name: String,
) -> Result<Vec<Change>, String> {
    folders::preview_rename(&data_dir(&app)?, &file, &new_name)
}

#[tauri::command]
pub fn rename_folder(
    app: AppHandle,
    file: String,
    new_name: String,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::rename_folder(&data_dir(&app)?, &file, &new_name, &expected_hash)
}

/// The files a delete would touch and the backups it would keep.
#[tauri::command]
pub fn preview_delete_folder(app: AppHandle, file: String) -> Result<Vec<Change>, String> {
    folders::preview_delete(&data_dir(&app)?, &file)
}

/// Deletes a folder; `confirm` must be true.
#[tauri::command]
pub fn delete_folder(
    app: AppHandle,
    file: String,
    confirm: bool,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::delete_folder(&data_dir(&app)?, &file, confirm, &expected_hash)
}

/// The files a change of apps would touch, without changing anything.
#[tauri::command]
pub fn preview_folder_scope(
    app: AppHandle,
    file: String,
    spec: ScopeSpec,
) -> Result<Vec<Change>, String> {
    folders::preview_scope(&data_dir(&app)?, &file, &spec)
}

#[tauri::command]
pub fn set_folder_scope(
    app: AppHandle,
    file: String,
    spec: ScopeSpec,
    expected_hash: String,
) -> Result<Outcome, String> {
    folders::set_scope(&data_dir(&app)?, &file, &spec, &expected_hash)
}

/// What moving one phrase to the folder that fits "Works in" would do, without changing
/// anything. Refusals (a clashing trigger, a folder with its own filters, a read-only file) come
/// back inside the preview as `blocked`.
#[tauri::command]
pub fn preview_phrase_where(
    app: AppHandle,
    file: String,
    index: usize,
    place: PhraseWhere,
) -> Result<MovePreview, String> {
    folders::preview_phrase_where(&data_dir(&app)?, &file, index, &place)
}

/// Moves one phrase to the folder that fits "Works in", creating an app folder if none matches.
/// `expected_hash` is the hash the phrase's folder had when the UI loaded it.
#[tauri::command]
pub fn set_phrase_where(
    app: AppHandle,
    file: String,
    index: usize,
    place: PhraseWhere,
    expected_hash: String,
    pin_folder: Option<String>,
) -> Result<MoveOutcome, String> {
    let dir = data_dir(&app)?;
    let mut hash = expected_hash;
    // A phrase with no folder label of its own is in the folder named after its file. Moving it
    // to another file would change that, so its folder is written onto it first.
    if let Some(pin) = pin_folder {
        let pinned = xpando_core::edit::edit_file(&dir, &file, &hash, |text| {
            xpando_core::edit::set_phrase_folders(text, &[index], Some(&pin))
        })?;
        hash = pinned.hash;
    }
    folders::set_phrase_where(&dir, &file, index, &place, &hash)
}
