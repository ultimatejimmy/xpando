//! Commands that change phrases in espanso's match files. The work is done by
//! `xpando_core::edit`; these only locate the config directory and pass values through.
//! Errors are plain text the UI shows as is.

use tauri::AppHandle;
use xpando_core::edit::{self, NewPhrase, PhraseEdit};
use xpando_core::Folder;

use crate::espanso_config_dir;

/// Saves new values for entry `index` of `file` (a path relative to the `match` directory).
/// `expected_hash` is the hash the folder had when the UI loaded it.
#[tauri::command]
pub fn save_phrase(
    app: AppHandle,
    file: String,
    index: usize,
    edit: PhraseEdit,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = espanso_config_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        edit::update_phrase(text, index, &edit)
    })
}

/// Appends a new plain-text phrase to `file`.
#[tauri::command]
pub fn create_phrase(
    app: AppHandle,
    file: String,
    phrase: NewPhrase,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = espanso_config_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        edit::add_phrase(text, &phrase)
    })
}

/// Deletes entry `index` of `file` together with its comment.
#[tauri::command]
pub fn delete_phrase(
    app: AppHandle,
    file: String,
    index: usize,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = espanso_config_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        edit::delete_phrase(text, index)
    })
}
