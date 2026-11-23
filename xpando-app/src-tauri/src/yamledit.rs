//! Commands behind the YAML editor of Advanced mode. The checks live in
//! `xpando_core::yamledit`; these locate the config directory and pass values through.
//! Save failures are structured (`SaveFailure`) so the UI can underline the offending line.

use tauri::AppHandle;
use xpando_core::yamledit::{self, SaveFailure, Validation};
use xpando_core::Folder;

use crate::espanso_config_dir;

/// Checks new text for entry `index` of `file` (a path relative to `match`) without saving.
/// `index` of `None` checks a new entry that is not in the file yet. An `Err` means the file
/// could not be read or changed on disk (`file-changed:` prefix).
#[tauri::command]
pub fn validate_entry_yaml(
    app: AppHandle,
    file: String,
    index: Option<usize>,
    text: String,
    expected_hash: String,
) -> Result<Validation, String> {
    let dir = espanso_config_dir(&app)?;
    let current = yamledit::read_for_validation(&dir, &file, &expected_hash)?;
    Ok(yamledit::validate_entry_source(&current, index, &text))
}

/// Checks new text for the whole of `file` without saving.
#[tauri::command]
pub fn validate_file_yaml(
    app: AppHandle,
    file: String,
    text: String,
    confirm_loss: bool,
    expected_hash: String,
) -> Result<Validation, String> {
    let dir = espanso_config_dir(&app)?;
    let current = yamledit::read_for_validation(&dir, &file, &expected_hash)?;
    Ok(yamledit::validate_file_source(
        &current,
        &text,
        confirm_loss,
    ))
}

/// Replaces entry `index` of `file` with `text` and returns the reloaded folder.
#[tauri::command]
pub fn save_entry_yaml(
    app: AppHandle,
    file: String,
    index: usize,
    text: String,
    expected_hash: String,
) -> Result<Folder, SaveFailure> {
    let dir = espanso_config_dir(&app).map_err(|m| SaveFailure {
        kind: "other".into(),
        message: m,
        error: None,
    })?;
    yamledit::apply_to_file(&dir, &file, &expected_hash, |current| {
        yamledit::replace_entry_source(current, index, &text)
    })
}

/// Replaces the whole of `file` with `text` and returns the reloaded folder.
/// `confirm_loss` accepts that phrases present now will be gone.
#[tauri::command]
pub fn save_file_yaml(
    app: AppHandle,
    file: String,
    text: String,
    confirm_loss: bool,
    expected_hash: String,
) -> Result<Folder, SaveFailure> {
    let dir = espanso_config_dir(&app).map_err(|m| SaveFailure {
        kind: "other".into(),
        message: m,
        error: None,
    })?;
    yamledit::apply_to_file(&dir, &file, &expected_hash, |current| {
        yamledit::replace_file_source(current, &text, confirm_loss)
    })
}

/// The whole text of `file`, for the whole-file editor. Line endings are normalised to `\n`
/// for display; the save restores the file's own.
#[tauri::command]
pub fn get_file_source(
    app: AppHandle,
    file: String,
    expected_hash: String,
) -> Result<String, String> {
    let dir = espanso_config_dir(&app)?;
    let text = yamledit::read_for_validation(&dir, &file, &expected_hash)?;
    Ok(text.replace("\r\n", "\n"))
}
