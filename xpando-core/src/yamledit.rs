//! Editing the raw YAML of a match file from the app's Advanced mode.
//!
//! Two scopes: one entry of `matches:` ([`replace_entry_source`]) and the whole file
//! ([`replace_file_source`]). Both are text edits with checks afterwards: the result is parsed
//! again and everything the user did not mean to change must come out identical. A failure
//! returns a [`YamlEditError`] with a line and column inside the user's own text, so the UI can
//! point at the problem.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_norway::Value;

use crate::edit::{content_hash, resolve_match_file, save_file, SaveError};
use crate::{parse_entry, parse_match_text, scan_entries, Folder};

/// What kind of problem stopped an edit. The UI uses it to decide how to present it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The text is not valid YAML.
    Syntax,
    /// The text holds no entry, or more than one.
    EntryCount,
    /// The text is YAML but not shaped like an entry (or not like a match file).
    Shape,
    /// The text would not be a usable phrase.
    NotPhrase,
    /// Applying the text would alter something outside what the user is editing.
    SideEffect,
    /// The file's layout is one this editor does not handle (flow-style list, ...).
    Layout,
    /// Whole-file edit that removes phrases; confirm to go ahead (`lost` lists them).
    LosesPhrases,
}

/// A problem with the text the user entered. `line` and `column` are 1-based and point into
/// that text (the entry's text, or the file's text for whole-file edits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YamlEditError {
    pub kind: ErrorKind,
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// For [`ErrorKind::LosesPhrases`]: a short description of each phrase that would go.
    pub lost: Vec<String>,
}

impl std::fmt::Display for YamlEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for YamlEditError {}

fn err(kind: ErrorKind, line: usize, column: usize, message: impl Into<String>) -> YamlEditError {
    YamlEditError {
        kind,
        line: line.max(1),
        column: column.max(1),
        message: message.into(),
        lost: Vec::new(),
    }
}

/// Result of checking text without writing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Validation {
    pub ok: bool,
    pub error: Option<YamlEditError>,
    /// Spaces added to (positive) or removed from (negative) every line of the text to match
    /// the entry's indentation in the file. Zero when nothing was adjusted.
    pub indent_shift: i32,
    /// A non-blocking remark, e.g. the entry was not a readable phrase before either.
    pub notice: Option<String>,
}

impl Validation {
    fn from(r: Result<Plan, YamlEditError>) -> Self {
        match r {
            Ok(p) => Validation {
                ok: true,
                error: None,
                indent_shift: p.shift,
                notice: p.notice,
            },
            Err(e) => Validation {
                ok: false,
                error: Some(e),
                indent_shift: 0,
                notice: None,
            },
        }
    }
}

struct Plan {
    new_text: String,
    shift: i32,
    notice: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

fn indent_of(l: &str) -> usize {
    l.len() - l.trim_start_matches(' ').len()
}

fn is_blank(l: &str) -> bool {
    l.trim().is_empty()
}

/// The line ending new lines should use: the file's first one.
fn detect_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// Splits into lines, each with its own terminator (`"\n"`, `"\r\n"` or `""` for a last line
/// without one). Concatenating the pieces gives back the text exactly.
fn split_keep(text: &str) -> Vec<(&str, &str)> {
    text.split_inclusive('\n')
        .map(|raw| {
            if let Some(c) = raw.strip_suffix("\r\n") {
                (c, "\r\n")
            } else if let Some(c) = raw.strip_suffix('\n') {
                (c, "\n")
            } else {
                (raw, "")
            }
        })
        .collect()
}

/// libyaml's message without the trailing " at line N column M".
fn clean_message(e: &serde_norway::Error) -> String {
    let s = e.to_string();
    match s.rfind(" at line ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

fn syntax_error(e: &serde_norway::Error, line_offset: usize) -> YamlEditError {
    let (line, col) = e.location().map_or((1, 1), |l| (l.line(), l.column()));
    err(
        ErrorKind::Syntax,
        line.saturating_sub(line_offset),
        col,
        clean_message(e),
    )
}

/// Structural facts about the file that the edit must leave alone or may rely on.
struct FileView {
    entries: Vec<Value>,
    spans: Vec<crate::EntrySpan>,
    rest: Vec<(Value, Value)>,
}

fn view_file(text: &str) -> Result<FileView, YamlEditError> {
    let layout = |m: &str| err(ErrorKind::Layout, 1, 1, m);
    let doc: Value =
        serde_norway::from_str(text).map_err(|_| layout("The file is not valid YAML."))?;
    let map = doc
        .as_mapping()
        .ok_or_else(|| layout("The file is not a YAML mapping."))?;
    let entries = map
        .get("matches")
        .and_then(Value::as_sequence)
        .ok_or_else(|| layout("The file has no block-style `matches:` list."))?
        .clone();
    let spans = scan_entries(text)
        .filter(|s| s.len() == entries.len())
        .ok_or_else(|| {
            layout(
                "This file's layout can't be edited reliably here (for example a flow-style \
                 list or unusual indentation). Edit it in an external editor.",
            )
        })?;
    let rest = map
        .iter()
        .filter(|(k, _)| k.as_str() != Some("matches"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(FileView {
        entries,
        spans,
        rest,
    })
}

/// Last line (0-based) of the entry that really belongs to it: the span minus its blank tail.
fn entry_last_line(lines: &[(&str, &str)], first: usize, end: usize) -> usize {
    let mut e = end.min(lines.len()).saturating_sub(1);
    while e > first && is_blank(lines[e].0) {
        e -= 1;
    }
    e
}

// ---------------------------------------------------------------------------------------
// One entry
// ---------------------------------------------------------------------------------------

struct OneEntry {
    /// The text's lines re-indented to the target indentation.
    lines: Vec<String>,
    shift: i32,
    value: Value,
}

/// Parses `source` as exactly one entry of `matches:` indented by `indent` spaces.
fn parse_one_entry(source: &str, indent: usize) -> Result<OneEntry, YamlEditError> {
    let mut raw: Vec<&str> = source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    while raw.last().is_some_and(|l| is_blank(l)) {
        raw.pop();
    }
    let is_significant = |l: &str| !is_blank(l) && !l.trim_start().starts_with('#');
    let Some(first) = raw.iter().position(|l| is_significant(l)) else {
        return Err(err(
            ErrorKind::EntryCount,
            1,
            1,
            "There is no entry here. Write one starting with `- ` (for example `- trigger: \":hi\"`).",
        ));
    };
    let head = raw[first].trim_start_matches(' ');
    if head != "-" && !head.starts_with("- ") {
        return Err(err(
            ErrorKind::Shape,
            first + 1,
            indent_of(raw[first]) + 1,
            "An entry starts with `- ` (a dash and a space).",
        ));
    }
    let own = indent_of(raw[first]);
    let shift = indent as i32 - own as i32;
    let mut lines = Vec::with_capacity(raw.len());
    for (i, l) in raw.iter().enumerate() {
        if l.is_empty() || shift == 0 {
            lines.push(l.to_string());
        } else if shift > 0 {
            lines.push(format!("{}{l}", " ".repeat(shift as usize)));
        } else {
            let k = (-shift) as usize;
            let lead = indent_of(l);
            if lead < k && is_significant(l) {
                return Err(err(
                    ErrorKind::Shape,
                    i + 1,
                    lead + 1,
                    "This line is indented less than the entry's first line, so it is not part of the entry.",
                ));
            }
            lines.push(l[k.min(lead)..].to_string());
        }
    }
    let wrapped = format!("matches:\n{}", lines.join("\n"));
    let doc: Value = serde_norway::from_str(&wrapped).map_err(|e| {
        let mut er = syntax_error(&e, 1);
        // Map the column back from the re-indented line to the user's own line.
        er.column = (er.column as i64 - shift as i64).max(1) as usize;
        er
    })?;
    let shape = |m: &str| err(ErrorKind::Shape, first + 1, 1, m);
    let map = doc
        .as_mapping()
        .ok_or_else(|| shape("This is not an entry of `matches:`."))?;
    if map.len() != 1 {
        return Err(shape(
            "This text has content outside the entry (another top-level key).",
        ));
    }
    let seq = map
        .get("matches")
        .and_then(Value::as_sequence)
        .ok_or_else(|| shape("This is not an entry of `matches:`."))?;
    if seq.len() != 1 {
        let second = scan_entries(&wrapped)
            .and_then(|s| s.get(1).map(|sp| sp.start_line))
            .unwrap_or(first + 1);
        return Err(err(
            ErrorKind::EntryCount,
            second,
            own + 1,
            format!(
                "This text holds {} entries; an entry is edited on its own. Use New phrase to add another one.",
                seq.len()
            ),
        ));
    }
    Ok(OneEntry {
        lines,
        shift,
        value: seq[0].clone(),
    })
}

fn plan_entry(text: &str, index: usize, new_source: &str) -> Result<Plan, YamlEditError> {
    let view = view_file(text)?;
    let Some(span) = view.spans.get(index) else {
        return Err(err(
            ErrorKind::Layout,
            1,
            1,
            "That entry no longer exists in the file.",
        ));
    };
    let lines = split_keep(text);
    let indent = indent_of(lines[span.start_line].0);
    let one = parse_one_entry(new_source, indent)?;

    let last = entry_last_line(&lines, span.first_line, span.end_line);
    let eol = detect_eol(text);
    let mut out = String::with_capacity(text.len() + new_source.len());
    for (c, e) in &lines[..span.first_line] {
        out.push_str(c);
        out.push_str(e);
    }
    for (i, l) in one.lines.iter().enumerate() {
        out.push_str(l);
        out.push_str(if i + 1 == one.lines.len() {
            lines[last].1
        } else {
            eol
        });
    }
    // The old last line has no terminator only at the very end of the file.
    for (c, e) in &lines[last + 1..] {
        out.push_str(c);
        out.push_str(e);
    }

    // Everything but the edited entry must come out the same.
    let after = view_file(&out).map_err(|_| {
        err(
            ErrorKind::SideEffect,
            1,
            1,
            "With this text the file would no longer be a plain list of entries.",
        )
    })?;
    let side = |m: &str| err(ErrorKind::SideEffect, 1, 1, m);
    if after.entries.len() != view.entries.len() {
        return Err(side(
            "This change would add or remove entries elsewhere in the file.",
        ));
    }
    if after.rest != view.rest {
        return Err(side("This change would alter other parts of the file."));
    }
    for i in 0..view.entries.len() {
        if i == index {
            continue;
        }
        if after.entries[i] != view.entries[i] {
            return Err(side("This change would alter another entry in the file."));
        }
        if after.spans[i].comment != view.spans[i].comment {
            return Err(side(
                "A comment at the end of this text would become the comment of the next entry. \
                 Remove it or put a blank line after it.",
            ));
        }
    }
    let (_, old_w) = parse_match_text(text);
    let (_, new_w) = parse_match_text(&out);
    let others = |w: &[String]| -> Vec<String> {
        let tag = format!("match #{}:", index + 1);
        w.iter().filter(|x| !x.starts_with(&tag)).cloned().collect()
    };
    if others(&old_w) != others(&new_w) {
        return Err(side("This change would affect how another entry is read."));
    }

    let old_ok = parse_entry(&view.entries[index]).is_ok();
    let mut notice = None;
    if let Err(why) = parse_entry(&one.value) {
        if old_ok {
            return Err(err(
                ErrorKind::NotPhrase,
                1,
                1,
                format!("This would no longer be a usable phrase: {why}."),
            ));
        }
        notice = Some(format!("Valid YAML, but still not a usable phrase: {why}."));
    }
    Ok(Plan {
        new_text: out,
        shift: one.shift,
        notice,
    })
}

/// Replaces entry `index` of `matches:` (its comment block and its lines, exactly the span
/// [`crate::advanced::entry_source`] shows) with `new_source`.
///
/// `new_source` must be exactly one entry. If its first `- ` is indented differently from the
/// entry's place in the file, every line is shifted by the same amount; nothing else changes.
/// Line endings and the file's trailing-newline state are kept.
pub fn replace_entry_source(
    text: &str,
    index: usize,
    new_source: &str,
) -> Result<String, YamlEditError> {
    plan_entry(text, index, new_source).map(|p| p.new_text)
}

/// Checks [`replace_entry_source`] without producing the file.
///
/// With `index` of `None` the text is a new entry that is not in the file yet: it must be one
/// readable phrase and is read at the file's indentation (two spaces when the file has no
/// entries), but nothing about existing entries is checked.
pub fn validate_entry_source(text: &str, index: Option<usize>, new_source: &str) -> Validation {
    match index {
        Some(i) => Validation::from(plan_entry(text, i, new_source)),
        None => {
            let indent = view_file(text)
                .ok()
                .and_then(|v| v.spans.first().map(|s| s.start_line))
                .map_or(2, |l| indent_of(text.lines().nth(l).unwrap_or("")));
            let r = parse_one_entry(new_source, indent).and_then(|one| {
                parse_entry(&one.value)
                    .map_err(|why| {
                        err(
                            ErrorKind::NotPhrase,
                            1,
                            1,
                            format!("This is not a usable phrase: {why}."),
                        )
                    })
                    .map(|_| Plan {
                        new_text: String::new(),
                        shift: one.shift,
                        notice: None,
                    })
            });
            Validation::from(r)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Whole file
// ---------------------------------------------------------------------------------------

fn describe(p: &crate::Phrase) -> String {
    let t = p.triggers.join(", ");
    match &p.label {
        Some(l) => format!("{l} ({t})"),
        None => t,
    }
}

fn plan_file(text: &str, new_text: &str, confirm_loss: bool) -> Result<Plan, YamlEditError> {
    // The user's text uses `\n`; the file keeps its own line endings and final newline.
    let eol = detect_eol(text);
    let mut body = new_text.replace("\r\n", "\n");
    let ends_nl = text.is_empty() || text.ends_with('\n');
    if ends_nl && !body.ends_with('\n') {
        body.push('\n');
    } else if !ends_nl && body.ends_with('\n') {
        body.pop();
    }
    let out = if eol == "\r\n" {
        body.replace('\n', "\r\n")
    } else {
        body
    };

    let doc: Value = serde_norway::from_str(&out).map_err(|e| syntax_error(&e, 0))?;
    let map = doc.as_mapping().ok_or_else(|| {
        err(
            ErrorKind::Shape,
            1,
            1,
            "A match file is a YAML mapping (`key: value` lines at the top level).",
        )
    })?;
    match map.get("matches") {
        None | Some(Value::Null) | Some(Value::Sequence(_)) => {}
        Some(_) => {
            return Err(err(
                ErrorKind::Shape,
                1,
                1,
                "`matches` must be a list of entries.",
            ));
        }
    }
    let (old_p, old_w) = parse_match_text(text);
    let (new_p, new_w) = parse_match_text(&out);
    if new_w.len() > old_w.len() {
        let why = new_w
            .iter()
            .find(|w| !old_w.contains(w))
            .or(new_w.first())
            .cloned()
            .unwrap_or_default();
        return Err(err(
            ErrorKind::NotPhrase,
            1,
            1,
            format!("An entry would not be readable as a phrase ({why})."),
        ));
    }
    // A phrase is "kept" when a new phrase shares a trigger or its body with it.
    let lost: Vec<String> = old_p
        .iter()
        .filter(|o| {
            !new_p.iter().any(|n| {
                n.triggers.iter().any(|t| o.triggers.contains(t))
                    || (n.kind == o.kind && n.content == o.content)
            })
        })
        .map(describe)
        .collect();
    if !lost.is_empty() && !confirm_loss {
        let mut e = err(
            ErrorKind::LosesPhrases,
            1,
            1,
            format!("This would remove {} phrase(s) from the file.", lost.len()),
        );
        e.lost = lost;
        return Err(e);
    }
    Ok(Plan {
        new_text: out,
        shift: 0,
        notice: None,
    })
}

/// Replaces the whole text of a match file with `new_text` after checking that it is a YAML
/// mapping whose `matches` is a list (or absent), that no entry became unreadable, and that
/// no phrase disappeared (unless `confirm_loss`). Keeps the file's line endings and final
/// newline.
pub fn replace_file_source(
    text: &str,
    new_text: &str,
    confirm_loss: bool,
) -> Result<String, YamlEditError> {
    plan_file(text, new_text, confirm_loss).map(|p| p.new_text)
}

/// Checks [`replace_file_source`] without producing the file.
pub fn validate_file_source(text: &str, new_text: &str, confirm_loss: bool) -> Validation {
    Validation::from(plan_file(text, new_text, confirm_loss))
}

// ---------------------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------------------

/// Why a save through [`apply_to_file`] failed, in a form the UI can show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveFailure {
    /// `"invalid"` when the text was refused (see `error`), `"other"` for everything else
    /// (including a file changed on disk: the message then starts with `file-changed:`).
    pub kind: String,
    pub message: String,
    pub error: Option<YamlEditError>,
}

impl SaveFailure {
    fn other(message: impl Into<String>) -> Self {
        SaveFailure {
            kind: "other".into(),
            message: message.into(),
            error: None,
        }
    }
}

/// Loads `file`, checks its hash, lets `edit` turn its text into new text, saves it with the
/// usual safety net (`.bak`, atomic rename) and returns the reloaded folder.
pub fn apply_to_file(
    config_dir: &Path,
    file: &str,
    expected_hash: &str,
    edit: impl FnOnce(&str) -> Result<String, YamlEditError>,
) -> Result<Folder, SaveFailure> {
    let path = resolve_match_file(config_dir, file).map_err(SaveFailure::other)?;
    let bytes =
        fs::read(&path).map_err(|e| SaveFailure::other(format!("Could not read the file: {e}")))?;
    if content_hash(&bytes) != expected_hash {
        return Err(SaveFailure::other(SaveError::Conflict.to_string()));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| SaveFailure::other("The file is not valid UTF-8."))?;
    let new_text = edit(&text).map_err(|e| SaveFailure {
        kind: "invalid".into(),
        message: e.to_string(),
        error: Some(e),
    })?;
    if new_text != text {
        save_file(&path, &new_text, expected_hash)
            .map_err(|e| SaveFailure::other(e.to_string()))?;
    }
    crate::load_folders(config_dir)
        .map_err(|e| SaveFailure::other(e.to_string()))?
        .into_iter()
        .find(|f| f.file == file)
        .ok_or_else(|| SaveFailure::other("The file disappeared after saving."))
}

/// Reads `file` for validation: its text, after checking it still hashes to `expected_hash`.
pub fn read_for_validation(
    config_dir: &Path,
    file: &str,
    expected_hash: &str,
) -> Result<String, String> {
    let path = resolve_match_file(config_dir, file)?;
    let bytes = fs::read(&path).map_err(|e| format!("Could not read the file: {e}"))?;
    if content_hash(&bytes) != expected_hash {
        return Err(SaveError::Conflict.to_string());
    }
    String::from_utf8(bytes).map_err(|_| "The file is not valid UTF-8.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advanced::entry_source;

    const FILE: &str = "# header\nmatches:\n  # Greeting\n  # second line\n  - trigger: \":hi\"\n    replace: Hello\n\n  - trigger: \":bye\"\n    replace: |\n      Bye\n      now\n\n  - trigger: \":end\"\n    replace: End\n\nglobal_vars:\n  - name: x\n    type: echo\n    params:\n      echo: y\n";

    fn src(text: &str, i: usize) -> String {
        let mut s = entry_source(text, i).unwrap().lines.join("\n");
        s.push('\n');
        s
    }

    fn crlf(s: &str) -> String {
        s.replace('\n', "\r\n")
    }

    const ONE: &str = "- trigger: a\n  replace: c\n";

    #[test]
    fn unchanged_source_is_byte_identical() {
        for i in 0..3 {
            assert_eq!(replace_entry_source(FILE, i, &src(FILE, i)).unwrap(), FILE);
        }
        let c = crlf(FILE);
        for i in 0..3 {
            assert_eq!(replace_entry_source(&c, i, &src(&c, i)).unwrap(), c);
        }
    }

    #[test]
    fn unchanged_source_without_final_newline() {
        let t = "matches:\n  - trigger: a\n    replace: b";
        assert_eq!(replace_entry_source(t, 0, &src(t, 0)).unwrap(), t);
        let t2 = "matches:\r\n  - trigger: a\r\n    replace: b\r\n  - trigger: c\r\n    replace: d";
        for i in 0..2 {
            assert_eq!(replace_entry_source(t2, i, &src(t2, i)).unwrap(), t2);
        }
    }

    #[test]
    fn edit_comment_only() {
        let new = "  # Hello there\n  - trigger: \":hi\"\n    replace: Hello\n";
        let out = replace_entry_source(FILE, 0, new).unwrap();
        assert_eq!(
            out,
            FILE.replace("  # Greeting\n  # second line\n", "  # Hello there\n")
        );
        assert_eq!(
            parse_match_text(&out).0[0].comment.as_deref(),
            Some("Hello there")
        );
    }

    #[test]
    fn change_trigger_first_middle_last() {
        let out = replace_entry_source(FILE, 0, &src(FILE, 0).replace(":hi", ":hello")).unwrap();
        assert_eq!(out, FILE.replace(":hi", ":hello"));
        let out = replace_entry_source(FILE, 1, &src(FILE, 1).replace(":bye", ":cya")).unwrap();
        assert_eq!(out, FILE.replace(":bye", ":cya"));
        let out = replace_entry_source(FILE, 2, &src(FILE, 2).replace(":end", ":fin")).unwrap();
        assert_eq!(out, FILE.replace(":end", ":fin"));
    }

    #[test]
    fn grow_and_shrink_keep_the_rest() {
        let new = "  - trigger: \":hi\"\n    replace: Hello\n    label: Greet\n    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y\"\n";
        let out = replace_entry_source(FILE, 0, new).unwrap();
        let (p, w) = parse_match_text(&out);
        assert!(w.is_empty());
        assert_eq!(p[0].vars.len(), 1);
        assert_eq!(p[1].triggers, [":bye"]);
        assert!(out.ends_with("      echo: y\n"));
        let back = replace_entry_source(
            &out,
            0,
            "  # Greeting\n  # second line\n  - trigger: \":hi\"\n    replace: Hello\n",
        )
        .unwrap();
        assert_eq!(back, FILE);
    }

    #[test]
    fn last_entry_keeps_following_top_level_keys() {
        let out =
            replace_entry_source(FILE, 2, "  - trigger: \":end\"\n    replace: Done\n").unwrap();
        assert!(out.contains("replace: Done\n\nglobal_vars:\n"));
    }

    #[test]
    fn crlf_is_kept_for_new_lines() {
        let c = crlf(FILE);
        let new = "  - trigger: \":hi\"\n    replace: Hello\n    label: L\n";
        let out = replace_entry_source(&c, 0, new).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'));
        assert!(out.contains("    label: L\r\n"));
    }

    #[test]
    fn crlf_in_user_text_is_accepted_in_lf_file() {
        let new = "  - trigger: \":hi\"\r\n    replace: Hello\r\n";
        let out = replace_entry_source(FILE, 0, new).unwrap();
        assert!(!out.contains('\r'));
    }

    #[test]
    fn mixed_endings_elsewhere_are_untouched() {
        let t = "matches:\r\n  - trigger: a\r\n    replace: b\n  - trigger: c\n    replace: d\r\n";
        let out = replace_entry_source(t, 1, "  - trigger: c\n    replace: e\n").unwrap();
        assert_eq!(
            out,
            "matches:\r\n  - trigger: a\r\n    replace: b\n  - trigger: c\r\n    replace: e\r\n"
        );
    }

    #[test]
    fn no_final_newline_is_kept() {
        let t = "matches:\n  - trigger: a\n    replace: b\n  - trigger: c\n    replace: d";
        let out = replace_entry_source(t, 1, "  - trigger: c\n    replace: e\n\n\n").unwrap();
        assert_eq!(
            out,
            "matches:\n  - trigger: a\n    replace: b\n  - trigger: c\n    replace: e"
        );
        let out = replace_entry_source(t, 0, "  - trigger: a\n    replace: z").unwrap();
        assert_eq!(out, t.replace("replace: b", "replace: z"));
    }

    #[test]
    fn add_a_vars_block() {
        let new = "  - trigger: \":bye\"\n    replace: \"{{x}}\"\n    vars:\n      - name: x\n        type: echo\n        params:\n          echo: hi\n";
        let out = replace_entry_source(FILE, 1, new).unwrap();
        let (p, w) = parse_match_text(&out);
        assert!(w.is_empty());
        assert_eq!(p[1].vars[0].name, "x");
        assert_eq!(p[2].triggers, [":end"]);
        assert!(out.contains("echo: hi\n\n  - trigger: \":end\""));
    }

    #[test]
    fn dedented_paste_is_shifted() {
        let new = "- trigger: \":hi\"\n  replace: |\n    a\n\n    b\n";
        let out = replace_entry_source(FILE, 0, new).unwrap();
        let v = validate_entry_source(FILE, Some(0), new);
        assert!(v.ok);
        assert_eq!(v.indent_shift, 2);
        assert!(out.contains("  - trigger: \":hi\"\n    replace: |\n      a\n\n      b\n"));
        assert_eq!(parse_match_text(&out).0[0].content, "a\n\nb");
    }

    #[test]
    fn over_indented_paste_is_shifted_back() {
        let new = "      - trigger: \":hi\"\n        replace: Yo\n";
        let out = replace_entry_source(FILE, 0, new).unwrap();
        assert!(out.contains("  - trigger: \":hi\"\n    replace: Yo\n"));
        assert_eq!(validate_entry_source(FILE, Some(0), new).indent_shift, -4);
    }

    #[test]
    fn line_less_indented_than_entry_is_refused() {
        let new = "      - trigger: \":hi\"\n  replace: Yo\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Shape);
        assert_eq!(e.line, 2);
    }

    #[test]
    fn file_with_zero_indent_entries() {
        let t = "matches:\n- trigger: a\n  replace: b\n- trigger: c\n  replace: d\n";
        let out = replace_entry_source(t, 0, "- trigger: a\n  replace: z\n").unwrap();
        assert_eq!(out, t.replace("replace: b", "replace: z"));
        let out = replace_entry_source(t, 1, "  - trigger: c\n    replace: z\n").unwrap();
        assert_eq!(out, t.replace("replace: d", "replace: z"));
    }

    #[test]
    fn two_entries_are_refused_at_the_second() {
        let new = "  - trigger: a\n    replace: b\n  - trigger: c\n    replace: d\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert_eq!(e.kind, ErrorKind::EntryCount);
        assert_eq!(e.line, 3);
    }

    #[test]
    fn zero_entries_are_refused() {
        for new in ["", "\n\n", "  # only a comment\n"] {
            let e = replace_entry_source(FILE, 0, new).unwrap_err();
            assert_eq!(e.kind, ErrorKind::EntryCount, "{new:?}");
        }
    }

    #[test]
    fn not_a_list_item_is_refused() {
        let e = replace_entry_source(FILE, 0, "  trigger: a\n  replace: b\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::Shape);
        assert_eq!(e.line, 1);
    }

    #[test]
    fn broken_yaml_reports_line_and_column_in_user_text() {
        // Unclosed flow sequence on line 3 (a comment is line 1).
        let new = "  # note\n  - trigger: a\n    replace: [oops\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Syntax);
        assert!(e.line >= 3, "{e:?}");
        // Bad indentation inside the entry.
        let new = "  - trigger: a\n     replace: b\n    label: c\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Syntax);
        assert!(e.line >= 1 && e.line <= 3);
        // Unbalanced quote.
        let e = replace_entry_source(FILE, 0, "  - trigger: \"a\n    replace: b\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::Syntax);
        // A duplicate key must not panic, whatever the parser decides.
        let _ = replace_entry_source(FILE, 0, "  - trigger: a\n    trigger: b\n    replace: c\n");
    }

    #[test]
    fn error_columns_account_for_the_shift() {
        // Dedented paste (shift +2): the column refers to the user's own text.
        let new = "- trigger: a\n  replace: [oops\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Syntax);
        assert!(e.line >= 2);
        assert!(e.column <= 16, "{e:?}");
    }

    #[test]
    fn top_level_content_inside_the_entry_is_refused() {
        let new = "  - trigger: a\n    replace: b\nglobal_vars: []\n";
        let e = replace_entry_source(FILE, 0, new).unwrap_err();
        assert!(matches!(e.kind, ErrorKind::Shape | ErrorKind::Syntax));
        let t = "matches:\n- trigger: a\n  replace: b\n";
        let e = replace_entry_source(t, 0, "- trigger: a\n  replace: b\nother: 1\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::Shape);
    }

    #[test]
    fn entry_that_is_not_a_phrase_is_refused_when_it_was_one() {
        let e = replace_entry_source(FILE, 0, "  - trigger: a\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotPhrase);
        let e = replace_entry_source(FILE, 0, "  - just text\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotPhrase);
    }

    #[test]
    fn broken_entry_can_be_repaired_and_stays_editable() {
        let t = "matches:\n  - trigger: \":x\"\n\n  - trigger: b\n    replace: c\n";
        let v = validate_entry_source(t, Some(0), "  - trigger: \":x\"\n    label: no body\n");
        assert!(v.ok);
        assert!(v.notice.is_some());
        let out = replace_entry_source(t, 0, "  - trigger: \":x\"\n    replace: fixed\n").unwrap();
        assert!(parse_match_text(&out).1.is_empty());
    }

    #[test]
    fn trailing_comment_would_steal_next_entry_comment() {
        let t = "matches:\n  - trigger: a\n    replace: b\n  # about c\n  - trigger: c\n    replace: d\n";
        let e =
            replace_entry_source(t, 0, "  - trigger: a\n    replace: b\n  # extra\n").unwrap_err();
        assert_eq!(e.kind, ErrorKind::SideEffect);
        let ok = replace_entry_source(t, 0, "  - trigger: a\n    replace: b\n    # note\n");
        assert!(ok.is_ok());
    }

    #[test]
    fn unusual_layouts_are_refused() {
        let kind = |t: &str, i: usize| replace_entry_source(t, i, ONE).unwrap_err().kind;
        assert_eq!(
            kind("matches: [{trigger: a, replace: b}]\n", 0),
            ErrorKind::Layout
        );
        assert_eq!(kind("foo: 1\n", 0), ErrorKind::Layout);
        assert_eq!(kind("matches:\n  - [unclosed\n", 0), ErrorKind::Layout);
        assert_eq!(kind(FILE, 9), ErrorKind::Layout);
        assert_eq!(kind("", 0), ErrorKind::Layout);
    }

    #[test]
    fn yaml_features_in_user_text_do_not_leak() {
        let t = "matches:\n  - trigger: a\n    replace: b\n  - trigger: c\n    replace: d\n";
        let out = replace_entry_source(t, 0, "  - trigger: a\n    replace: &x b\n").unwrap();
        assert_eq!(parse_match_text(&out).0[1].content, "d");
    }

    #[test]
    fn validation_matches_replace() {
        let good = "  - trigger: a\n    replace: b\n";
        assert!(validate_entry_source(FILE, Some(0), good).ok);
        let v = validate_entry_source(FILE, Some(0), "  - trigger: [x\n");
        assert!(!v.ok);
        assert_eq!(v.error.unwrap().kind, ErrorKind::Syntax);
    }

    #[test]
    fn new_entry_mode_is_tolerant_about_the_file() {
        assert!(validate_entry_source("matches: []\n", None, ONE).ok);
        assert!(validate_entry_source("", None, ONE).ok);
        assert!(!validate_entry_source(FILE, None, "- trigger: a\n").ok);
        let two = "- trigger: a\n  replace: b\n- trigger: c\n  replace: d\n";
        assert_eq!(
            validate_entry_source(FILE, None, two).error.unwrap().kind,
            ErrorKind::EntryCount
        );
        assert_eq!(validate_entry_source(FILE, None, ONE).indent_shift, 2);
    }

    // ---- whole file ----

    #[test]
    fn whole_file_same_text_is_identical() {
        assert_eq!(replace_file_source(FILE, FILE, false).unwrap(), FILE);
        let c = crlf(FILE);
        assert_eq!(replace_file_source(&c, FILE, false).unwrap(), c);
        assert_eq!(replace_file_source(&c, &c, false).unwrap(), c);
    }

    #[test]
    fn whole_file_keeps_final_newline_state() {
        let t = "matches:\n  - trigger: a\n    replace: b";
        assert_eq!(replace_file_source(t, &format!("{t}\n"), false).unwrap(), t);
        let t2 = format!("{t}\n");
        assert_eq!(replace_file_source(&t2, t, false).unwrap(), t2);
    }

    #[test]
    fn whole_file_edit_top_level_keys() {
        let new = FILE.replace("global_vars:", "imports:\n  - \"x.yml\"\nglobal_vars:");
        let out = replace_file_source(FILE, &new, false).unwrap();
        assert_eq!(out, new);
        assert_eq!(parse_match_text(&out).0.len(), 3);
    }

    #[test]
    fn whole_file_syntax_error_has_position() {
        let new = "matches:\n  - trigger: a\n    replace: [oops\n";
        let e = replace_file_source(FILE, new, false).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Syntax);
        assert!(e.line >= 3, "{e:?}");
    }

    #[test]
    fn whole_file_shape_errors() {
        let kind = |n: &str| replace_file_source(FILE, n, false).unwrap_err().kind;
        assert_eq!(kind("- a\n- b\n"), ErrorKind::Shape);
        assert_eq!(kind("just text\n"), ErrorKind::Shape);
        assert_eq!(kind("matches: 3\n"), ErrorKind::Shape);
        assert!(replace_file_source(FILE, "global_vars: []\n", true).is_ok());
    }

    #[test]
    fn whole_file_cannot_add_unreadable_entries() {
        let new = FILE.replace("    replace: End\n", "");
        let e = replace_file_source(FILE, &new, true).unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotPhrase);
    }

    #[test]
    fn whole_file_losing_phrases_needs_confirmation() {
        let new = "matches:\n  - trigger: \":hi\"\n    replace: Hello\n";
        let e = replace_file_source(FILE, new, false).unwrap_err();
        assert_eq!(e.kind, ErrorKind::LosesPhrases);
        assert_eq!(e.lost.len(), 2);
        assert!(replace_file_source(FILE, new, true).is_ok());
    }

    #[test]
    fn whole_file_editing_a_trigger_or_body_is_not_a_loss() {
        assert!(replace_file_source(FILE, &FILE.replace(":bye", ":cya"), false).is_ok());
        assert!(replace_file_source(FILE, &FILE.replace("Hello", "Howdy"), false).is_ok());
        let reordered = "matches:\n  - trigger: \":end\"\n    replace: End\n  - trigger: \":hi\"\n    replace: Hello\n  - trigger: \":bye\"\n    replace: Bye\n";
        assert!(replace_file_source(FILE, reordered, false).is_ok());
    }

    #[test]
    fn whole_file_validation_reports() {
        assert!(validate_file_source(FILE, FILE, false).ok);
        assert!(!validate_file_source(FILE, "a: [", false).ok);
    }

    // ---- saving ----

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("xpando-yamledit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("match")).unwrap();
        d
    }

    #[test]
    fn apply_saves_with_backup_and_reloads() {
        let d = tmp("apply");
        fs::write(d.join("match/a.yml"), FILE).unwrap();
        let hash = content_hash(FILE.as_bytes());
        let folder = apply_to_file(&d, "a.yml", &hash, |t| {
            replace_entry_source(t, 0, "  - trigger: \":hi\"\n    replace: Changed\n")
        })
        .unwrap();
        assert_eq!(folder.phrases[0].content, "Changed");
        assert_eq!(fs::read_to_string(d.join("match/a.yml.bak")).unwrap(), FILE);
        // A stale hash is a conflict and writes nothing.
        let f = apply_to_file(&d, "a.yml", &hash, |t| Ok(t.to_string())).unwrap_err();
        assert!(f.message.starts_with("file-changed:"));
        // A refused edit is reported structured and leaves the file alone.
        let h2 = folder.hash.clone();
        let before = fs::read_to_string(d.join("match/a.yml")).unwrap();
        let f = apply_to_file(&d, "a.yml", &h2, |t| replace_entry_source(t, 0, "  - [x\n"))
            .unwrap_err();
        assert_eq!(f.kind, "invalid");
        assert!(f.error.is_some());
        assert_eq!(fs::read_to_string(d.join("match/a.yml")).unwrap(), before);
        // Paths outside match/ are refused.
        assert!(apply_to_file(&d, "../x.yml", &h2, |t| Ok(t.into())).is_err());
        fs::remove_dir_all(&d).unwrap();
    }

    /// Dry run against real files named in `XPANDO_DRYRUN_DIR` (read only): replacing each
    /// entry with its own source must reproduce the file byte for byte.
    #[test]
    fn dry_run_on_real_files_when_asked() {
        let Ok(dir) = std::env::var("XPANDO_DRYRUN_DIR") else {
            return;
        };
        let files = crate::discover_match_files(Path::new(&dir)).unwrap();
        let (mut entries, mut skipped) = (0, 0);
        for f in files {
            let text = fs::read_to_string(&f).unwrap();
            let mut i = 0;
            while let Some(s) = entry_source(&text, i) {
                let mut own = s.lines.join("\n");
                own.push('\n');
                let out = replace_entry_source(&text, i, &own)
                    .unwrap_or_else(|e| panic!("{}: entry {i}: {e}", f.display()));
                assert_eq!(out, text, "{} entry {i}", f.display());
                entries += 1;
                i += 1;
            }
            if i == 0 {
                skipped += 1;
            }
            assert_eq!(replace_file_source(&text, &text, false).unwrap(), text);
        }
        eprintln!("dry run: {entries} entries identical, {skipped} files without entries");
    }
}
