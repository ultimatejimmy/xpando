//! Commands for rich text (`html:`) phrases. The work is done by `xpando_core::htmledit`
//! (and `xpando_core::edit` for the fields around the body); these only locate the config
//! directory and pass values through. Errors are plain text the UI shows as is.
//!
//! Every save is one `edit_file` call: one hash check, one atomic write, one `.bak`, and the
//! steps below each re-verify the file before the next one runs.

use tauri::AppHandle;
use xpando_core::edit::{self, NewPhrase, PhraseEdit};
use xpando_core::htmledit::{self, HtmlConversion, PlainConversion, Sanitized};
use xpando_core::{parse_match_text, Folder};

use crate::data::data_dir;

/// Saves changes to an existing phrase whose body is, or is becoming, rich text.
///
/// - `convert` = `None`: the body stays as it is (`html` phrase) and `edit` changes the
///   triggers, label and comment; `html`, when given, replaces the body.
/// - `convert` = `"html"`: `edit` is applied (its `body` is ignored), the plain `replace:`
///   body becomes `html:`, then `html` (when given) is the new body. The UI has shown the
///   user what the conversion changes, so it may be lossy.
/// - `convert` = `"plain"`: the `html:` body becomes `replace:`, then `edit` (whose `body`
///   is the new text) is applied.
#[tauri::command]
pub fn save_rich_phrase(
    app: AppHandle,
    file: String,
    index: usize,
    edit: PhraseEdit,
    convert: Option<String>,
    html: Option<String>,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = data_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        match convert.as_deref() {
            None => {
                let edit = PhraseEdit {
                    body: None,
                    ..edit.clone()
                };
                let text = edit::update_phrase(text, index, &edit)?;
                match &html {
                    Some(h) => htmledit::set_html_body(&text, index, h),
                    None => Ok(text),
                }
            }
            Some("html") => {
                let edit = PhraseEdit {
                    body: None,
                    ..edit.clone()
                };
                let text = edit::update_phrase(text, index, &edit)?;
                let text = htmledit::convert_to_html(&text, index, true)?;
                match &html {
                    Some(h) => htmledit::set_html_body(&text, index, h),
                    None => Ok(text),
                }
            }
            Some("plain") => {
                let text = htmledit::convert_to_plain(text, index, true)?;
                edit::update_phrase(&text, index, &edit)
            }
            Some(other) => Err(format!("Unknown conversion target \"{other}\".")),
        }
    })
}

/// Appends a new phrase whose body is rich text. `phrase.body` is ignored; `html` is the body.
#[tauri::command]
pub fn create_rich_phrase(
    app: AppHandle,
    file: String,
    phrase: NewPhrase,
    html: String,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = data_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        // Add it as plain text, find it again, then turn it into rich text.
        let seed = htmledit::html_to_text(&html);
        let seed = if seed.trim().is_empty() {
            "-".to_string()
        } else {
            seed
        };
        let added = edit::add_phrase(
            text,
            &NewPhrase {
                body: seed,
                ..phrase.clone()
            },
        )?;
        let index = parse_match_text(&added)
            .0
            .iter()
            .map(|p| p.index)
            .max()
            .ok_or("The new phrase could not be found after adding it.")?;
        let converted = htmledit::convert_to_html(&added, index, true)?;
        htmledit::set_html_body(&converted, index, &html)
    })
}

/// The canonical, safe form of `html` (what would be saved) and what was dropped. The UI
/// loads this into the editor and the read-only view instead of the raw file text.
#[tauri::command]
pub fn sanitize_html(html: String) -> Sanitized {
    htmledit::sanitize_html(&html)
}

/// Rich text for a plain body, for switching a phrase to rich text in the editor.
#[tauri::command]
pub fn plain_to_html(text: String) -> Result<HtmlConversion, String> {
    htmledit::plain_to_html(&text)
}

/// Plain text for a rich body, and what that loses, for switching back.
#[tauri::command]
pub fn html_to_plain(html: String) -> PlainConversion {
    htmledit::html_to_plain(&html)
}
