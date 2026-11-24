//! Format-preserving edits of espanso match files.
//!
//! A YAML round trip through a parser drops comments and rewrites quoting, so every
//! operation here works on the raw lines: it finds the lines of one entry (see
//! [`crate::scan_entries`]), rewrites only the fields that actually changed and copies
//! every other byte through untouched (comments, blank lines, quoting, line endings).
//!
//! Each operation re-parses its own output and compares it with the input before returning
//! it. If any other phrase differs, or the edited phrase is not what was asked for, the
//! operation fails and nothing is written. Layouts the line editor does not understand
//! (flow-style lists, entries starting on the line after the `-`, ...) are refused rather
//! than guessed at.
//!
//! [`save_file`] adds the file-level safety: a content hash taken when the file was loaded
//! must still match, a one-time `.bak` copy is kept and the write is atomic.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_norway::Value;

use super::{
    make_preview, parse_match_text, scan_entries, EntrySpan, Folder, Phrase, PhraseOptions,
};

/// Prefix of the error returned when the file changed since it was loaded, so a UI can
/// offer a reload instead of just showing text.
pub const CONFLICT_PREFIX: &str = "file-changed:";

const UNSUPPORTED_FILE: &str = "This file's layout is not one xpando can edit safely (for \
    example a flow-style list). Edit it in a text editor.";
const UNSUPPORTED_ENTRY: &str = "This entry's layout is not one xpando can edit safely. \
    Edit it in a text editor.";

// ---------------------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------------------

/// What to do with the comment block above an entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentChange {
    /// Leave the comment lines exactly as they are.
    #[default]
    Keep,
    /// Replace the comment block with these lines (an empty text clears it).
    Set(String),
    /// Remove the comment block.
    Clear,
}

/// New values for an existing phrase. Anything equal to the current value is left alone, so
/// its original quoting and layout survive.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhraseEdit {
    /// 1 or more triggers. Written as `trigger:` for one and a `triggers:` list for several.
    pub triggers: Vec<String>,
    /// The label; `None` or an empty text removes it.
    #[serde(default)]
    pub label: Option<String>,
    /// New text of a plain `replace` body; `None` leaves the body alone.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub comment: CommentChange,
    /// `None` leaves every option as it is. `Some` is the complete wanted set: an option left
    /// unset in it is removed from the entry, one equal to the current value is not touched.
    #[serde(default)]
    pub options: Option<PhraseOptions>,
}

/// A phrase to append to a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPhrase {
    pub triggers: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
    pub body: String,
    /// Written as a `# comment` block above the entry.
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub options: PhraseOptions,
}

// ---------------------------------------------------------------------------------------
// Line helpers
// ---------------------------------------------------------------------------------------

fn text_of(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

fn indent(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn is_blank(line: &str) -> bool {
    text_of(line).trim().is_empty()
}

fn split_lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(String::from).collect()
}

/// The line ending new lines should use: the file's first one.
fn detect_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// Joins lines back into text: every line but the last gets a terminator, and the file ends
/// with a newline exactly when the original did.
fn finish(mut lines: Vec<String>, eol: &str, ends_nl: bool) -> String {
    let n = lines.len();
    for (i, l) in lines.iter_mut().enumerate() {
        if i + 1 < n && !l.ends_with('\n') {
            l.push_str(eol);
        }
    }
    let mut out = lines.concat();
    if ends_nl {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
    } else if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

/// A YAML double-quoted scalar for `s`.
fn dq(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if c.is_control() || c == '\u{feff}' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Whether `core` (no trailing newline) can be written as a literal block scalar and read
/// back identically with the auto-detected indentation.
fn block_safe(core: &str) -> bool {
    let first = core.chars().next();
    matches!(first, Some(c) if !c.is_whitespace())
        && !core.chars().any(|c| {
            (c.is_control() && c != '\n' && c != '\t')
                || matches!(c, '\u{feff}' | '\u{85}' | '\u{2028}' | '\u{2029}')
        })
}

/// Lines of `key: value` for a body, without the leading indentation of the first line.
/// Continuation lines carry absolute indentation. `value` includes any trailing newlines.
fn render_value(key: &str, value: &str, k: usize, step: usize, eol: &str) -> Vec<String> {
    if value.contains('\n') {
        let core = value.trim_end_matches('\n');
        let trailing = value.len() - core.len();
        if trailing <= 1 && block_safe(core) {
            let marker = if trailing == 0 { "|-" } else { "|" };
            let pad = " ".repeat(k + step);
            let mut out = vec![format!("{key}: {marker}{eol}")];
            for line in core.split('\n') {
                if line.is_empty() {
                    out.push(eol.to_string());
                } else {
                    out.push(format!("{pad}{line}{eol}"));
                }
            }
            return out;
        }
    }
    vec![format!("{key}: {}{eol}", dq(value))]
}

fn render_triggers(triggers: &[String], k: usize, step: usize, eol: &str) -> Vec<String> {
    if triggers.len() == 1 {
        return vec![format!("trigger: {}{eol}", dq(&triggers[0]))];
    }
    let pad = " ".repeat(k + step);
    let mut out = vec![format!("triggers:{eol}")];
    for t in triggers {
        out.push(format!("{pad}- {}{eol}", dq(t)));
    }
    out
}

fn render_comment(comment: &str, d: usize, eol: &str) -> Vec<String> {
    let pad = " ".repeat(d);
    comment
        .split('\n')
        .map(|l| {
            if l.is_empty() {
                format!("{pad}#{eol}")
            } else {
                format!("{pad}# {l}{eol}")
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------------------

const UPPERCASE_STYLES: [&str; 3] = ["uppercase", "capitalize", "capitalize_words"];
const FORCE_MODES: [&str; 2] = ["clipboard", "keys"];

/// Checks the options that differ from `old`. Values already in the file are not judged, so
/// an entry with an odd value can still have its other fields edited.
fn validate_options(new: &PhraseOptions, old: &PhraseOptions) -> Result<PhraseOptions, String> {
    if new.uppercase_style != old.uppercase_style {
        if let Some(s) = &new.uppercase_style {
            if !UPPERCASE_STYLES.contains(&s.as_str()) {
                return Err(format!("\"{s}\" is not a known capitalisation style."));
            }
            if new.propagate_case != Some(true) {
                return Err(
                    "A capitalisation style only applies when \"match the case you \
                    type\" is on."
                        .into(),
                );
            }
        }
    }
    if new.force_mode != old.force_mode {
        if let Some(s) = &new.force_mode {
            if !FORCE_MODES.contains(&s.as_str()) {
                return Err(format!("\"{s}\" is not a known insert method."));
            }
        }
    }
    if new.search_terms != old.search_terms {
        for (i, t) in new.search_terms.iter().enumerate() {
            if t.trim().is_empty() || t.contains('\n') {
                return Err("A search term cannot be empty or span several lines.".into());
            }
            if new.search_terms[..i].contains(t) {
                return Err(format!("The search term \"{t}\" is listed twice."));
            }
        }
    }
    Ok(new.clone())
}

/// The scalar options in the order new keys are written.
fn scalar_options(o: &PhraseOptions) -> [(&'static str, Option<String>); 6] {
    let b = |v: Option<bool>| v.map(|v| v.to_string());
    [
        ("word", b(o.word)),
        ("left_word", b(o.left_word)),
        ("right_word", b(o.right_word)),
        ("propagate_case", b(o.propagate_case)),
        ("uppercase_style", o.uppercase_style.clone()),
        ("force_mode", o.force_mode.clone()),
    ]
}

fn render_search_terms(terms: &[String], k: usize, step: usize, eol: &str) -> Vec<String> {
    let pad = " ".repeat(k + step);
    let mut out = vec![format!("search_terms:{eol}")];
    for t in terms {
        out.push(format!("{pad}- {}{eol}", dq(t)));
    }
    out
}

/// Lines of the option keys that are set in `o`, for a new entry.
fn render_options(o: &PhraseOptions, k: usize, step: usize, eol: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (key, v) in scalar_options(o) {
        if let Some(v) = v {
            out.push(format!("{key}: {v}{eol}"));
        }
    }
    if !o.search_terms.is_empty() {
        out.extend(render_search_terms(&o.search_terms, k, step, eol));
    }
    out
}

/// Sets, changes or removes the option keys of `entry` that differ between `old` and `new`.
/// New keys go after the entry's existing ones; everything else is left alone.
fn apply_options(
    entry: &mut EntryText,
    old: &PhraseOptions,
    new: &PhraseOptions,
    eol: &str,
) -> Result<(), String> {
    let (k, step) = (entry.k, entry.step());
    let mut changes: Vec<(&str, Option<Vec<String>>)> = Vec::new();
    for ((key, o), (_, n)) in scalar_options(old).into_iter().zip(scalar_options(new)) {
        if o != n {
            changes.push((key, n.map(|v| vec![format!("{key}: {v}{eol}")])));
        }
    }
    if old.search_terms != new.search_terms {
        changes.push((
            "search_terms",
            (!new.search_terms.is_empty())
                .then(|| render_search_terms(&new.search_terms, k, step, eol)),
        ));
    }
    for (key, lines) in changes {
        let pos = entry.position(&[key]);
        if pos.len() > 1 {
            return Err(UNSUPPORTED_ENTRY.into());
        }
        match (lines, pos.first()) {
            (Some(own), Some(&p)) => entry.fields[p].own = own,
            (Some(own), None) => entry.fields.push(Field {
                key: key.to_string(),
                own,
                tail: Vec::new(),
            }),
            (None, Some(&p)) => entry.remove_field(p)?,
            (None, None) => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Request normalisation
// ---------------------------------------------------------------------------------------

fn validate_triggers(triggers: &[String]) -> Result<Vec<String>, String> {
    if triggers.is_empty() {
        return Err("A phrase needs at least one trigger.".into());
    }
    for (i, t) in triggers.iter().enumerate() {
        if t.trim().is_empty() {
            return Err("A trigger cannot be empty.".into());
        }
        if triggers[..i].contains(t) {
            return Err(format!("The trigger \"{t}\" is listed twice."));
        }
    }
    Ok(triggers.to_vec())
}

fn normalize_label(label: &Option<String>) -> Option<String> {
    label
        .as_ref()
        .map(|l| l.replace("\r\n", "\n"))
        .filter(|l| !l.trim().is_empty())
}

fn normalize_body(body: &str) -> String {
    body.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string()
}

fn normalize_comment(c: &str) -> Option<String> {
    let text = c.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    let joined = lines.join("\n");
    let trimmed = joined.trim_matches('\n');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

// ---------------------------------------------------------------------------------------
// Parsing a file into editable pieces
// ---------------------------------------------------------------------------------------

/// Number of entries in `matches:`, as the YAML parser counts them.
fn entry_count(text: &str) -> Result<usize, String> {
    let doc: Value =
        serde_norway::from_str(text).map_err(|e| format!("The file is not valid YAML: {e}"))?;
    match doc {
        Value::Null => Ok(0),
        Value::Mapping(m) => match m.get("matches") {
            None | Some(Value::Null) => Ok(0),
            Some(Value::Sequence(s)) => Ok(s.len()),
            Some(_) => Err("`matches` is not a list.".into()),
        },
        _ => Err("The file is not a YAML mapping.".into()),
    }
}

struct Ctx {
    lines: Vec<String>,
    eol: &'static str,
    ends_nl: bool,
    spans: Vec<EntrySpan>,
    phrases: Vec<Phrase>,
    count: usize,
}

impl Ctx {
    fn load(text: &str) -> Result<Ctx, String> {
        let count = entry_count(text)?;
        let spans = scan_entries(text)
            .filter(|s| s.len() == count)
            .ok_or(UNSUPPORTED_FILE)?;
        Ok(Ctx {
            lines: split_lines(text),
            eol: detect_eol(text),
            ends_nl: text.is_empty() || text.ends_with('\n'),
            spans,
            phrases: parse_match_text(text).0,
            count,
        })
    }

    fn phrase(&self, index: usize) -> Result<&Phrase, String> {
        if index >= self.count {
            return Err(format!("There is no entry #{} in this file.", index + 1));
        }
        self.phrases
            .iter()
            .find(|p| p.index == index)
            .ok_or_else(|| {
                format!(
                    "Entry #{} could not be read, so xpando leaves it untouched.",
                    index + 1
                )
            })
    }

    /// One past the last line that belongs to the entry: trailing blank lines and stand-alone
    /// comments (at or left of the entry's `-`) are not part of it.
    fn body_end(&self, span: &EntrySpan) -> usize {
        let d = indent(text_of(&self.lines[span.start_line]));
        let mut e = span.end_line;
        while e > span.start_line + 1 {
            let t = text_of(&self.lines[e - 1]);
            let tt = t.trim();
            if tt.is_empty() || (tt.starts_with('#') && indent(t) <= d) {
                e -= 1;
            } else {
                break;
            }
        }
        e
    }
}

struct Field {
    key: String,
    /// The field's own lines. The first one has its leading `k` columns (indentation and,
    /// for the entry's first field, the `- `) removed.
    own: Vec<String>,
    /// Blank and comment lines after the field, kept in place when it is rewritten.
    tail: Vec<String>,
}

struct EntryText {
    /// The text before the first key, e.g. `  - `.
    prefix: String,
    /// Column of the keys.
    k: usize,
    fields: Vec<Field>,
}

fn field_key(rest: &str) -> Option<String> {
    let end = rest.find(':')?;
    let key = &rest[..end];
    let after = &rest[end + 1..];
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !key.starts_with(|c: char| c.is_ascii_digit() || c == '-');
    (valid && (after.is_empty() || after.starts_with([' ', '\t']))).then(|| key.to_string())
}

fn split_entry(lines: &[String]) -> Result<EntryText, String> {
    let first = text_of(&lines[0]);
    let d = indent(first);
    let after_indent = &first[d..];
    let rest = after_indent.strip_prefix('-').ok_or(UNSUPPORTED_ENTRY)?;
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let k = d + 1 + spaces;
    if first.len() <= k
        || first[k..].trim().is_empty()
        || first[k..].starts_with('#')
        || first[..d].contains('\t')
    {
        return Err(UNSUPPORTED_ENTRY.into());
    }

    let mut segs: Vec<(String, Vec<String>)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = text_of(l);
        let starts_field = if i == 0 {
            true
        } else {
            let tt = t.trim();
            let ind = indent(t);
            !tt.is_empty()
                && !tt.starts_with('#')
                && ind <= k
                && !(ind == k && (tt == "-" || tt.starts_with("- ")))
        };
        if starts_field {
            if i > 0 && (indent(t) != k || t[..k].contains('\t')) {
                return Err(UNSUPPORTED_ENTRY.into());
            }
            let key = field_key(&t[k..]).ok_or(UNSUPPORTED_ENTRY)?;
            segs.push((key, vec![l[k..].to_string()]));
        } else {
            segs.last_mut().ok_or(UNSUPPORTED_ENTRY)?.1.push(l.clone());
        }
    }

    let fields = segs
        .into_iter()
        .map(|(key, mut own)| {
            let mut tail = Vec::new();
            while own.len() > 1 {
                let t = text_of(own.last().unwrap());
                let tt = t.trim();
                if tt.is_empty() || (tt.starts_with('#') && indent(t) <= k) {
                    tail.push(own.pop().unwrap());
                } else {
                    break;
                }
            }
            tail.reverse();
            Field { key, own, tail }
        })
        .collect();
    Ok(EntryText {
        prefix: first[..k].to_string(),
        k,
        fields,
    })
}

impl EntryText {
    fn assemble(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut first = true;
        for f in &self.fields {
            for (i, l) in f.own.iter().enumerate() {
                if i == 0 {
                    let pre = if first {
                        self.prefix.clone()
                    } else {
                        " ".repeat(self.k)
                    };
                    out.push(format!("{pre}{l}"));
                    first = false;
                } else {
                    out.push(l.clone());
                }
            }
            out.extend(f.tail.iter().cloned());
        }
        out
    }

    fn position(&self, keys: &[&str]) -> Vec<usize> {
        (0..self.fields.len())
            .filter(|&i| keys.contains(&self.fields[i].key.as_str()))
            .collect()
    }

    /// Indentation step of nested content, read from the entry itself; 2 when unknown.
    fn step(&self) -> usize {
        self.fields
            .iter()
            .flat_map(|f| f.own.iter().skip(1))
            .map(|l| text_of(l))
            .filter(|t| !t.trim().is_empty() && indent(t) > self.k)
            .map(|t| indent(t) - self.k)
            .next()
            .unwrap_or(2)
            .clamp(1, 8)
    }

    fn remove_field(&mut self, pos: usize) -> Result<(), String> {
        let f = self.fields.remove(pos);
        if !f.tail.is_empty() {
            if pos == 0 {
                return Err(UNSUPPORTED_ENTRY.into());
            }
            self.fields[pos - 1].tail.extend(f.tail);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// Safety net
// ---------------------------------------------------------------------------------------

fn without_line(p: &Phrase) -> Phrase {
    Phrase {
        line: None,
        ..p.clone()
    }
}

enum Op<'a> {
    Update { index: usize, expected: &'a Phrase },
    Add { expected: &'a Phrase },
    Delete { index: usize },
}

/// Re-parses `new` and checks it against `old` and the operation. Any difference is an error.
fn verify(old: &str, new: &str, old_count: usize, op: Op) -> Result<(), String> {
    let fail = |what: &str| -> Result<(), String> {
        Err(format!(
            "Safety check failed ({what}); nothing was changed. Please report this."
        ))
    };
    let new_count = match entry_count(new) {
        Ok(n) => n,
        Err(_) => return fail("the result is not valid YAML"),
    };
    let expected_count = match op {
        Op::Update { .. } => old_count,
        Op::Add { .. } => old_count + 1,
        Op::Delete { .. } => old_count - 1,
    };
    if new_count != expected_count {
        return fail("entry count differs");
    }
    if scan_entries(new).map(|s| s.len()) != Some(new_count) {
        return fail("entry layout not recognised");
    }
    let (old_p, old_w) = parse_match_text(old);
    let (new_p, new_w) = parse_match_text(new);
    if !matches!(op, Op::Delete { .. }) && old_w != new_w {
        return fail("warnings differ");
    }
    if matches!(op, Op::Delete { .. }) && old_w.len() != new_w.len() {
        return fail("warnings differ");
    }

    // Every other phrase must be identical (only `line` may move).
    let others: Vec<Phrase> = old_p
        .iter()
        .filter_map(|p| match &op {
            Op::Update { index, .. } if p.index == *index => None,
            Op::Delete { index } if p.index == *index => None,
            Op::Delete { index } if p.index > *index => Some(Phrase {
                index: p.index - 1,
                ..without_line(p)
            }),
            _ => Some(without_line(p)),
        })
        .collect();
    let edited_index = match &op {
        Op::Update { index, .. } => Some(*index),
        Op::Add { .. } => Some(old_count),
        Op::Delete { .. } => None,
    };
    let new_others: Vec<Phrase> = new_p
        .iter()
        .filter(|p| Some(p.index) != edited_index)
        .map(without_line)
        .collect();
    if others != new_others {
        return fail("another phrase changed");
    }
    let (index, expected) = match op {
        Op::Update { index, expected } => (index, expected),
        Op::Add { expected } => (old_count, expected),
        Op::Delete { .. } => return Ok(()),
    };
    match new_p.iter().find(|p| p.index == index) {
        Some(p) if without_line(p) == without_line(expected) => Ok(()),
        _ => fail("the edited phrase differs from the request"),
    }
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

/// Number of `\n` that end the raw `replace` value of entry `index` (a `|` block keeps one).
fn replace_trailing_newlines(text: &str, index: usize) -> usize {
    let Ok(Value::Mapping(doc)) = serde_norway::from_str::<Value>(text) else {
        return 0;
    };
    doc.get("matches")
        .and_then(Value::as_sequence)
        .and_then(|s| s.get(index))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get("replace"))
        .and_then(Value::as_str)
        .map(|s| s.len() - s.trim_end_matches('\n').len())
        .unwrap_or(0)
}

/// Changes the triggers, label, plain-text body and/or comment of entry `index`.
///
/// Values equal to the current ones are left untouched; if nothing changes the input is
/// returned unchanged. Errors (as plain text for the user) leave the input alone.
pub fn update_phrase(text: &str, index: usize, edit: &PhraseEdit) -> Result<String, String> {
    let triggers = validate_triggers(&edit.triggers)?;
    let label = normalize_label(&edit.label);
    let body = edit.body.as_deref().map(normalize_body);

    let ctx = Ctx::load(text)?;
    let old = ctx.phrase(index)?;

    let new_comment = match &edit.comment {
        CommentChange::Keep => old.comment.clone(),
        CommentChange::Clear => None,
        CommentChange::Set(c) => normalize_comment(c),
    };
    let trig_changed = triggers != old.triggers;
    let label_changed = label != old.label;
    let body_changed = body.as_ref().is_some_and(|b| *b != old.content);
    let comment_changed = new_comment != old.comment;
    let new_options = match &edit.options {
        None => old.options.clone(),
        Some(o) => validate_options(o, &old.options)?,
    };
    let options_changed = new_options != old.options;
    if !(trig_changed || label_changed || body_changed || comment_changed || options_changed) {
        return Ok(text.to_string());
    }
    if trig_changed && old.is_regex {
        return Err("The trigger of a regex phrase cannot be edited here.".into());
    }
    if body_changed && old.kind != "replace" {
        return Err(format!(
            "Only plain-text phrases can have their text edited; this one uses `{}`.",
            old.kind
        ));
    }

    let span = &ctx.spans[index];
    let be = ctx.body_end(span);
    let mut entry = split_entry(&ctx.lines[span.start_line..be])?;
    let (k, step, eol) = (entry.k, entry.step(), ctx.eol);

    if trig_changed {
        let pos = entry.position(&["trigger", "triggers"]);
        if pos.len() != 1 {
            return Err(UNSUPPORTED_ENTRY.into());
        }
        let rendered = render_triggers(&triggers, k, step, eol);
        let f = &mut entry.fields[pos[0]];
        f.key = if triggers.len() == 1 {
            "trigger"
        } else {
            "triggers"
        }
        .to_string();
        f.own = rendered;
    }
    if body_changed {
        let pos = entry.position(&["replace"]);
        if pos.len() != 1 {
            return Err(UNSUPPORTED_ENTRY.into());
        }
        let newlines = replace_trailing_newlines(text, index);
        let value = format!("{}{}", body.as_deref().unwrap_or(""), "\n".repeat(newlines));
        entry.fields[pos[0]].own = render_value("replace", &value, k, step, eol);
    }
    if label_changed {
        let existing = entry.position(&["label"]);
        match (&label, existing.first()) {
            (Some(l), Some(&p)) => {
                entry.fields[p].own = render_value("label", l, k, step, eol);
            }
            (Some(l), None) => {
                let anchor = entry
                    .position(&["replace", "html", "markdown", "image_path", "form"])
                    .last()
                    .copied()
                    .or_else(|| entry.position(&["trigger", "triggers"]).first().copied())
                    .ok_or(UNSUPPORTED_ENTRY)?;
                entry.fields.insert(
                    anchor + 1,
                    Field {
                        key: "label".into(),
                        own: render_value("label", l, k, step, eol),
                        tail: Vec::new(),
                    },
                );
            }
            (None, Some(&p)) => entry.remove_field(p)?,
            (None, None) => {}
        }
    }

    if options_changed {
        apply_options(&mut entry, &old.options, &new_options, eol)?;
    }

    let d = indent(text_of(&ctx.lines[span.start_line]));
    let mut out: Vec<String> = ctx.lines[..span.first_line].to_vec();
    if comment_changed {
        if let Some(c) = &new_comment {
            out.extend(render_comment(c, d, eol));
        }
    } else {
        out.extend(ctx.lines[span.first_line..span.start_line].iter().cloned());
    }
    out.extend(entry.assemble());
    out.extend(ctx.lines[be..].iter().cloned());
    let new_text = finish(out, eol, ctx.ends_nl);

    let mut expected = old.clone();
    expected.triggers = triggers;
    expected.label = label;
    expected.comment = new_comment;
    expected.options = new_options;
    if let Some(b) = &body {
        expected.content = b.clone();
        expected.preview = make_preview(b);
    }
    verify(
        text,
        &new_text,
        ctx.count,
        Op::Update {
            index,
            expected: &expected,
        },
    )?;
    Ok(new_text)
}

/// Appends a plain-text phrase at the end of the `matches:` block, in the file's style.
pub fn add_phrase(text: &str, new: &NewPhrase) -> Result<String, String> {
    let triggers = validate_triggers(&new.triggers)?;
    let label = normalize_label(&new.label);
    let body = normalize_body(&new.body);
    let comment = new.comment.as_deref().and_then(normalize_comment);
    let options = validate_options(&new.options, &PhraseOptions::default())?;

    let count = entry_count(text)?;
    let eol = detect_eol(text);
    let ends_nl = text.is_empty() || text.ends_with('\n');
    let mut lines = split_lines(text);

    // Where the new entry goes, its dash column, key offset and nested step, and whether to
    // separate it from the previous entry with a blank line.
    let (insert_at, d, offset, step, blank_before);
    if count > 0 {
        let ctx = Ctx::load(text)?;
        let last = ctx.spans.last().ok_or(UNSUPPORTED_FILE)?;
        let be = ctx.body_end(last);
        let entry = split_entry(&ctx.lines[last.start_line..be])?;
        d = indent(text_of(&ctx.lines[last.start_line]));
        offset = entry.k - d;
        step = entry.step();
        let style_blank = ctx
            .spans
            .iter()
            .skip(1)
            .any(|s| s.first_line > 0 && is_blank(&ctx.lines[s.first_line - 1]));
        let n = lines.len();
        let trailing_blanks = (be..last.end_line)
            .take_while(|&i| is_blank(&lines[i]))
            .count();
        if last.end_line == n && be + trailing_blanks == n {
            // Only blank lines follow: the entry goes after them.
            insert_at = n;
            blank_before = trailing_blanks == 0 && style_blank;
        } else {
            insert_at = be;
            blank_before = style_blank;
        }
    } else {
        d = 2;
        offset = 2;
        step = 2;
        blank_before = false;
        let header = lines
            .iter()
            .position(|l| text_of(l).trim_end() == "matches:");
        let flow_empty = lines
            .iter()
            .position(|l| text_of(l).trim_end() == "matches: []");
        match (header, flow_empty) {
            (Some(i), _) => insert_at = i + 1,
            (None, Some(i)) => {
                lines[i] = format!("matches:{eol}");
                insert_at = i + 1;
            }
            (None, None) => {
                let has_key = matches!(
                    serde_norway::from_str::<Value>(text),
                    Ok(Value::Mapping(m)) if m.contains_key("matches")
                );
                if has_key {
                    return Err(UNSUPPORTED_FILE.into());
                }
                lines.push(format!("matches:{eol}"));
                insert_at = lines.len();
            }
        }
    }

    let mut block = Vec::new();
    if blank_before {
        block.push(eol.to_string());
    }
    if let Some(c) = &comment {
        block.extend(render_comment(c, d, eol));
    }
    let k = d + offset;
    let mut entry_lines = render_triggers(&triggers, k, step, eol);
    entry_lines.extend(render_value("replace", &body, k, step, eol));
    if let Some(l) = &label {
        entry_lines.extend(render_value("label", l, k, step, eol));
    }
    entry_lines.extend(render_options(&options, k, step, eol));
    let prefix = format!("{}-{}", " ".repeat(d), " ".repeat(offset - 1));
    for (i, l) in entry_lines.into_iter().enumerate() {
        block.push(if i == 0 {
            format!("{prefix}{l}")
        } else if l.starts_with(' ') || l == eol {
            l
        } else {
            format!("{}{l}", " ".repeat(k))
        });
    }
    lines.splice(insert_at..insert_at, block);
    let new_text = finish(lines, eol, ends_nl);

    let expected = Phrase {
        triggers,
        is_regex: false,
        label,
        kind: "replace".into(),
        preview: make_preview(&body),
        content: body,
        vars: Vec::new(),
        comment,
        index: count,
        line: None,
        options,
    };
    verify(
        text,
        &new_text,
        count,
        Op::Add {
            expected: &expected,
        },
    )?;
    Ok(new_text)
}

/// Removes entry `index` together with its own comment block. Neighbours, their comments and
/// stand-alone comments between entries are left alone.
pub fn delete_phrase(text: &str, index: usize) -> Result<String, String> {
    let ctx = Ctx::load(text)?;
    ctx.phrase(index)?;
    let span = &ctx.spans[index];
    let be = ctx.body_end(span);
    let mut lines = ctx.lines.clone();
    lines.drain(span.first_line..be);

    // Avoid leaving two blank lines where the entry used to be.
    let p = span.first_line;
    let after_gap =
        p > 0 && (is_blank(&lines[p - 1]) || text_of(&lines[p - 1]).trim_end() == "matches:");
    if after_gap && p < lines.len() && is_blank(&lines[p]) {
        while p < lines.len() && is_blank(&lines[p]) {
            lines.remove(p);
        }
    } else if p >= lines.len() {
        while lines.last().is_some_and(|l| is_blank(l)) {
            lines.pop();
        }
    }

    let new_text = finish(lines, ctx.eol, ctx.ends_nl);
    verify(text, &new_text, ctx.count, Op::Delete { index })?;
    Ok(new_text)
}

// ---------------------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------------------

/// FNV-1a 64-bit hash of `bytes` as 16 hex digits. Detects changes; not for security.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Why [`save_file`] did not write.
#[derive(Debug)]
pub enum SaveError {
    /// The file no longer has the content it had when it was loaded.
    Conflict,
    Io(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Conflict => write!(
                f,
                "{CONFLICT_PREFIX} This file was changed outside xpando after it was loaded. \
                 Reload to see the current version, then redo your edit."
            ),
            SaveError::Io(e) => write!(f, "Could not write the file: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `new_text` to `path` if the file still hashes to `expected_hash`.
///
/// The first save of a file keeps its original as `<name>.bak` (espanso ignores it). The new
/// content goes to `<name>.tmp` next to the file and is renamed over it, so espanso never
/// sees a half-written file.
pub fn save_file(path: &Path, new_text: &str, expected_hash: &str) -> Result<(), SaveError> {
    let io_err = |e: io::Error| SaveError::Io(e.to_string());
    let current = fs::read(path).map_err(io_err)?;
    if content_hash(&current) != expected_hash {
        return Err(SaveError::Conflict);
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sibling(path, ".bak"))
    {
        Ok(mut f) => f.write_all(&current).map_err(io_err)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io_err(e)),
    }
    let tmp = sibling(path, ".tmp");
    let written = fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(new_text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(e));
    }
    Ok(())
}

/// Resolves a match file given as a path relative to `<config_dir>/match` (`/` separators)
/// to a real file, refusing anything that would leave that directory.
pub fn resolve_match_file(config_dir: &Path, file: &str) -> Result<PathBuf, String> {
    let bad = || format!("\"{file}\" is not a match file.");
    if file.is_empty() || file.contains(['\\', ':', '\0']) {
        return Err(bad());
    }
    let mut rel = PathBuf::new();
    for c in Path::new(file).components() {
        match c {
            Component::Normal(p) => rel.push(p),
            _ => return Err(bad()),
        }
    }
    if !matches!(
        rel.extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    ) {
        return Err(bad());
    }
    let root = fs::canonicalize(config_dir.join("match")).map_err(|_| bad())?;
    let path = fs::canonicalize(root.join(&rel)).map_err(|_| bad())?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(bad());
    }
    Ok(path)
}

/// Loads `file`, checks it still hashes to `expected_hash`, lets `edit` turn its text into
/// new text, saves that and returns the reloaded folder.
pub fn edit_file(
    config_dir: &Path,
    file: &str,
    expected_hash: &str,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Folder, String> {
    let path = resolve_match_file(config_dir, file)?;
    let bytes = fs::read(&path).map_err(|e| format!("Could not read the file: {e}"))?;
    if content_hash(&bytes) != expected_hash {
        return Err(SaveError::Conflict.to_string());
    }
    let text = String::from_utf8(bytes).map_err(|_| "The file is not valid UTF-8.".to_string())?;
    let new_text = edit(&text)?;
    if new_text != text {
        save_file(&path, &new_text, expected_hash).map_err(|e| e.to_string())?;
    }
    super::load_folders(config_dir)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|f| f.file == file)
        .ok_or_else(|| "The file disappeared after saving.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe(triggers: &[&str], label: Option<&str>, body: Option<&str>) -> PhraseEdit {
        PhraseEdit {
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            label: label.map(String::from),
            body: body.map(String::from),
            comment: CommentChange::Keep,
            options: None,
        }
    }

    fn np(triggers: &[&str], label: Option<&str>, body: &str) -> NewPhrase {
        NewPhrase {
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            label: label.map(String::from),
            body: body.into(),
            comment: None,
            options: PhraseOptions::default(),
        }
    }

    /// Edit that keeps every field of phrase `index` as it is.
    fn same(text: &str, index: usize) -> PhraseEdit {
        let p = parse_match_text(text)
            .0
            .into_iter()
            .find(|p| p.index == index)
            .unwrap();
        PhraseEdit {
            triggers: p.triggers,
            label: p.label,
            body: (p.kind == "replace").then_some(p.content),
            comment: CommentChange::Keep,
            options: None,
        }
    }

    const BASE: &str = "# espanso match file\n\nmatches:\n  # Simple text replacement\n  - trigger: \":espanso\"\n    replace: \"Hi there!\"\n\n  # Print the current date\n  - trigger: \":date\"\n    replace: \"{{mydate}}\"\n    vars:\n      - name: mydate\n        type: date\n        params:\n          format: \"%m/%d/%Y\"\n\n  # PhraseExpress: {#datetime}\n  - trigger: \"TDC\"\n    replace: \"{{out}}\"\n    vars:\n      - name: out\n        type: echo\n        params:\n          echo: \"{{d}}\"\n      # inner comment\n      - name: s\n        type: shell\n        params:\n          cmd: \"echo 'x'\"\n";

    #[test]
    fn edit_first_middle_last_keep_comments() {
        let t = update_phrase(BASE, 0, &pe(&[":hello"], None, None)).unwrap();
        assert_eq!(t, BASE.replace("\":espanso\"", "\":hello\""));
        let t = update_phrase(BASE, 1, &pe(&[":date"], None, Some("Today"))).unwrap();
        assert!(t.contains("  # Print the current date\n  - trigger: \":date\"\n    replace: \"Today\"\n    vars:\n"));
        assert!(t.contains("  # PhraseExpress: {#datetime}\n  - trigger: \"TDC\""));
        let t = update_phrase(BASE, 2, &pe(&["TDC", "tdc"], Some("Stamp"), None)).unwrap();
        assert!(t.contains(
            "  - triggers:\n      - \"TDC\"\n      - \"tdc\"\n    replace: \"{{out}}\"\n    label: \"Stamp\"\n    vars:\n"
        ));
        assert!(t.contains("      # inner comment\n"));
        // Everything before the edited entry is byte-identical.
        let cut = BASE.find("  - trigger: \"TDC\"").unwrap();
        assert!(t.starts_with(&BASE[..cut]));
    }

    #[test]
    fn noop_is_byte_identical_for_every_shape() {
        let shapes = [
            BASE,
            "matches:\n  - trigger: a\n    replace: x\n",
            "matches:\n  - trigger: 'a'\n    replace: 'it''s'\n",
            "matches:\r\n  - trigger: a\r\n    replace: |\r\n      one\r\n      two\r\n",
            "matches:\n  - trigger: a\n    replace: x",
            "matches:\n  - triggers: [\":a\", \":b\"]\n    replace: X\n    label: L\n",
            "matches:\n  - triggers:\n      - \":a\"\n      - \":b\"\n    replace: \"tab\\there \\\"q\\\"\"\n",
            "matches:\n  - trigger: h\n    html: \"<b>x</b>\"\n  - trigger: m\n    markdown: \"**x**\"\n  - trigger: f\n    form: \"Hi [[name]]\"\n    form_fields:\n      name:\n        type: text\n  - trigger: i\n    image_path: /a/b.png\n  - regex: ':h(\\d+)'\n    replace: x\n",
            "matches:\n    - trigger: a\n      replace: |\n          deep\n    - trigger: b\n      replace: y\n\n\n",
            "matches:\n  - trigger: a\n    replace: |+\n      keep\n\n  - trigger: b\n    replace: y\nglobal_vars:\n  - name: x\n    type: echo\n",
            "matches:\n  - trigger: a  # why\n    replace: x # because\n",
        ];
        for text in shapes {
            let (p, _) = parse_match_text(text);
            assert!(!p.is_empty());
            for ph in &p {
                let out = update_phrase(text, ph.index, &same(text, ph.index)).unwrap();
                assert_eq!(out, text, "entry {} of {text:?}", ph.index);
            }
        }
    }

    #[test]
    fn block_scalar_and_multiline() {
        let text = "matches:\n  - trigger: a\n    replace: |\n      line one\n      line two\n    label: A\n  - trigger: b\n    replace: x\n";
        let t = update_phrase(
            text,
            0,
            &pe(&["a"], Some("A"), Some("new one\nnew two\n\nlast")),
        )
        .unwrap();
        assert_eq!(
            t,
            "matches:\n  - trigger: a\n    replace: |\n      new one\n      new two\n\n      last\n    label: A\n  - trigger: b\n    replace: x\n"
        );
        // Single line -> multi-line -> single line, with `|-` for the new text.
        let t = update_phrase(text, 1, &pe(&["b"], None, Some("x\ny"))).unwrap();
        assert!(t.ends_with("  - trigger: b\n    replace: |-\n      x\n      y\n"));
        let back = update_phrase(&t, 1, &pe(&["b"], None, Some("just \"one\""))).unwrap();
        assert!(back.ends_with("    replace: \"just \\\"one\\\"\"\n"));
        let p = &parse_match_text(&back).0[1];
        assert_eq!(p.content, "just \"one\"");
    }

    #[test]
    fn escapes_round_trip() {
        let body = "tab\there \\ back \"q\" é 日本 \u{1}x";
        let t = update_phrase(
            "matches:\n  - trigger: a\n    replace: x\n",
            0,
            &pe(&["a"], None, Some(body)),
        )
        .unwrap();
        assert_eq!(parse_match_text(&t).0[0].content, body);
        // Leading-space multi-line text falls back to an escaped scalar.
        let body = "  indented\nsecond";
        let t = update_phrase(&t, 0, &pe(&["a"], None, Some(body))).unwrap();
        assert_eq!(parse_match_text(&t).0[0].content, body);
    }

    #[test]
    fn keeps_trailing_newline_of_block_value() {
        let text = "matches:\n  - trigger: a\n    replace: |\n      one\n";
        let t = update_phrase(text, 0, &pe(&["a"], None, Some("two"))).unwrap();
        assert_eq!(t, "matches:\n  - trigger: a\n    replace: |\n      two\n");
    }

    #[test]
    fn crlf_and_no_trailing_newline() {
        let text = "matches:\r\n  # c\r\n  - trigger: a\r\n    replace: x\r\n\r\n  - trigger: b\r\n    replace: y";
        let t = update_phrase(text, 1, &pe(&["b"], Some("B"), Some("l1\nl2"))).unwrap();
        assert_eq!(
            t,
            "matches:\r\n  # c\r\n  - trigger: a\r\n    replace: x\r\n\r\n  - trigger: b\r\n    replace: |-\r\n      l1\r\n      l2\r\n    label: \"B\""
        );
        let t = update_phrase(text, 0, &pe(&["a", "aa"], None, None)).unwrap();
        assert!(t.contains(
            "  # c\r\n  - triggers:\r\n      - \"a\"\r\n      - \"aa\"\r\n    replace: x\r\n"
        ));
        assert!(!t.ends_with('\n'));
        assert!(!t.replace("\r\n", "").contains('\n'));
        let t = add_phrase(text, &np(&["c"], None, "z")).unwrap();
        assert!(t.ends_with("    replace: y\r\n\r\n  - trigger: \"c\"\r\n    replace: \"z\""));
        let t = delete_phrase(text, 1).unwrap();
        assert_eq!(t, "matches:\r\n  # c\r\n  - trigger: a\r\n    replace: x");
    }

    #[test]
    fn label_set_change_clear() {
        let text = "matches:\n  - trigger: a\n    label: Old\n    replace: x\n    vars: []\n";
        let t = update_phrase(text, 0, &pe(&["a"], Some("New"), None)).unwrap();
        assert_eq!(t, text.replace("label: Old", "label: \"New\""));
        let t = update_phrase(text, 0, &pe(&["a"], None, None)).unwrap();
        assert_eq!(
            t,
            "matches:\n  - trigger: a\n    replace: x\n    vars: []\n"
        );
        let t = update_phrase(text, 0, &pe(&["a"], Some("  "), None)).unwrap();
        assert!(!t.contains("label"));
        // Removing a label that is the first key moves the dash to the next key.
        let first = "matches:\n  - label: L\n    trigger: a\n    replace: x\n";
        let t = update_phrase(first, 0, &pe(&["a"], None, None)).unwrap();
        assert_eq!(t, "matches:\n  - trigger: a\n    replace: x\n");
    }

    #[test]
    fn untouchable_entries_refuse() {
        let text = "matches:\n  - trigger: h\n    html: \"<b>x</b>\"\n  - regex: ':a'\n    replace: x\n  - trigger: v\n    replace: \"{{d}}\"\n    vars:\n      - name: d\n        type: date\n";
        assert!(update_phrase(text, 0, &pe(&["h"], None, Some("plain"))).is_err());
        let e = update_phrase(text, 1, &pe(&["other"], None, None)).unwrap_err();
        assert!(e.contains("regex"));
        // Label of an html entry is fine; vars stay byte-for-byte.
        let t = update_phrase(text, 0, &pe(&["h"], Some("Bold"), None)).unwrap();
        assert!(t.contains("    html: \"<b>x</b>\"\n    label: \"Bold\"\n"));
        let t = update_phrase(text, 2, &pe(&["v2"], None, Some("{{d}} today"))).unwrap();
        assert!(t.ends_with("    vars:\n      - name: d\n        type: date\n"));
    }

    #[test]
    fn validates_requests() {
        let text = "matches:\n  - trigger: a\n    replace: x\n";
        assert!(update_phrase(text, 0, &pe(&[], None, None)).is_err());
        assert!(update_phrase(text, 0, &pe(&[" "], None, None)).is_err());
        assert!(update_phrase(text, 0, &pe(&["a", "a"], None, None)).is_err());
        assert!(update_phrase(text, 5, &pe(&["a"], None, None)).is_err());
        assert!(delete_phrase(text, 5).is_err());
        assert!(add_phrase(text, &np(&[], None, "x")).is_err());
    }

    #[test]
    fn unparseable_entries_are_skipped_and_untouched() {
        let text = "matches:\n  # first\n  - trigger: a\n    replace: x\n  # broken\n  - just text\n  # third\n  - trigger: c\n    replace: z\n";
        let t = update_phrase(text, 2, &pe(&["cc"], None, Some("zz"))).unwrap();
        assert_eq!(
            t,
            "matches:\n  # first\n  - trigger: a\n    replace: x\n  # broken\n  - just text\n  # third\n  - trigger: \"cc\"\n    replace: \"zz\"\n"
        );
        assert!(update_phrase(text, 1, &pe(&["q"], None, None)).is_err());
        assert!(delete_phrase(text, 1).is_err());
        let t = delete_phrase(text, 2).unwrap();
        assert_eq!(
            t,
            "matches:\n  # first\n  - trigger: a\n    replace: x\n  # broken\n  - just text\n"
        );
        let t = delete_phrase(text, 0).unwrap();
        assert!(t.starts_with("matches:\n  # broken\n  - just text\n  # third\n"));
        let t = add_phrase(text, &np(&["n"], None, "new")).unwrap();
        assert_eq!(parse_match_text(&t).0.last().unwrap().index, 3);
    }

    #[test]
    fn comment_edits() {
        let mut e = pe(&["a"], None, None);
        e.comment = CommentChange::Set("new note\nsecond".into());
        let text =
            "matches:\n  # old\n  - trigger: a\n    replace: x\n  - trigger: b\n    replace: y\n";
        let t = update_phrase(text, 0, &e).unwrap();
        assert_eq!(
            t,
            "matches:\n  # new note\n  # second\n  - trigger: a\n    replace: x\n  - trigger: b\n    replace: y\n"
        );
        e.comment = CommentChange::Clear;
        let t = update_phrase(text, 0, &e).unwrap();
        assert_eq!(
            t,
            "matches:\n  - trigger: a\n    replace: x\n  - trigger: b\n    replace: y\n"
        );
        let mut e = pe(&["b"], None, None);
        e.comment = CommentChange::Set("about b".into());
        let t = update_phrase(text, 1, &e).unwrap();
        assert!(t.contains("    replace: x\n  # about b\n  - trigger: b\n"));
    }

    #[test]
    fn delete_first_middle_last() {
        let t = delete_phrase(BASE, 0).unwrap();
        assert_eq!(
            t,
            BASE.replace(
                "  # Simple text replacement\n  - trigger: \":espanso\"\n    replace: \"Hi there!\"\n\n",
                ""
            )
        );
        assert!(t.starts_with("# espanso match file\n\nmatches:\n  # Print the current date\n"));
        let t = delete_phrase(BASE, 1).unwrap();
        assert!(t.contains("\"Hi there!\"\n\n  # PhraseExpress: {#datetime}\n"));
        assert!(!t.contains(":date"));
        let t = delete_phrase(BASE, 2).unwrap();
        assert!(t.ends_with("    replace: \"Hi there!\"\n\n  # Print the current date\n  - trigger: \":date\"\n    replace: \"{{mydate}}\"\n    vars:\n      - name: mydate\n        type: date\n        params:\n          format: \"%m/%d/%Y\"\n"));
        // Deleting everything leaves a valid empty list.
        let one = "matches:\n  # c\n  - trigger: a\n    replace: x\nglobal_vars: []\n";
        let t = delete_phrase(one, 0).unwrap();
        assert_eq!(t, "matches:\nglobal_vars: []\n");
        assert!(parse_match_text(&t).0.is_empty());
    }

    #[test]
    fn delete_keeps_standalone_comments() {
        let text = "matches:\n  - trigger: a\n    replace: x\n\n  # standalone\n\n  # about b\n  - trigger: b\n    replace: y\n";
        let t = delete_phrase(text, 1).unwrap();
        assert_eq!(
            t,
            "matches:\n  - trigger: a\n    replace: x\n\n  # standalone\n"
        );
        let t = delete_phrase(text, 0).unwrap();
        assert_eq!(
            t,
            "matches:\n  # standalone\n\n  # about b\n  - trigger: b\n    replace: y\n"
        );
    }

    #[test]
    fn add_styles() {
        // Match the file's layout; blank lines between entries when the file uses them.
        let t = add_phrase(BASE, &np(&[":new"], Some("N"), "multi\nline")).unwrap();
        assert!(t.ends_with(
            "          cmd: \"echo 'x'\"\n\n  - trigger: \":new\"\n    replace: |-\n      multi\n      line\n    label: \"N\"\n"
        ));
        let compact = "matches:\n  - trigger: a\n    replace: x\n  - trigger: b\n    replace: y\n";
        let t = add_phrase(compact, &np(&["c", "d"], None, "z")).unwrap();
        assert_eq!(
            t,
            format!("{compact}  - triggers:\n      - \"c\"\n      - \"d\"\n    replace: \"z\"\n")
        );
        // Four-space layout.
        let wide = "matches:\n    - trigger: a\n      replace: |\n          deep\n";
        let t = add_phrase(wide, &np(&["b"], None, "l1\nl2")).unwrap();
        assert!(
            t.ends_with("    - trigger: \"b\"\n      replace: |-\n          l1\n          l2\n")
        );
        // With a description comment.
        let mut n = np(&["c"], None, "z");
        n.comment = Some("My note".into());
        let t = add_phrase(compact, &n).unwrap();
        assert_eq!(
            parse_match_text(&t).0[2].comment.as_deref(),
            Some("My note")
        );
    }

    #[test]
    fn add_to_empty_and_odd_files() {
        for (text, want) in [
            (
                "matches:\n",
                "matches:\n  - trigger: \"a\"\n    replace: \"x\"\n",
            ),
            (
                "matches: []\n",
                "matches:\n  - trigger: \"a\"\n    replace: \"x\"\n",
            ),
            (
                "matches:",
                "matches:\n  - trigger: \"a\"\n    replace: \"x\"",
            ),
            ("", "matches:\n  - trigger: \"a\"\n    replace: \"x\"\n"),
            (
                "# header\n",
                "# header\nmatches:\n  - trigger: \"a\"\n    replace: \"x\"\n",
            ),
            (
                "matches:\n  # examples\nglobal_vars: []\n",
                "matches:\n  - trigger: \"a\"\n    replace: \"x\"\n  # examples\nglobal_vars: []\n",
            ),
        ] {
            let t = add_phrase(text, &np(&["a"], None, "x")).unwrap();
            assert_eq!(t, want, "from {text:?}");
        }
    }

    #[test]
    fn add_with_trailing_blank_lines_or_following_keys() {
        let text = "matches:\n  # c\n  - trigger: a\n    replace: x\n\n\n";
        let t = add_phrase(text, &np(&["b"], None, "y")).unwrap();
        assert_eq!(
            t,
            "matches:\n  # c\n  - trigger: a\n    replace: x\n\n\n  - trigger: \"b\"\n    replace: \"y\"\n"
        );
        let text = "matches:\n  - trigger: a\n    replace: x\n\nglobal_vars:\n  - name: v\n    type: echo\n";
        let t = add_phrase(text, &np(&["b"], None, "y")).unwrap();
        assert_eq!(
            t,
            "matches:\n  - trigger: a\n    replace: x\n  - trigger: \"b\"\n    replace: \"y\"\n\nglobal_vars:\n  - name: v\n    type: echo\n"
        );
        // A stand-alone trailing comment does not become the new entry's description.
        let text = "matches:\n  - trigger: a\n    replace: x\n  # - trigger: old\n";
        let t = add_phrase(text, &np(&["b"], None, "y")).unwrap();
        assert_eq!(parse_match_text(&t).0[1].comment, None);
        assert!(t.ends_with("  # - trigger: old\n"));
    }

    #[test]
    fn unusual_layouts_are_refused() {
        let flow = "matches:\n  [{trigger: a, replace: x}]\n";
        assert!(update_phrase(flow, 0, &pe(&["b"], None, None)).is_err());
        assert!(delete_phrase(flow, 0).is_err());
        assert!(add_phrase(flow, &np(&["b"], None, "y")).is_err());
        let flow_entries = "matches:\n  - {trigger: a, replace: x}\n";
        assert!(update_phrase(flow_entries, 0, &pe(&["b"], None, None)).is_err());
        let split = "matches:\n  -\n    trigger: a\n    replace: x\n";
        assert!(update_phrase(split, 0, &pe(&["b"], None, None)).is_err());
        assert!(update_phrase("matches: [unclosed", 0, &pe(&["b"], None, None)).is_err());
        let flow_key = "matches: [{trigger: a, replace: x}]\n";
        assert!(add_phrase(flow_key, &np(&["b"], None, "y")).is_err());
        // Refusal leaves a no-op alone too.
        assert!(update_phrase(flow, 0, &pe(&["a"], None, None)).is_err());
    }

    #[test]
    fn list_at_key_indent_is_understood() {
        let text = "matches:\n  - triggers:\n    - \":a\"\n    - \":b\"\n    replace: x\n";
        let t = update_phrase(text, 0, &pe(&[":a", ":b"], None, Some("y"))).unwrap();
        assert_eq!(t, text.replace("replace: x", "replace: \"y\""));
    }

    #[test]
    fn mixed_line_endings_untouched_lines_keep_theirs() {
        let text =
            "matches:\r\n  - trigger: a\n    replace: x\r\n  - trigger: b\r\n    replace: y\n";
        let t = update_phrase(text, 1, &pe(&["bb"], None, None)).unwrap();
        assert_eq!(
            t,
            "matches:\r\n  - trigger: a\n    replace: x\r\n  - trigger: \"bb\"\r\n    replace: y\n"
        );
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        assert_eq!(content_hash(b""), "cbf29ce484222325");
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
        assert_eq!(content_hash(b"matches:\n"), content_hash(b"matches:\n"));
    }

    fn temp_config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xpando-edit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("match/sub")).unwrap();
        dir
    }

    #[test]
    fn save_file_refuses_changed_file_and_writes_atomically() {
        let dir = temp_config("save");
        let path = dir.join("match/base.yml");
        let orig = "matches:\n  - trigger: a\n    replace: x\n";
        fs::write(&path, orig).unwrap();
        let hash = content_hash(orig.as_bytes());

        // Changed on disk since load: refused, nothing written, no leftovers.
        fs::write(&path, format!("{orig}# edited elsewhere\n")).unwrap();
        let err = save_file(&path, "new", &hash).unwrap_err();
        assert!(matches!(err, SaveError::Conflict));
        assert!(err.to_string().starts_with(CONFLICT_PREFIX));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{orig}# edited elsewhere\n")
        );
        assert!(!dir.join("match/base.yml.bak").exists());

        // First good save keeps a backup of the original; later saves keep that backup.
        fs::write(&path, orig).unwrap();
        save_file(&path, "first", &hash).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        assert_eq!(
            fs::read_to_string(dir.join("match/base.yml.bak")).unwrap(),
            orig
        );
        save_file(&path, "second", &content_hash(b"first")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(
            fs::read_to_string(dir.join("match/base.yml.bak")).unwrap(),
            orig
        );

        let mut names: Vec<_> = fs::read_dir(dir.join("match"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["base.yml", "base.yml.bak", "sub"]);
        // espanso only loads .yml/.yaml, so the backup is not discovered.
        assert_eq!(crate::discover_match_files(&dir).unwrap().len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn edit_file_end_to_end() {
        let dir = temp_config("e2e");
        let orig = "matches:\n  # note\n  - trigger: a\n    replace: x\n";
        fs::write(dir.join("match/sub/f.yml"), orig).unwrap();
        let folder = crate::load_folders(&dir).unwrap().remove(0);
        assert_eq!(folder.file, "sub/f.yml");
        assert_eq!(folder.hash, content_hash(orig.as_bytes()));

        let f = edit_file(&dir, "sub/f.yml", &folder.hash, |t| {
            update_phrase(t, 0, &pe(&["a"], None, Some("y")))
        })
        .unwrap();
        assert_eq!(f.phrases[0].content, "y");
        assert_eq!(f.phrases[0].comment.as_deref(), Some("note"));
        assert_ne!(f.hash, folder.hash);

        // The stale hash is now rejected, even for an edit that would be fine.
        let err = edit_file(&dir, "sub/f.yml", &folder.hash, |t| delete_phrase(t, 0)).unwrap_err();
        assert!(err.starts_with(CONFLICT_PREFIX));
        // An edit that fails writes nothing.
        let before = fs::read_to_string(dir.join("match/sub/f.yml")).unwrap();
        assert!(edit_file(&dir, "sub/f.yml", &f.hash, |t| delete_phrase(t, 9)).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("match/sub/f.yml")).unwrap(),
            before
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_rejects_escapes() {
        let dir = temp_config("resolve");
        fs::write(dir.join("match/ok.yml"), "").unwrap();
        fs::write(dir.join("secret.yml"), "").unwrap();
        assert!(resolve_match_file(&dir, "ok.yml").is_ok());
        for bad in [
            "../secret.yml",
            "sub/../../secret.yml",
            "/ok.yml",
            "C:/x.yml",
            "..\\secret.yml",
            "ok.txt",
            "",
            "sub",
            "missing.yml",
            "./ok.yml",
        ] {
            assert!(resolve_match_file(&dir, bad).is_err(), "{bad}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    fn opts(f: impl FnOnce(&mut PhraseOptions)) -> Option<PhraseOptions> {
        let mut o = PhraseOptions::default();
        f(&mut o);
        Some(o)
    }

    fn with_opts(text: &str, index: usize, o: Option<PhraseOptions>) -> Result<String, String> {
        let mut e = same(text, index);
        e.options = o;
        update_phrase(text, index, &e)
    }

    /// Read-only dry run over real files: `XPANDO_DRYRUN_DIR=<config dir> cargo test -- --ignored`.
    /// Nothing is written; every entry is "edited" to its current values in memory.
    #[test]
    #[ignore]
    fn dry_run_noop_on_real_files() {
        let Ok(dir) = std::env::var("XPANDO_DRYRUN_DIR") else {
            return;
        };
        let files = crate::discover_match_files(Path::new(&dir)).unwrap();
        let mut entries = 0;
        for f in files {
            let text = fs::read_to_string(&f).unwrap();
            for p in parse_match_text(&text).0 {
                let mut e = same(&text, p.index);
                e.options = Some(p.options.clone());
                match update_phrase(&text, p.index, &e) {
                    Ok(out) => assert_eq!(out, text, "{} #{}", f.display(), p.index),
                    Err(m) => println!("refused {} #{}: {m}", f.display(), p.index),
                }
                // Set an option and clear it again: the text must come back byte for byte.
                if p.options.word.is_none() && p.options.search_terms.is_empty() {
                    let mut on = p.options.clone();
                    on.word = Some(true);
                    on.search_terms = vec!["t".into()];
                    if let Ok(with) = with_opts(&text, p.index, Some(on)) {
                        assert_eq!(
                            with_opts(&with, p.index, Some(p.options.clone())).unwrap(),
                            text,
                            "{} #{}",
                            f.display(),
                            p.index
                        );
                    }
                }
                entries += 1;
            }
        }
        println!("checked {entries} entries");
    }

    #[test]
    fn options_are_parsed() {
        let t = "matches:\n  - trigger: a\n    replace: x\n    word: true\n    propagate_case: true\n    uppercase_style: capitalize\n    force_mode: keys\n    search_terms:\n      - one\n      - two\n";
        let o = parse_match_text(t).0[0].options.clone();
        assert_eq!(o.word, Some(true));
        assert_eq!(o.left_word, None);
        assert_eq!(o.uppercase_style.as_deref(), Some("capitalize"));
        assert_eq!(o.force_mode.as_deref(), Some("keys"));
        assert_eq!(o.search_terms, ["one", "two"]);
    }

    #[test]
    fn options_add_change_clear_keep_other_lines() {
        let base = "matches:\n  # note\n  - trigger: \":a\"\n    replace: x\n    vars: []\n  - trigger: \":b\"\n    replace: y\n";
        let full = |word: bool| {
            opts(move |o| {
                o.word = Some(word);
                o.propagate_case = Some(true);
                o.uppercase_style = Some("capitalize_words".into());
                o.force_mode = Some("clipboard".into());
                o.search_terms = vec!["hi".into(), "yo".into()];
            })
        };
        let t = with_opts(base, 0, full(true)).unwrap();
        assert_eq!(
            t,
            "matches:\n  # note\n  - trigger: \":a\"\n    replace: x\n    vars: []\n    word: true\n    propagate_case: true\n    uppercase_style: capitalize_words\n    force_mode: clipboard\n    search_terms:\n      - \"hi\"\n      - \"yo\"\n  - trigger: \":b\"\n    replace: y\n"
        );
        let t2 = with_opts(&t, 0, full(false)).unwrap();
        assert_eq!(t2, t.replace("word: true", "word: false"));
        assert_eq!(with_opts(&t, 0, None).unwrap(), t);
        assert_eq!(with_opts(&t, 0, opts(|_| {})).unwrap(), base);
    }

    #[test]
    fn options_on_last_entry_without_final_newline_and_crlf() {
        let t = "matches:\r\n  - trigger: a\r\n    replace: x";
        let o = with_opts(t, 0, opts(|o| o.left_word = Some(true))).unwrap();
        assert_eq!(
            o,
            "matches:\r\n  - trigger: a\r\n    replace: x\r\n    left_word: true"
        );
        assert_eq!(with_opts(&o, 0, opts(|_| {})).unwrap(), t);
    }

    #[test]
    fn options_flow_list_and_first_key_cases() {
        let t =
            "matches:\n  - word: true\n    trigger: a\n    replace: x\n    search_terms: [p, q]\n";
        let c = with_opts(t, 0, opts(|o| o.word = Some(true))).unwrap();
        assert_eq!(c, t.replace("    search_terms: [p, q]\n", ""));
        let c2 = with_opts(
            t,
            0,
            opts(|o| {
                o.word = Some(true);
                o.search_terms = vec!["z".into()];
            }),
        )
        .unwrap();
        assert_eq!(
            c2,
            "matches:\n  - word: true\n    trigger: a\n    replace: x\n    search_terms:\n      - \"z\"\n"
        );
    }

    #[test]
    fn options_validation_and_new_phrase() {
        let t = "matches:\n  - trigger: a\n    replace: x\n";
        assert!(with_opts(t, 0, opts(|o| o.force_mode = Some("nope".into()))).is_err());
        assert!(with_opts(
            t,
            0,
            opts(|o| o.uppercase_style = Some("capitalize".into()))
        )
        .is_err());
        assert!(with_opts(
            t,
            0,
            opts(|o| o.search_terms = vec!["a".into(), "a".into()])
        )
        .is_err());
        assert!(with_opts(t, 0, opts(|o| o.search_terms = vec![" ".into()])).is_err());
        let mut n = np(&[":n"], None, "body");
        n.comment = Some("About n\nsecond".into());
        n.options.word = Some(true);
        n.options.search_terms = vec!["s".into()];
        let out = add_phrase(t, &n).unwrap();
        assert!(out.ends_with(
            "  # About n\n  # second\n  - trigger: \":n\"\n    replace: \"body\"\n    word: true\n    search_terms:\n      - \"s\"\n"
        ));
        let odd = "matches:\n  - trigger: a\n    replace: x\n    force_mode: weird\n";
        let mut e = same(odd, 0);
        e.label = Some("L".into());
        assert!(update_phrase(odd, 0, &e).is_ok());
    }
}
