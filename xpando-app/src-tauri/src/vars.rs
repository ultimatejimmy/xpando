//! Commands for the Variables editor. The work is done by `xpando_core::vars`; these locate
//! the config directory, apply the hash check and pass values through. Errors are plain text
//! the UI shows as is.

use std::fs;

use tauri::AppHandle;
use xpando_core::edit::{self, NewPhrase, PhraseEdit};
use xpando_core::vars::{self, VarSpec, VarsChange};
use xpando_core::Folder;

use crate::data::data_dir;

/// The variables of entry `index` of `file`, in a form the editor can write back. An error
/// means they use a layout that can only be edited in a text editor.
#[tauri::command]
pub fn get_phrase_vars(app: AppHandle, file: String, index: usize) -> Result<Vec<VarSpec>, String> {
    let dir = data_dir(&app)?;
    let path = edit::resolve_match_file(&dir, &file)?;
    let text = fs::read_to_string(path).map_err(|e| format!("Could not read the file: {e}"))?;
    vars::read_vars(&text, index)
}

/// Saves entry `index` in one write: the usual trigger/label `edit` (its body is ignored),
/// then the new variable list and/or body from `change`. If any step fails nothing is written.
#[tauri::command]
pub fn save_phrase_vars(
    app: AppHandle,
    file: String,
    index: usize,
    change: VarsChange,
    edit: Option<PhraseEdit>,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = data_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        let text = match edit {
            Some(mut e) => {
                e.body = None;
                edit::update_phrase(text, index, &e)?
            }
            None => text.to_string(),
        };
        vars::update_entry(&text, index, &change)
    })
}

/// Appends a new plain-text phrase together with its variables, in one write.
#[tauri::command]
pub fn create_phrase_with_vars(
    app: AppHandle,
    file: String,
    phrase: NewPhrase,
    variables: Vec<VarSpec>,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = data_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        let before = xpando_core::parse_match_text(text).0.len();
        let added = edit::add_phrase(text, &phrase)?;
        let (phrases, _) = xpando_core::parse_match_text(&added);
        let index = phrases
            .iter()
            .map(|p| p.index)
            .max()
            .filter(|_| phrases.len() > before)
            .ok_or("The new phrase could not be found after adding it.")?;
        vars::update_vars(&added, index, &variables)
    })
}
