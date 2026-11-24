//! Commands that change phrases in espanso's match files. The work is done by
//! `xpando_core::edit`; these only locate the config directory and pass values through.
//! Errors are plain text the UI shows as is.

use tauri::AppHandle;
use xpando_core::edit::{self, NewPhrase, PhraseEdit};
use xpando_core::Folder;

use crate::data::data_dir;

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
    let dir = data_dir(&app)?;
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
    let dir = data_dir(&app)?;
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
    let dir = data_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        edit::delete_phrase(text, index)
    })
}

/// One phrase to file in a folder.
#[derive(Debug, Clone, xpando_core::serde::Deserialize)]
#[serde(crate = "xpando_core::serde")]
pub struct PhraseRef {
    pub file: String,
    pub index: usize,
}

/// Files phrases in the folder `folder` (an empty text is "no folder"; `None` puts them back in
/// the folder named after their file). Each file is changed in one write; if a file cannot be
/// changed the error says so and the files done before it stay changed.
#[tauri::command]
pub fn set_phrases_folder(app: AppHandle, items: Vec<PhraseRef>, folder: Option<String>) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let mut files: Vec<String> = Vec::new();
    for it in &items {
        if !files.contains(&it.file) {
            files.push(it.file.clone());
        }
    }
    for file in files {
        let indexes: Vec<usize> = items.iter().filter(|i| i.file == file).map(|i| i.index).collect();
        edit::edit_file_now(&dir, &file, |text| {
            edit::set_phrase_folders(text, &indexes, folder.as_deref())
        })
        .map_err(|e| format!("Could not change the folder in {file}: {e}"))?;
    }
    Ok(())
}

const FOLDER_LIST: &str = "xpando-folders.txt";

/// The folder names xpando keeps even while they have no phrases (one per line).
#[tauri::command]
pub fn folder_names_get(app: AppHandle) -> Result<Vec<String>, String> {
    let path = data_dir(&app)?.join(FOLDER_LIST);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Could not read the folder list: {e}")),
    }
}

/// Replaces the list of kept folder names.
#[tauri::command]
pub fn folder_names_set(app: AppHandle, names: Vec<String>) -> Result<(), String> {
    let path = data_dir(&app)?.join(FOLDER_LIST);
    let mut clean: Vec<String> = Vec::new();
    for n in names {
        let n = edit::validate_folder_name(&n)?;
        if !n.is_empty() && !clean.iter().any(|c| c.eq_ignore_ascii_case(&n)) {
            clean.push(n);
        }
    }
    std::fs::write(&path, clean.join("\n") + "\n").map_err(|e| format!("Could not save the folder list: {e}"))
}
