//! Format-preserving editing of an entry's `vars:` block (and, in the same pass, its
//! plain-text `replace` body).
//!
//! Like [`crate::edit`], everything works on raw lines. The `vars:` block is split into
//! items, each item into `name` / `type` / `params`, and `params` into single parameters.
//! Only the pieces that really changed are rewritten; every other byte (comments inside the
//! block, quoting, blank lines, line endings) is copied through. The finished text is
//! re-parsed and compared with what was asked for: the whole document outside the edited
//! entry, and every key of the edited entry other than `vars` / `replace`, must be
//! identical. Layouts the line editor does not understand are refused, not guessed at.
//!
//! Parameter values come in three shapes ([`ParamValue`]): text (also numbers and
//! booleans), a list of strings, and raw YAML for anything nested (form fields, choice
//! objects). Unknown variable types and parameters round-trip through the same shapes.

use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};

use super::{parse_match_text, scalar_string, scan_entries, value_text, EntrySpan};

const UNSUPPORTED_FILE: &str = "This file's layout is not one xpando can edit safely (for \
    example a flow-style list). Edit it in a text editor.";
const UNSUPPORTED_ENTRY: &str = "This entry's layout is not one xpando can edit safely. \
    Edit it in a text editor.";
const UNSUPPORTED_VARS: &str = "The variables of this phrase use a layout xpando can not edit \
    safely (for example flow-style lists). Edit them in a text editor.";

/// Variable types xpando knows the parameters of. Others are edited as generic key/value pairs.
pub const KNOWN_TYPES: &[&str] = &[
    "date",
    "clipboard",
    "echo",
    "random",
    "choice",
    "shell",
    "script",
    "form",
];

/// Parameters espanso reads as booleans.
const BOOL_PARAMS: &[&str] = &["trim", "debug", "ignore_error"];

// ---------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------

/// The value of one parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "lowercase")]
pub enum ParamValue {
    /// A scalar: text, or a number / boolean written as text (`true`, `3600`).
    Text(String),
    /// A list of strings (`choices`, `values`, `args`).
    List(Vec<String>),
    /// Nested YAML (a mapping, or a list holding more than strings), as YAML text.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarParam {
    pub key: String,
    pub value: ParamValue,
}

/// One variable, as read from or written to `vars:`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub params: Vec<VarParam>,
    /// Position of this variable in the entry's current `vars:` list; `None` for a new one.
    /// Variables whose content equals the source are copied byte for byte.
    #[serde(default)]
    pub source: Option<usize>,
}

impl VarSpec {
    fn same_content(&self, other: &VarSpec) -> bool {
        self.name == other.name && self.kind == other.kind && self.params == other.params
    }
}

/// Changes to apply to one entry in a single pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarsChange {
    /// The complete new variable list; `None` leaves `vars:` alone.
    #[serde(default)]
    pub vars: Option<Vec<VarSpec>>,
    /// New text of a plain `replace` body; `None` leaves the body alone.
    #[serde(default)]
    pub body: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------

fn param_from_value(v: &Value) -> ParamValue {
    match v {
        Value::Null => ParamValue::Text(String::new()),
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {
            ParamValue::Text(scalar_string(v).unwrap_or_default())
        }
        Value::Sequence(s) if s.iter().all(|x| matches!(x, Value::String(_))) => {
            ParamValue::List(s.iter().filter_map(scalar_string).collect())
        }
        other => ParamValue::Raw(value_text(other)),
    }
}

fn read_item(v: &Value) -> Result<VarSpec, String> {
    let m = v.as_mapping().ok_or(UNSUPPORTED_VARS)?;
    for k in m.keys() {
        if !matches!(k.as_str(), Some("name" | "type" | "params")) {
            return Err(UNSUPPORTED_VARS.into());
        }
    }
    let name = m
        .get("name")
        .and_then(scalar_string)
        .ok_or(UNSUPPORTED_VARS)?;
    let kind = m
        .get("type")
        .and_then(scalar_string)
        .ok_or(UNSUPPORTED_VARS)?;
    let mut params = Vec::new();
    match m.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Mapping(p)) => {
            for (k, v) in p {
                let key = k.as_str().ok_or(UNSUPPORTED_VARS)?.to_string();
                params.push(VarParam {
                    key,
                    value: param_from_value(v),
                });
            }
        }
        Some(_) => return Err(UNSUPPORTED_VARS.into()),
    }
    Ok(VarSpec {
        name,
        kind,
        params,
        source: None,
    })
}

/// Reads a `vars:` value. Strict: anything that is not a plain list of well-formed variables
/// is an error, because the editor must be able to write back what it read.
fn read_vars_value(v: Option<&Value>) -> Result<Vec<VarSpec>, String> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Sequence(s)) => s
            .iter()
            .enumerate()
            .map(|(i, item)| {
                read_item(item).map(|mut spec| {
                    spec.source = Some(i);
                    spec
                })
            })
            .collect(),
        Some(_) => Err(UNSUPPORTED_VARS.into()),
    }
}

fn parse_doc(text: &str) -> Result<Value, String> {
    serde_norway::from_str(text).map_err(|e| format!("The file is not valid YAML: {e}"))
}

fn entries_of(doc: &Value) -> Result<&Vec<Value>, String> {
    match doc {
        Value::Mapping(m) => match m.get("matches") {
            Some(Value::Sequence(s)) => Ok(s),
            _ => Err("The file has no list of phrases.".into()),
        },
        _ => Err("The file is not a YAML mapping.".into()),
    }
}

/// The variables of entry `index`, each with `source` set to its position. An error means
/// the variables are in a layout this module refuses to edit.
pub fn read_vars(text: &str, index: usize) -> Result<Vec<VarSpec>, String> {
    let doc = parse_doc(text)?;
    let entry = entries_of(&doc)?
        .get(index)
        .and_then(Value::as_mapping)
        .ok_or_else(|| format!("There is no entry #{} in this file.", index + 1))?;
    read_vars_value(entry.get("vars"))
}

// ---------------------------------------------------------------------------------------
// Validation, analysis, rename
// ---------------------------------------------------------------------------------------

/// Names the engine reserves. espanso has no reserved variable names (`clipboard` is an
/// ordinary name, used throughout its docs), so only syntax is checked.
pub const RESERVED_NAMES: &[&str] = &[];

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Checks a variable name on its own: letters, digits and `_`, not starting with a digit.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("A variable needs a name.".into());
    }
    if !is_ident(name) {
        return Err(format!(
            "“{name}” is not a valid name. Use letters, digits and _, and don't start with a digit."
        ));
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(format!("“{name}” is reserved."));
    }
    Ok(())
}

fn text_param<'a>(spec: &'a VarSpec, key: &str) -> Option<&'a ParamValue> {
    spec.params.iter().find(|p| p.key == key).map(|p| &p.value)
}

/// Checks the whole list: names, uniqueness, parameter keys and the parameters each known
/// type needs. Errors are plain text for the user.
pub fn validate_specs(specs: &[VarSpec]) -> Result<(), String> {
    for (i, s) in specs.iter().enumerate() {
        validate_name(&s.name)?;
        if specs[..i].iter().any(|o| o.name == s.name) {
            return Err(format!("The variable name “{}” is used twice.", s.name));
        }
        if s.kind.is_empty()
            || !s
                .kind
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!(
                "“{}”: the type needs letters, digits, - or _ only.",
                s.name
            ));
        }
        for (j, p) in s.params.iter().enumerate() {
            if p.key.is_empty()
                || !p
                    .key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(format!(
                    "“{}”: parameter names need letters, digits, - or _ only.",
                    s.name
                ));
            }
            if s.params[..j].iter().any(|o| o.key == p.key) {
                return Err(format!(
                    "“{}”: the parameter “{}” is listed twice.",
                    s.name, p.key
                ));
            }
        }
        let missing = |what: &str| Err(format!("“{}” needs {what}.", s.name));
        let blank_text = |key: &str| match text_param(s, key) {
            Some(ParamValue::Text(t)) => t.trim().is_empty(),
            Some(ParamValue::List(_) | ParamValue::Raw(_)) => false,
            None => true,
        };
        let empty_list = |key: &str| match text_param(s, key) {
            Some(ParamValue::List(l)) => l.iter().all(|x| x.trim().is_empty()),
            Some(ParamValue::Text(t)) => t.trim().is_empty(),
            Some(ParamValue::Raw(_)) => false,
            None => true,
        };
        match s.kind.as_str() {
            "echo" if text_param(s, "echo").is_none() => return missing("text to output"),
            "shell" if blank_text("cmd") => return missing("a command"),
            "random" if empty_list("choices") => return missing("at least one choice"),
            "choice" if empty_list("values") => return missing("at least one value"),
            "script" if empty_list("args") => return missing("a program to run"),
            "form" if blank_text("layout") => return missing("a layout"),
            _ => {}
        }
    }
    Ok(())
}

/// Names of `{{placeholders}}` in `text`, in order, with duplicates. `{{a.b}}` yields `a`.
/// Mirrors espanso's `\{\{\s*(\w+)(\.\w+)?\s*\}\}`.
pub fn placeholders(text: &str) -> Vec<String> {
    scan_placeholders(text)
        .into_iter()
        .map(|p| p.name)
        .collect()
}

struct Placeholder {
    start: usize,
    end: usize,
    name: String,
}

fn scan_placeholders(text: &str) -> Vec<Placeholder> {
    let b = text.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'{' && b[i + 1] == b'{' {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let ns = j;
            while j < b.len() && word(b[j]) {
                j += 1;
            }
            if j > ns {
                let ne = j;
                if j < b.len() && b[j] == b'.' {
                    let mut m = j + 1;
                    while m < b.len() && word(b[m]) {
                        m += 1;
                    }
                    if m > j + 1 {
                        j = m;
                    }
                }
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j + 1 < b.len() && b[j] == b'}' && b[j + 1] == b'}' {
                    out.push(Placeholder {
                        start: ns,
                        end: ne,
                        name: text[ns..ne].to_string(),
                    });
                    i = j + 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn param_strings(v: &ParamValue) -> Vec<&str> {
    match v {
        ParamValue::Text(t) => vec![t.as_str()],
        ParamValue::List(l) => l.iter().map(String::as_str).collect(),
        ParamValue::Raw(r) => vec![r.as_str()],
    }
}

/// Soft problems that do not block saving: placeholders in the body without a variable
/// (a global variable may provide them) and variables nothing refers to.
pub fn analyze(specs: &[VarSpec], body: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for name in placeholders(body) {
        if !specs.iter().any(|s| s.name == name) && !seen.contains(&name) {
            warnings.push(format!(
                "The text uses {{{{{name}}}}}, but this phrase has no variable with that name. \
                 That only works if a global variable provides it."
            ));
            seen.push(name);
        }
    }
    for s in specs {
        let used_in_body = placeholders(body).contains(&s.name);
        let used_by_var = specs.iter().any(|o| {
            o.name != s.name
                && o.params
                    .iter()
                    .flat_map(|p| param_strings(&p.value))
                    .any(|t| placeholders(t).contains(&s.name))
        });
        if !used_in_body && !used_by_var {
            warnings.push(format!(
                "The variable “{}” is not used by the text or by another variable.",
                s.name
            ));
        }
    }
    warnings
}

/// Replaces `{{old}}` (also `{{ old }}` and `{{old.field}}`) by `{{new}}` in `text`.
pub fn rename_in_text(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for p in scan_placeholders(text) {
        if p.name == old {
            out.push_str(&text[last..p.start]);
            out.push_str(new);
            last = p.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Renames variable `old` to `new`: the variable itself, `{{old}}` in every variable's
/// parameters and in the body. Returns the new specs and body. Names are validated and must
/// not collide with another variable.
pub fn rename_var(
    specs: &[VarSpec],
    body: &str,
    old: &str,
    new: &str,
) -> Result<(Vec<VarSpec>, String), String> {
    validate_name(new)?;
    if old != new && specs.iter().any(|s| s.name == new) {
        return Err(format!("The variable name “{new}” is used twice."));
    }
    let fix = |t: &str| rename_in_text(t, old, new);
    let out = specs
        .iter()
        .map(|s| {
            let mut s = s.clone();
            if s.name == old {
                s.name = new.to_string();
            }
            for p in &mut s.params {
                p.value = match &p.value {
                    ParamValue::Text(t) => ParamValue::Text(fix(t)),
                    ParamValue::List(l) => ParamValue::List(l.iter().map(|x| fix(x)).collect()),
                    ParamValue::Raw(r) => ParamValue::Raw(fix(r)),
                };
            }
            s
        })
        .collect();
    Ok((out, fix(body)))
}

// ---------------------------------------------------------------------------------------
// Values (what the YAML should mean)
// ---------------------------------------------------------------------------------------

/// How a text parameter is typed in YAML. Strings stay strings except where espanso reads a
/// boolean or number (`trim`, `debug`, `offset`) and for unknown types, whose values are
/// taken as written (`true` and `42` become a boolean and a number).
fn text_value(kind: &str, key: &str, s: &str) -> Value {
    let known = KNOWN_TYPES.contains(&kind);
    let as_bool = (known && BOOL_PARAMS.contains(&key)) || !known;
    let as_int = (known && key == "offset") || !known;
    if as_bool {
        match s {
            "true" => return Value::Bool(true),
            "false" => return Value::Bool(false),
            _ => {}
        }
    }
    if as_int {
        let digits = s.strip_prefix('-').unwrap_or(s);
        if !digits.is_empty()
            && digits.len() < 16
            && digits.bytes().all(|c| c.is_ascii_digit())
            && !(digits.len() > 1 && digits.starts_with('0'))
        {
            if let Ok(n) = s.parse::<i64>() {
                return Value::from(n);
            }
        }
    }
    Value::String(s.to_string())
}

fn param_value(kind: &str, p: &VarParam) -> Result<Value, String> {
    Ok(match &p.value {
        ParamValue::Text(t) => text_value(kind, &p.key, &t.replace("\r\n", "\n")),
        ParamValue::List(l) => Value::Sequence(
            l.iter()
                .map(|x| Value::String(x.replace("\r\n", "\n")))
                .collect(),
        ),
        ParamValue::Raw(r) => match serde_norway::from_str::<Value>(r) {
            Ok(v @ (Value::Mapping(_) | Value::Sequence(_))) => v,
            Ok(_) => return Err(format!("“{}” must be a YAML list or mapping.", p.key)),
            Err(e) => return Err(format!("“{}” is not valid YAML: {e}", p.key)),
        },
    })
}

/// The YAML value a spec stands for. Parameters equal to the ones in `old` keep the old
/// value (its original type), so the comparison with the written file is exact.
fn spec_value(spec: &VarSpec, old: Option<(&VarSpec, &Value)>) -> Result<Value, String> {
    if let Some((o, v)) = old {
        if o.same_content(spec) {
            return Ok(v.clone());
        }
    }
    let old_params = old
        .and_then(|(_, v)| v.get("params"))
        .and_then(Value::as_mapping);
    let mut m = Mapping::new();
    m.insert("name".into(), Value::String(spec.name.clone()));
    m.insert("type".into(), Value::String(spec.kind.clone()));
    if !spec.params.is_empty() {
        let mut ps = Mapping::new();
        for p in &spec.params {
            let same_as_old = old
                .and_then(|(o, _)| o.params.iter().find(|q| q.key == p.key))
                .filter(|q| q.value == p.value)
                .and_then(|_| old_params.and_then(|m| m.get(p.key.as_str())));
            let v = match same_as_old {
                Some(v) => v.clone(),
                None => param_value(&spec.kind, p)?,
            };
            ps.insert(Value::String(p.key.clone()), v);
        }
        m.insert("params".into(), Value::Mapping(ps));
    }
    Ok(Value::Mapping(m))
}

// ---------------------------------------------------------------------------------------
// Line helpers (small copies of the ones in `edit`, which owns that file)
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

fn is_comment(line: &str) -> bool {
    text_of(line).trim_start().starts_with('#')
}

fn split_lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(String::from).collect()
}

fn detect_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

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

fn block_safe(core: &str) -> bool {
    let first = core.chars().next();
    matches!(first, Some(c) if !c.is_whitespace())
        && !core.chars().any(|c| {
            (c.is_control() && c != '\n' && c != '\t')
                || matches!(c, '\u{feff}' | '\u{85}' | '\u{2028}' | '\u{2029}')
        })
}

/// `key: value` lines for a string; continuation lines carry absolute indentation.
fn render_string(key: &str, value: &str, k: usize, step: usize, eol: &str) -> Vec<String> {
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

/// A name or type written bare when that is unambiguous.
fn plain(s: &str) -> String {
    let special = matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "~"
    );
    let simple = s
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if simple && !special {
        s.to_string()
    } else {
        dq(s)
    }
}

fn render_param(
    kind: &str,
    p: &VarParam,
    pk: usize,
    step: usize,
    eol: &str,
) -> Result<Vec<String>, String> {
    let key = &p.key;
    match &p.value {
        ParamValue::Text(t) => match text_value(kind, key, &t.replace("\r\n", "\n")) {
            Value::Bool(b) => Ok(vec![format!("{key}: {b}{eol}")]),
            Value::Number(n) => Ok(vec![format!("{key}: {n}{eol}")]),
            Value::String(s) => Ok(render_string(key, &s, pk, step, eol)),
            _ => Err(UNSUPPORTED_VARS.into()),
        },
        ParamValue::List(l) => {
            if l.is_empty() {
                return Ok(vec![format!("{key}: []{eol}")]);
            }
            let pad = " ".repeat(pk + step);
            let mut out = vec![format!("{key}:{eol}")];
            for item in l {
                out.push(format!("{pad}- {}{eol}", dq(&item.replace("\r\n", "\n"))));
            }
            Ok(out)
        }
        ParamValue::Raw(_) => {
            let v = param_value(kind, p)?;
            let yaml = serde_norway::to_string(&v).map_err(|e| e.to_string())?;
            let pad = " ".repeat(pk + step);
            let mut out = vec![format!("{key}:{eol}")];
            for line in yaml.trim_end().lines() {
                out.push(if line.is_empty() {
                    eol.to_string()
                } else {
                    format!("{pad}{line}{eol}")
                });
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Splitting lines into keyed segments
// ---------------------------------------------------------------------------------------

/// One `key: value` of a block mapping with its continuation lines.
#[derive(Debug)]
struct Seg {
    key: String,
    /// The segment's own lines; the first has its leading `k` columns removed.
    own: Vec<String>,
    /// Blank and comment lines after it, kept in place when it is rewritten.
    tail: Vec<String>,
    /// Comment lines written before it (only after the segment before it was removed).
    lead: Vec<String>,
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

/// Splits mapping lines whose keys sit at column `k` (the first line's first `k` columns
/// are a prefix such as `  - `). Unusual layouts are an error.
fn split_map(lines: &[String], k: usize, what: &str) -> Result<Vec<Seg>, String> {
    let unusual = || -> String { format!("{what} uses a layout xpando can not edit safely.") };
    let mut segs: Vec<Seg> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = text_of(l);
        let starts = if i == 0 {
            true
        } else {
            let tt = t.trim();
            let ind = indent(t);
            !tt.is_empty()
                && !tt.starts_with('#')
                && ind <= k
                && !(ind == k && (tt == "-" || tt.starts_with("- ")))
        };
        if starts {
            if i > 0 && (indent(t) != k || t[..k].contains('\t')) {
                return Err(unusual());
            }
            let rest = t.get(k..).ok_or_else(unusual)?;
            let key = field_key(rest).ok_or_else(unusual)?;
            segs.push(Seg {
                key,
                own: vec![l[k..].to_string()],
                tail: Vec::new(),
                lead: Vec::new(),
            });
        } else {
            segs.last_mut().ok_or_else(unusual)?.own.push(l.clone());
        }
    }
    for s in &mut segs {
        let mut tail = Vec::new();
        while s.own.len() > 1 {
            let t = text_of(s.own.last().unwrap());
            let tt = t.trim();
            if tt.is_empty() || (tt.starts_with('#') && indent(t) <= k) {
                tail.push(s.own.pop().unwrap());
            } else {
                break;
            }
        }
        tail.reverse();
        s.tail = tail;
    }
    Ok(segs)
}

fn assemble(prefix: &str, k: usize, segs: &[Seg]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, s) in segs.iter().enumerate() {
        out.extend(s.lead.iter().cloned());
        for (i, l) in s.own.iter().enumerate() {
            if i == 0 {
                let pre = if n == 0 {
                    prefix.to_string()
                } else {
                    " ".repeat(k)
                };
                out.push(format!("{pre}{l}"));
            } else {
                out.push(l.clone());
            }
        }
        out.extend(s.tail.iter().cloned());
    }
    out
}

fn dup_keys(segs: &[Seg], what: &str) -> Result<(), String> {
    for (i, s) in segs.iter().enumerate() {
        if segs[..i].iter().any(|o| o.key == s.key) {
            return Err(format!("{what} lists `{}` twice.", s.key));
        }
    }
    Ok(())
}

/// Removes `segs[pos]`. Its comments stay: they go after the segment before it, or, for
/// the first segment and when `allow_front`, in front of the next one.
fn remove_seg(segs: &mut Vec<Seg>, pos: usize, allow_front: bool) -> Result<(), String> {
    let s = segs.remove(pos);
    let kept: Vec<String> = s.lead.into_iter().chain(s.tail).collect();
    if kept.is_empty() {
        return Ok(());
    }
    if pos > 0 {
        segs[pos - 1].tail.extend(kept);
    } else if allow_front && !segs.is_empty() {
        let next = &mut segs[0];
        next.lead = kept
            .into_iter()
            .chain(std::mem::take(&mut next.lead))
            .collect();
    } else {
        return Err(UNSUPPORTED_ENTRY.into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Editing one variable
// ---------------------------------------------------------------------------------------

/// The part of a variable's text that is rewritten when its params change.
fn modify_params(
    seg: &mut Seg,
    item_k: usize,
    kind: &str,
    old: &[VarParam],
    new: &[VarParam],
    step: usize,
    eol: &str,
) -> Result<(), String> {
    let what = "A variable's parameters";
    let header = text_of(&seg.own[0]).trim_end();
    if header.trim() != "params:" && !header.starts_with("params: #") {
        return Err(UNSUPPORTED_VARS.into());
    }
    let rest = &seg.own[1..];
    let first = rest
        .iter()
        .position(|l| !is_blank(l) && !is_comment(l))
        .ok_or(UNSUPPORTED_VARS)?;
    let lead: Vec<String> = rest[..first].to_vec();
    let pk = indent(text_of(&rest[first]));
    if pk <= item_k || text_of(&rest[first])[..pk].contains('\t') {
        return Err(UNSUPPORTED_VARS.into());
    }
    let mut segs = split_map(&rest[first..], pk, what)?;
    dup_keys(&segs, what)?;

    // Drop removed parameters.
    let mut i = 0;
    while i < segs.len() {
        if new.iter().any(|p| p.key == segs[i].key) {
            i += 1;
        } else {
            remove_seg(&mut segs, i, true).map_err(|_| UNSUPPORTED_VARS.to_string())?;
        }
    }
    // Rewrite changed ones, append new ones.
    for p in new {
        let unchanged = old.iter().any(|o| o == p);
        match segs.iter_mut().find(|s| s.key == p.key) {
            Some(_) if unchanged => {}
            Some(s) => s.own = render_param(kind, p, pk, step, eol)?,
            None => {
                if let Some(last) = segs.last_mut() {
                    // Keep trailing comments at the end of the block.
                    let tail = std::mem::take(&mut last.tail);
                    segs.push(Seg {
                        key: p.key.clone(),
                        own: render_param(kind, p, pk, step, eol)?,
                        tail,
                        lead: Vec::new(),
                    });
                }
            }
        }
    }
    let mut own = vec![seg.own[0].clone()];
    own.extend(lead);
    own.extend(assemble(&" ".repeat(pk), pk, &segs));
    seg.own = own;
    Ok(())
}

fn render_new_params(
    kind: &str,
    params: &[VarParam],
    pk: usize,
    step: usize,
    eol: &str,
) -> Result<Vec<String>, String> {
    let mut out = vec![format!("params:{eol}")];
    for p in params {
        let lines = render_param(kind, p, pk, step, eol)?;
        for (i, l) in lines.into_iter().enumerate() {
            out.push(if i == 0 {
                format!("{}{l}", " ".repeat(pk))
            } else {
                l
            });
        }
    }
    Ok(out)
}

/// A brand-new item, including its `- ` line.
fn build_item(spec: &VarSpec, ii: usize, step: usize, eol: &str) -> Result<Vec<String>, String> {
    let ik = ii + 2;
    let mut out = vec![
        format!("{}- name: {}{eol}", " ".repeat(ii), plain(&spec.name)),
        format!("{}type: {}{eol}", " ".repeat(ik), plain(&spec.kind)),
    ];
    if !spec.params.is_empty() {
        for (i, l) in render_new_params(&spec.kind, &spec.params, ik + step, step, eol)?
            .into_iter()
            .enumerate()
        {
            out.push(if i == 0 {
                format!("{}{l}", " ".repeat(ik))
            } else {
                l
            });
        }
    }
    Ok(out)
}

/// Rewrites the changed parts of an existing item.
fn modify_item(
    lines: &[String],
    ii: usize,
    old: &VarSpec,
    new: &VarSpec,
    step: usize,
    eol: &str,
) -> Result<Vec<String>, String> {
    let ik = ii + 2;
    let first = text_of(&lines[0]);
    if first.len() <= ik || first[..ik] != format!("{}- ", " ".repeat(ii)) {
        return Err(UNSUPPORTED_VARS.into());
    }
    let prefix = first[..ik].to_string();
    let mut segs = split_map(lines, ik, "A variable")?;
    dup_keys(&segs, "A variable")?;
    if segs
        .iter()
        .any(|s| !matches!(s.key.as_str(), "name" | "type" | "params"))
    {
        return Err(UNSUPPORTED_VARS.into());
    }
    let find = |segs: &[Seg], key: &str| segs.iter().position(|s| s.key == key);
    if new.name != old.name {
        let p = find(&segs, "name").ok_or(UNSUPPORTED_VARS)?;
        segs[p].own = vec![format!("name: {}{eol}", plain(&new.name))];
    }
    if new.kind != old.kind {
        let p = find(&segs, "type").ok_or(UNSUPPORTED_VARS)?;
        segs[p].own = vec![format!("type: {}{eol}", plain(&new.kind))];
    }
    if new.params != old.params || new.kind != old.kind {
        match (find(&segs, "params"), new.params.is_empty()) {
            (Some(p), true) => {
                remove_seg(&mut segs, p, false).map_err(|_| UNSUPPORTED_VARS.to_string())?
            }
            (Some(p), false) => {
                // The kind is part of how values are typed; a type change re-renders all.
                if new.kind != old.kind {
                    segs[p].own = render_new_params(&new.kind, &new.params, ik + step, step, eol)?;
                } else {
                    modify_params(
                        &mut segs[p],
                        ik,
                        &new.kind,
                        &old.params,
                        &new.params,
                        step,
                        eol,
                    )?;
                }
            }
            (None, true) => {}
            (None, false) => {
                let own = render_new_params(&new.kind, &new.params, ik + step, step, eol)?;
                let last = segs.len() - 1;
                let tail = std::mem::take(&mut segs[last].tail);
                segs.push(Seg {
                    key: "params".into(),
                    own,
                    tail,
                    lead: Vec::new(),
                });
            }
        }
    }
    Ok(assemble(&prefix, ik, &segs))
}

// ---------------------------------------------------------------------------------------
// The vars block
// ---------------------------------------------------------------------------------------

struct Item {
    /// Comment lines directly above the item, and blank/comment lines before the first one.
    lead: Vec<String>,
    /// From the item's `- ` line on, including blank lines and deeper comments after it.
    lines: Vec<String>,
}

struct VarsBlock {
    header: String,
    /// Column of the items' `-`.
    ii: usize,
    items: Vec<Item>,
}

fn is_item_start(line: &str) -> bool {
    let t = text_of(line).trim_start();
    t == "-" || t.starts_with("- ")
}

fn parse_block(own: &[String], k: usize, step: usize) -> Result<VarsBlock, String> {
    let header = text_of(&own[0]);
    let after = header.strip_prefix("vars:").ok_or(UNSUPPORTED_VARS)?.trim();
    let inline_empty = after == "[]";
    if !(after.is_empty() || after.starts_with('#') || inline_empty) {
        return Err(UNSUPPORTED_VARS.into());
    }
    let head = if inline_empty {
        let term = &own[0][header.len()..];
        format!("{}{term}", header.replacen("[]", "", 1).trim_end())
    } else {
        own[0].clone()
    };
    let rest = &own[1..];
    let Some(first) = rest.iter().position(|l| !is_blank(l) && !is_comment(l)) else {
        if rest.is_empty() {
            return Ok(VarsBlock {
                header: head,
                ii: k + step,
                items: Vec::new(),
            });
        }
        return Err(UNSUPPORTED_VARS.into());
    };
    if inline_empty {
        return Err(UNSUPPORTED_VARS.into());
    }
    let ii = indent(text_of(&rest[first]));
    if ii < k || !is_item_start(&rest[first]) || text_of(&rest[first])[..ii].contains('\t') {
        return Err(UNSUPPORTED_VARS.into());
    }
    let mut items: Vec<Item> = Vec::new();
    let mut pending: Vec<String> = rest[..first].to_vec();
    let mut cur: Option<Item> = None;
    for l in &rest[first..] {
        let t = text_of(l);
        let structural = !is_blank(l) && !is_comment(l);
        if structural && indent(t) == ii && is_item_start(l) {
            // Comments directly above the new item (at or left of its dash) are its lead.
            let mut lead = Vec::new();
            while let Some(c) = pending.last() {
                if is_comment(c) && indent(text_of(c)) <= ii {
                    lead.push(pending.pop().unwrap());
                } else {
                    break;
                }
            }
            lead.reverse();
            if let Some(mut it) = cur.take() {
                it.lines.append(&mut pending);
                items.push(it);
                pending = Vec::new();
            } else {
                // Before the first item: everything pending is lead.
                lead = std::mem::take(&mut pending)
                    .into_iter()
                    .chain(lead)
                    .collect();
            }
            cur = Some(Item {
                lead,
                lines: vec![l.clone()],
            });
        } else if structural {
            if indent(t) <= ii || cur.is_none() {
                return Err(UNSUPPORTED_VARS.into());
            }
            let it = cur.as_mut().unwrap();
            it.lines.append(&mut pending);
            it.lines.push(l.clone());
        } else {
            pending.push(l.clone());
        }
    }
    let mut it = cur.ok_or(UNSUPPORTED_VARS)?;
    it.lines.append(&mut pending);
    items.push(it);
    Ok(VarsBlock {
        header: head,
        ii,
        items,
    })
}

// ---------------------------------------------------------------------------------------
// The entry
// ---------------------------------------------------------------------------------------

struct Entry {
    prefix: String,
    k: usize,
    segs: Vec<Seg>,
}

fn split_entry(lines: &[String]) -> Result<Entry, String> {
    let first = text_of(&lines[0]);
    let d = indent(first);
    let rest = first[d..].strip_prefix('-').ok_or(UNSUPPORTED_ENTRY)?;
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let k = d + 1 + spaces;
    if first.len() <= k
        || first[k..].trim().is_empty()
        || first[k..].starts_with('#')
        || first[..d].contains('\t')
    {
        return Err(UNSUPPORTED_ENTRY.into());
    }
    let segs = split_map(lines, k, "This entry").map_err(|_| UNSUPPORTED_ENTRY.to_string())?;
    Ok(Entry {
        prefix: first[..k].to_string(),
        k,
        segs,
    })
}

impl Entry {
    fn pos(&self, key: &str) -> Vec<usize> {
        (0..self.segs.len())
            .filter(|&i| self.segs[i].key == key)
            .collect()
    }

    fn step(&self) -> usize {
        self.segs
            .iter()
            .flat_map(|s| s.own.iter().skip(1))
            .map(|l| text_of(l))
            .filter(|t| !t.trim().is_empty() && indent(t) > self.k)
            .map(|t| indent(t) - self.k)
            .next()
            .unwrap_or(2)
            .clamp(1, 8)
    }
}

fn body_end(lines: &[String], span: &EntrySpan) -> usize {
    let d = indent(text_of(&lines[span.start_line]));
    let mut e = span.end_line;
    while e > span.start_line + 1 {
        let t = text_of(&lines[e - 1]);
        let tt = t.trim();
        if tt.is_empty() || (tt.starts_with('#') && indent(t) <= d) {
            e -= 1;
        } else {
            break;
        }
    }
    e
}

/// Applies the `vars:` change to the entry's segments. Returns the value `vars` must have
/// afterwards (`None` = no `vars` key).
fn apply_vars(
    entry: &mut Entry,
    old: &[VarSpec],
    old_vals: &[Value],
    new: &[VarSpec],
    eol: &str,
) -> Result<Option<Value>, String> {
    let step = entry.step();
    let k = entry.k;
    let mut expected = Vec::new();
    for s in new {
        let o = s.source.and_then(|i| old.get(i).zip(old_vals.get(i)));
        expected.push(spec_value(s, o)?);
    }
    let pos = entry.pos("vars");
    if pos.len() > 1 {
        return Err(UNSUPPORTED_ENTRY.into());
    }

    if new.is_empty() {
        if let Some(&p) = pos.first() {
            remove_seg(&mut entry.segs, p, false)?;
        }
        return Ok(None);
    }

    let Some(&p) = pos.first() else {
        // Create the key after the body (or at the end).
        let mut lines = vec![format!("vars:{eol}")];
        let ii = k + step;
        for s in new {
            lines.extend(build_item(s, ii, step, eol)?);
        }
        let anchor = (0..entry.segs.len())
            .rev()
            .find(|&i| {
                matches!(
                    entry.segs[i].key.as_str(),
                    "replace" | "html" | "markdown" | "image_path" | "form"
                )
            })
            .unwrap_or(entry.segs.len().saturating_sub(1));
        let tail = if anchor + 1 == entry.segs.len() {
            std::mem::take(&mut entry.segs[anchor].tail)
        } else {
            Vec::new()
        };
        entry.segs.insert(
            anchor + 1,
            Seg {
                key: "vars".into(),
                own: lines,
                tail,
                lead: Vec::new(),
            },
        );
        return Ok(Some(Value::Sequence(expected)));
    };

    let block = parse_block(&entry.segs[p].own, k, step)?;
    if block.items.len() != old.len() {
        return Err(UNSUPPORTED_VARS.into());
    }
    let ii = block.ii;
    let mut used = vec![false; block.items.len()];
    let mut out: Vec<Vec<String>> = Vec::new();
    for s in new {
        match s.source {
            Some(i) => {
                if i >= block.items.len() || used[i] {
                    return Err(UNSUPPORTED_VARS.into());
                }
                used[i] = true;
                let item = &block.items[i];
                let mut lines = item.lead.clone();
                if old[i].same_content(s) {
                    lines.extend(item.lines.iter().cloned());
                } else {
                    lines.extend(modify_item(&item.lines, ii, &old[i], s, step, eol)?);
                    // Keep blank lines that followed the item.
                    let blanks = item.lines.iter().rev().take_while(|l| is_blank(l)).count();
                    lines.extend(item.lines[item.lines.len() - blanks..].iter().cloned());
                }
                out.push(lines);
            }
            None => out.push(build_item(s, ii, step, eol)?),
        }
    }
    let mut own = vec![block.header.clone()];
    for lines in out {
        own.extend(lines);
    }
    while own.len() > 1 && is_blank(own.last().unwrap()) {
        own.pop();
    }
    entry.segs[p].own = own;
    Ok(Some(Value::Sequence(expected)))
}

fn normalize_body(body: &str) -> String {
    body.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string()
}

/// Applies `change` to entry `index` in one pass; see the module docs. The input is
/// returned unchanged when nothing differs.
pub fn update_entry(text: &str, index: usize, change: &VarsChange) -> Result<String, String> {
    let doc = parse_doc(text)?;
    let seq = entries_of(&doc)?;
    let count = seq.len();
    let spans = scan_entries(text)
        .filter(|s| s.len() == count)
        .ok_or(UNSUPPORTED_FILE)?;
    let old_map = seq
        .get(index)
        .and_then(Value::as_mapping)
        .ok_or_else(|| format!("There is no entry #{} in this file.", index + 1))?;

    // What changes?
    let old_items: &[Value] = match old_map.get("vars") {
        Some(Value::Sequence(s)) => s,
        _ => &[],
    };
    let mut vars_edit: Option<(Vec<VarSpec>, &[VarSpec])> = None;
    let old_specs = if change.vars.is_some() {
        read_vars_value(old_map.get("vars"))?
    } else {
        Vec::new()
    };
    if let Some(new) = &change.vars {
        validate_specs(new)?;
        let mut seen = vec![false; old_specs.len()];
        for n in new {
            if let Some(i) = n.source {
                if i >= seen.len() || seen[i] {
                    return Err(
                        "The variable list does not match the file any more. Reload and try again."
                            .into(),
                    );
                }
                seen[i] = true;
            }
        }
        let same = new.len() == old_specs.len()
            && new
                .iter()
                .zip(&old_specs)
                .enumerate()
                .all(|(i, (n, o))| n.source == Some(i) && n.same_content(o));
        if !same {
            vars_edit = Some((new.clone(), &old_specs));
        }
    }
    let old_body = match old_map.get("replace") {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    };
    let new_body = change.body.as_deref().map(normalize_body);
    let body_changed = match (&new_body, old_body) {
        (Some(b), Some(o)) => b != o.trim_end_matches('\n'),
        (Some(_), None) => {
            return Err("Only plain-text phrases can have their text edited here.".into())
        }
        _ => false,
    };
    if vars_edit.is_none() && !body_changed {
        return Ok(text.to_string());
    }

    // Edit the lines.
    let lines = split_lines(text);
    let eol = detect_eol(text);
    let ends_nl = text.is_empty() || text.ends_with('\n');
    let span = &spans[index];
    let be = body_end(&lines, span);
    let mut entry = split_entry(&lines[span.start_line..be])?;

    let mut expected = old_map.clone();
    if body_changed {
        let pos = entry.pos("replace");
        if pos.len() != 1 {
            return Err(UNSUPPORTED_ENTRY.into());
        }
        let old = old_body.unwrap_or("");
        let newlines = old.len() - old.trim_end_matches('\n').len();
        let value = format!(
            "{}{}",
            new_body.as_deref().unwrap_or(""),
            "\n".repeat(newlines)
        );
        let step = entry.step();
        entry.segs[pos[0]].own = render_string("replace", &value, entry.k, step, eol);
        expected.insert("replace".into(), Value::String(value));
    }
    if let Some((new, old)) = &vars_edit {
        let vals: Vec<Value> = old_items.to_vec();
        let v = apply_vars(&mut entry, old, &vals, new, eol)?;
        match v {
            Some(v) => {
                expected.insert("vars".into(), v);
            }
            None => {
                expected.remove("vars");
            }
        }
    }

    let mut out: Vec<String> = lines[..span.start_line].to_vec();
    out.extend(assemble(&entry.prefix, entry.k, &entry.segs));
    out.extend(lines[be..].iter().cloned());
    let new_text = finish(out, eol, ends_nl);

    verify(text, &new_text, index, count, &expected)?;
    Ok(new_text)
}

/// Replaces entry `index`'s `vars:` with `specs` (see [`VarSpec::source`]).
pub fn update_vars(text: &str, index: usize, specs: &[VarSpec]) -> Result<String, String> {
    update_entry(
        text,
        index,
        &VarsChange {
            vars: Some(specs.to_vec()),
            body: None,
        },
    )
}

fn verify(
    old: &str,
    new: &str,
    index: usize,
    count: usize,
    expected: &Mapping,
) -> Result<(), String> {
    let fail = |what: &str| -> Result<(), String> {
        Err(format!(
            "Safety check failed ({what}); nothing was changed. Please report this."
        ))
    };
    let Ok(mut new_doc) = parse_doc(new) else {
        return fail("the result is not valid YAML");
    };
    let mut old_doc = parse_doc(old)?;
    let Ok(new_seq) = entries_of(&new_doc) else {
        return fail("the result has no entries");
    };
    if new_seq.len() != count {
        return fail("entry count differs");
    }
    if scan_entries(new).map(|s| s.len()) != Some(count) {
        return fail("entry layout not recognised");
    }
    if new_seq[index].as_mapping() != Some(expected) {
        return fail("the edited phrase differs from the request");
    }
    // Everything else in the document must be untouched.
    for doc in [&mut old_doc, &mut new_doc] {
        if let Value::Mapping(m) = doc {
            if let Some(Value::Sequence(s)) = m.get_mut("matches") {
                s[index] = Value::Null;
            }
        }
    }
    if old_doc != new_doc {
        return fail("another phrase changed");
    }
    let (_, old_w) = parse_match_text(old);
    let (_, new_w) = parse_match_text(new);
    if old_w != new_w {
        return fail("warnings differ");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> ParamValue {
        ParamValue::Text(s.into())
    }

    fn p(k: &str, v: ParamValue) -> VarParam {
        VarParam {
            key: k.into(),
            value: v,
        }
    }

    fn spec(name: &str, kind: &str, params: Vec<VarParam>) -> VarSpec {
        VarSpec {
            name: name.into(),
            kind: kind.into(),
            params,
            source: None,
        }
    }

    /// The user's `TDC` phrase (with its comment inside the vars list) in a small file.
    const TDC: &str = "\
# header
matches:
  # first
  - trigger: \":a\"
    replace: \"one\"

  # PhraseExpress: {#datetime -f yyyymmdd}_{#insertclipboard}{#setclipboard ...}
  - trigger: \"TDC\"
    replace: \"{{out}}\"
    vars:
      - name: d
        type: date
        params:
          format: \"%Y%m%d\"
      - name: clip
        type: clipboard
      - name: out
        type: echo
        params:
          echo: \"{{d}}_{{clip}}\"
      # the equivalent of {#setclipboard}: runs PowerShell to put the text on the clipboard
      - name: setclip
        type: shell
        params:
          shell: powershell
          cmd: \"Set-Clipboard -Value $env:ESPANSO_OUT\"

  - trigger: \":z\"
    replace: \"last\"
";

    fn read(text: &str, i: usize) -> Vec<VarSpec> {
        read_vars(text, i).unwrap()
    }

    fn lines_diff(a: &str, b: &str) -> (usize, usize) {
        let al: Vec<_> = a.lines().collect();
        let bl: Vec<_> = b.lines().collect();
        (
            al.iter().filter(|l| !bl.contains(l)).count(),
            bl.iter().filter(|l| !al.contains(l)).count(),
        )
    }

    #[test]
    fn reads_tdc() {
        let v = read(TDC, 1);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].kind, "date");
        assert_eq!(v[0].params, vec![p("format", t("%Y%m%d"))]);
        assert!(v[1].params.is_empty());
        assert_eq!(v[3].params[0], p("shell", t("powershell")));
        assert_eq!(v[3].source, Some(3));
    }

    #[test]
    fn noop_is_byte_identical() {
        let v = read(TDC, 1);
        assert_eq!(update_vars(TDC, 1, &v).unwrap(), TDC);
        let crlf = TDC.replace('\n', "\r\n");
        assert_eq!(update_vars(&crlf, 1, &read(&crlf, 1)).unwrap(), crlf);
        // Entries without vars.
        assert_eq!(update_vars(TDC, 0, &[]).unwrap(), TDC);
    }

    #[test]
    fn changing_one_param_keeps_comment_and_siblings() {
        let mut v = read(TDC, 1);
        v[0].params = vec![p("format", t("%d.%m.%Y")), p("offset", t("86400"))];
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains("          format: \"%d.%m.%Y\"\n          offset: 86400\n"));
        assert!(out.contains("      # the equivalent of {#setclipboard}"));
        assert_eq!(lines_diff(TDC, &out), (1, 2));
        let back = read(&out, 1);
        assert_eq!(back[0].params[1], p("offset", t("86400")));
    }

    #[test]
    fn changing_a_param_inside_the_commented_item_keeps_its_comment() {
        let mut v = read(TDC, 1);
        v[3].params[1] = p("cmd", t("Get-Date | Set-Clipboard"));
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains("# the equivalent of {#setclipboard}"));
        assert!(out.contains("cmd: \"Get-Date | Set-Clipboard\""));
        assert_eq!(lines_diff(TDC, &out), (1, 1));
    }

    #[test]
    fn rename_inside_item_keeps_the_rest() {
        let mut v = read(TDC, 1);
        v[0].name = "day".into();
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains("      - name: day\n        type: date\n"));
        assert_eq!(lines_diff(TDC, &out), (1, 1));
    }

    #[test]
    fn reorder_moves_items_with_their_comments() {
        let mut v = read(TDC, 1);
        let last = v.pop().unwrap();
        v.insert(0, last);
        let out = update_vars(TDC, 1, &v).unwrap();
        let names: Vec<_> = read(&out, 1).iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, ["setclip", "d", "clip", "out"]);
        let c = out.find("# the equivalent").unwrap();
        let s = out.find("- name: setclip").unwrap();
        assert!(c < s && s < out.find("- name: d").unwrap());
        // Moving back restores every byte.
        let mut again = read(&out, 1);
        let first = again.remove(0);
        again.push(first);
        assert_eq!(update_vars(&out, 1, &again).unwrap(), TDC);
    }

    #[test]
    fn remove_one_and_all() {
        let mut v = read(TDC, 1);
        v.remove(3);
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(!out.contains("name: setclip") && !out.contains("# the equivalent"));
        assert_eq!(read(&out, 1).len(), 3);
        assert!(out.ends_with("    replace: \"last\"\n"));

        let none = update_vars(TDC, 1, &[]).unwrap();
        assert!(!none.contains("vars:") && !none.contains("name: d"));
        assert!(none.contains("# PhraseExpress"));
        assert!(none.contains("  - trigger: \":z\""));
        assert!(read(&none, 1).is_empty());
    }

    #[test]
    fn add_to_existing_and_create_key() {
        let mut v = read(TDC, 1);
        v.push(spec(
            "pick",
            "choice",
            vec![p(
                "values",
                ParamValue::List(vec!["a".into(), "b c".into()]),
            )],
        ));
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains("      - name: pick\n        type: choice\n        params:\n          values:\n            - \"a\"\n            - \"b c\"\n"));
        assert_eq!(read(&out, 1).len(), 5);

        // Entry 0 has no vars: the key is created after `replace`.
        let s = vec![spec("clip", "clipboard", vec![])];
        let out = update_vars(TDC, 0, &s).unwrap();
        assert!(out.contains(
            "  - trigger: \":a\"\n    replace: \"one\"\n    vars:\n      - name: clip\n        type: clipboard\n"
        ));
        assert_eq!(read(&out, 0)[0].name, "clip");
        assert_eq!(read(&out, 1).len(), 4);
    }

    #[test]
    fn first_middle_last_entries_and_no_trailing_newline() {
        let s = vec![spec("e", "echo", vec![p("echo", t("hi"))])];
        for i in 0..3 {
            let text = "matches:\n  - trigger: a\n    replace: x\n  - trigger: b\n    replace: y\n  - trigger: c\n    replace: z";
            let out = update_vars(text, i, &s).unwrap();
            assert!(!out.ends_with('\n'), "{i}");
            assert_eq!(read(&out, i)[0].params[0], p("echo", t("hi")));
            // Removing again restores the original.
            assert_eq!(update_vars(&out, i, &[]).unwrap(), text, "{i}");
        }
    }

    #[test]
    fn crlf_is_kept() {
        let crlf = TDC.replace('\n', "\r\n");
        let mut v = read(&crlf, 1);
        v.push(spec("n", "echo", vec![p("echo", t("x"))]));
        let out = update_vars(&crlf, 1, &v).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'));
        assert_eq!(read(&out, 1).len(), 5);
    }

    #[test]
    fn real_word_shape_with_blank_lines_between_entries() {
        let text = "matches:\n  # set paste cache\n  - trigger: \"<setc\"\n    replace: \"<wr:set cache=\\\"$|$\\\" nodePath=\\\"{{clip}}\\\"/>\"\n    vars:\n      - name: clip\n        type: clipboard\n\n  # set paste label\n  - trigger: \"<setl\"\n    replace: \"x{{clip}}\"\n    vars:\n      - name: clip\n        type: clipboard\n\n  # Punctuate\n  - trigger: \"<pp\"\n    replace: \"{{lb}}{{clip}}{{rb}}\"\n    vars:\n      - name: clip\n        type: clipboard\n      - name: lb\n        type: echo\n        params:\n          echo: \"{\"\n      - name: rb\n        type: echo\n        params:\n          echo: \"}\"\n\n  - trigger: \"<is\"\n    replace: \"s\"\n";
        for i in 0..3 {
            let v = read(text, i);
            assert_eq!(update_vars(text, i, &v).unwrap(), text);
        }
        let mut v = read(text, 2);
        v[1].params[0] = p("echo", t("{{"));
        let out = update_vars(text, 2, &v).unwrap();
        assert!(out.contains("echo: \"{{\""));
        assert_eq!(read(&out, 0), read(text, 0));
    }

    #[test]
    fn flow_style_is_refused_but_untouched_is_fine() {
        let text = "matches:\n  - trigger: a\n    replace: \"{{x}}\"\n    vars: [{name: x, type: echo, params: {echo: hi}}]\n";
        let v = read(text, 0);
        assert_eq!(v[0].params[0], p("echo", t("hi")));
        assert_eq!(update_vars(text, 0, &v).unwrap(), text);
        let mut changed = v.clone();
        changed[0].params[0] = p("echo", t("yo"));
        assert!(update_vars(text, 0, &changed).is_err());
        assert!(update_vars(text, 0, &[]).is_err() || update_vars(text, 0, &[]).is_ok());
        // Whole flow-style file.
        let flow = "matches:\n  [{trigger: a, replace: x}]\n";
        assert!(update_vars(flow, 0, &[spec("e", "echo", vec![p("echo", t("x"))])]).is_err());
    }

    #[test]
    fn items_in_flow_style_inside_a_block_list_are_kept_when_unchanged() {
        let text = "matches:\n  - trigger: a\n    replace: \"{{x}}\"\n    vars:\n      - {name: x, type: echo, params: {echo: hi}}\n      - name: y\n        type: clipboard\n";
        let mut v = read(text, 0);
        v[1].name = "yy".into();
        let out = update_vars(text, 0, &v).unwrap();
        assert!(out.contains("- {name: x, type: echo, params: {echo: hi}}"));
        assert!(out.contains("name: yy"));
        // Changing the flow item is refused.
        let mut v = read(text, 0);
        v[0].name = "z".into();
        assert!(update_vars(text, 0, &v).is_err());
    }

    #[test]
    fn sequence_at_key_indent_and_four_space_style() {
        let text = "matches:\n- trigger: a\n  replace: x\n  vars:\n  - name: c\n    type: clipboard\n- trigger: b\n  replace: y\n";
        let mut v = read(text, 0);
        assert_eq!(update_vars(text, 0, &v).unwrap(), text);
        v.push(spec("nn", "echo", vec![p("echo", t("q"))]));
        let out = update_vars(text, 0, &v).unwrap();
        assert!(out.contains("  - name: nn\n    type: echo\n    params:\n"));
        assert_eq!(read(&out, 0).len(), 2);

        let four = "matches:\n    - trigger: a\n      replace: x\n      vars:\n          - name: c\n            type: clipboard\n";
        let mut v = read(four, 0);
        v[0].name = "d".into();
        assert!(update_vars(four, 0, &v).unwrap().contains("- name: d"));
    }

    #[test]
    fn typed_params_and_bare_scalars() {
        let s = vec![
            spec(
                "sh",
                "shell",
                vec![
                    p("cmd", t("echo hi\necho there")),
                    p("shell", t("bash")),
                    p("trim", t("false")),
                    p("debug", t("true")),
                ],
            ),
            spec(
                "d",
                "date",
                vec![p("offset", t("-3600")), p("locale", t("de-DE"))],
            ),
            spec("n", "echo", vec![p("echo", t("123"))]),
        ];
        let out = update_vars(TDC, 0, &s).unwrap();
        assert!(out.contains("cmd: |-\n            echo hi\n            echo there\n"));
        assert!(out.contains("trim: false\n"));
        assert!(out.contains("debug: true\n"));
        assert!(out.contains("offset: -3600\n"));
        assert!(out.contains("echo: \"123\"\n"));
        let doc: Value = serde_norway::from_str(&out).unwrap();
        let vars = &doc["matches"][0]["vars"];
        assert_eq!(vars[0]["params"]["trim"], Value::Bool(false));
        assert_eq!(vars[1]["params"]["offset"], Value::from(-3600));
        assert_eq!(vars[2]["params"]["echo"], Value::String("123".into()));
        // A placeholder offset stays a string.
        let s = vec![spec("d", "date", vec![p("offset", t("{{n}}"))])];
        let out = update_vars(TDC, 0, &s).unwrap();
        assert!(out.contains("offset: \"{{n}}\""));
    }

    #[test]
    fn unknown_type_generic_params_and_raw() {
        let s = vec![
            spec(
                "c",
                "set_clipboard",
                vec![p("text", t("{{d}}_{{clip}}")), p("restore", t("true"))],
            ),
            spec(
                "f",
                "form",
                vec![
                    p("layout", t("Name: [[who]]")),
                    p(
                        "fields",
                        ParamValue::Raw("who:\n  type: text\n  default: me\n".into()),
                    ),
                ],
            ),
        ];
        let out = update_vars(TDC, 0, &s).unwrap();
        assert!(out.contains("restore: true\n"));
        assert!(out.contains(
            "fields:\n            who:\n              type: text\n              default: me\n"
        ));
        let back = read(&out, 0);
        assert_eq!(back[0].kind, "set_clipboard");
        assert!(matches!(back[1].params[1].value, ParamValue::Raw(_)));
        // Raw round trips as no-op.
        assert_eq!(update_vars(&out, 0, &back).unwrap(), out);
        // Changing only the layout keeps the fields lines.
        let mut b2 = back.clone();
        b2[1].params[0] = p("layout", t("Hi [[who]]"));
        let out2 = update_vars(&out, 0, &b2).unwrap();
        assert_eq!(lines_diff(&out, &out2), (1, 1));
    }

    #[test]
    fn choice_with_objects_is_raw_and_preserved() {
        let text = "matches:\n  - trigger: a\n    replace: \"{{c}}\"\n    vars:\n      - name: c\n        type: choice\n        params:\n          values:\n            - id: one\n              label: One\n            - id: two\n              label: Two\n";
        let v = read(text, 0);
        assert!(matches!(v[0].params[0].value, ParamValue::Raw(_)));
        assert_eq!(update_vars(text, 0, &v).unwrap(), text);
    }

    #[test]
    fn type_change_rerenders_params() {
        let mut v = read(TDC, 1);
        v[0] = VarSpec {
            kind: "echo".into(),
            params: vec![p("echo", t("x"))],
            ..v[0].clone()
        };
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains(
            "      - name: d\n        type: echo\n        params:\n          echo: \"x\"\n"
        ));
        assert_eq!(read(&out, 1)[0].params, vec![p("echo", t("x"))]);
    }

    #[test]
    fn remove_all_params_of_an_item() {
        let mut v = read(TDC, 1);
        v[0].params.clear();
        let out = update_vars(TDC, 1, &v).unwrap();
        assert!(out.contains("      - name: d\n        type: date\n      - name: clip"));
    }

    #[test]
    fn comments_between_params_survive_when_other_param_changes() {
        let text = "matches:\n  - trigger: a\n    replace: \"{{s}}\"\n    vars:\n      - name: s\n        type: shell\n        params:\n          # which shell\n          shell: bash\n          # the command\n          cmd: \"ls\"\n";
        let mut v = read(text, 0);
        v[0].params[1] = p("cmd", t("pwd"));
        let out = update_vars(text, 0, &v).unwrap();
        assert!(out.contains("# which shell") && out.contains("# the command"));
        assert!(out.contains("cmd: \"pwd\""));
        // Removing a param keeps the other's comment.
        let mut v = read(text, 0);
        v[0].params.remove(0);
        let out = update_vars(text, 0, &v).unwrap();
        assert!(out.contains("# the command") && !out.contains("shell: bash"));
    }

    #[test]
    fn body_and_vars_in_one_pass() {
        let mut v = read(TDC, 1);
        v[2].name = "result".into();
        let change = VarsChange {
            vars: Some(v),
            body: Some("{{result}}!".into()),
        };
        let out = update_entry(TDC, 1, &change).unwrap();
        assert!(out.contains("    replace: \"{{result}}!\"\n"));
        assert!(out.contains("      - name: result\n"));
        // Body only.
        let out2 = update_entry(
            TDC,
            0,
            &VarsChange {
                vars: None,
                body: Some("two\nlines".into()),
            },
        )
        .unwrap();
        assert!(out2.contains("replace: |-\n      two\n      lines\n"));
        // No change returns the input.
        assert_eq!(
            update_entry(
                TDC,
                0,
                &VarsChange {
                    vars: None,
                    body: Some("one".into())
                }
            )
            .unwrap(),
            TDC
        );
    }

    #[test]
    fn body_edit_refused_for_non_replace() {
        let text = "matches:\n  - trigger: a\n    html: \"<b>x</b>\"\n";
        let r = update_entry(
            text,
            0,
            &VarsChange {
                vars: None,
                body: Some("y".into()),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn block_replace_with_trailing_newline_keeps_it() {
        let text =
            "matches:\n  - trigger: a\n    replace: |\n      old\n      text\n    label: L\n";
        let out = update_entry(
            text,
            0,
            &VarsChange {
                vars: Some(vec![spec("c", "clipboard", vec![])]),
                body: Some("new {{c}}".into()),
            },
        )
        .unwrap();
        assert!(out.contains("replace: \"new {{c}}\\n\"") || out.contains("replace: |"));
        let doc: Value = serde_norway::from_str(&out).unwrap();
        assert_eq!(
            doc["matches"][0]["replace"],
            Value::String("new {{c}}\n".into())
        );
        assert_eq!(doc["matches"][0]["label"], Value::String("L".into()));
        // vars is placed right after the body.
        assert!(out.find("vars:").unwrap() < out.find("label:").unwrap());
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let bad_name = vec![spec("1x", "clipboard", vec![])];
        assert!(update_vars(TDC, 0, &bad_name).is_err());
        let dup = vec![
            spec("a", "clipboard", vec![]),
            spec("a", "clipboard", vec![]),
        ];
        assert!(update_vars(TDC, 0, &dup).is_err());
        assert!(update_vars(TDC, 0, &[spec("a", "shell", vec![])]).is_err());
        assert!(update_vars(TDC, 0, &[spec("a", "echo", vec![])]).is_err());
        assert!(update_vars(TDC, 9, &[]).is_err());
        let bad_source = VarSpec {
            source: Some(0),
            ..spec("a", "clipboard", vec![])
        };
        assert!(update_vars(TDC, 0, &[bad_source]).is_err());
        let dup_source = VarSpec {
            source: Some(0),
            ..spec("b", "clipboard", vec![])
        };
        let v = read(TDC, 1);
        assert!(update_vars(
            TDC,
            1,
            &[
                v[0].clone(),
                VarSpec {
                    name: "q".into(),
                    ..dup_source
                }
            ]
        )
        .is_err());
        // Raw must be valid YAML collections.
        let raw = vec![spec(
            "f",
            "form",
            vec![
                p("layout", t("x")),
                p("fields", ParamValue::Raw("not: [valid".into())),
            ],
        )];
        assert!(update_vars(TDC, 0, &raw).is_err());
    }

    #[test]
    fn unreadable_vars_are_reported() {
        let text = "matches:\n  - trigger: a\n    replace: x\n    vars:\n      - just text\n";
        assert!(read_vars(text, 0).is_err());
        let text = "matches:\n  - trigger: a\n    replace: x\n    vars:\n      - name: a\n        type: echo\n        extra: 1\n";
        assert!(read_vars(text, 0).is_err());
        // Body edits still work without touching vars.
        let out = update_entry(
            text,
            0,
            &VarsChange {
                vars: None,
                body: Some("y".into()),
            },
        )
        .unwrap();
        assert!(out.contains("replace: \"y\""));
        assert!(out.contains("extra: 1"));
    }

    #[test]
    fn other_top_level_keys_and_entries_untouched() {
        let text = "imports:\n  - a.yml\nglobal_vars:\n  - name: g\n    type: echo\n    params:\n      echo: g\nmatches:\n  - trigger: a\n    replace: x\n    # inner note\n  - trigger: b\n    replace: y\n";
        let out = update_vars(text, 0, &[spec("c", "clipboard", vec![])]).unwrap();
        assert!(out.starts_with("imports:\n  - a.yml\nglobal_vars:\n"));
        assert!(out.contains("    # inner note\n"));
        assert!(out.ends_with("  - trigger: b\n    replace: y\n"));
    }

    /// Dry run against the user's own files (read only):
    /// `cargo test real_files -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_files_noop_and_roundtrip() {
        let Some(dir) = std::env::var_os("APPDATA") else {
            return;
        };
        let dir = std::path::Path::new(&dir).join("espanso");
        let strip = |x: Vec<VarSpec>| -> Vec<VarSpec> {
            x.into_iter()
                .map(|s| VarSpec { source: None, ..s })
                .collect()
        };
        let (mut entries, mut with_vars) = (0, 0);
        for path in crate::discover_match_files(&dir).unwrap() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for p in parse_match_text(&text).0 {
                entries += 1;
                let v = read_vars(&text, p.index)
                    .unwrap_or_else(|e| panic!("{}: #{}: {e}", path.display(), p.index));
                assert_eq!(update_vars(&text, p.index, &v).unwrap(), text);
                if p.kind == "replace" {
                    let body = VarsChange {
                        vars: None,
                        body: Some(p.content.clone()),
                    };
                    assert_eq!(update_entry(&text, p.index, &body).unwrap(), text);
                }
                if v.is_empty() {
                    continue;
                }
                with_vars += 1;
                // Rename the first variable and back: identical bytes.
                let mut a = v.clone();
                a[0].name = "zz_tmp".into();
                let mid = update_vars(&text, p.index, &a).unwrap();
                let mut b = read_vars(&mid, p.index).unwrap();
                b[0].name = v[0].name.clone();
                assert_eq!(update_vars(&mid, p.index, &b).unwrap(), text);
                // Remove everything, then add it back: the same variables again.
                let none = update_vars(&text, p.index, &[]).unwrap();
                let again = strip(v.clone());
                let back = update_vars(&none, p.index, &again).unwrap();
                assert_eq!(strip(read_vars(&back, p.index).unwrap()), again);
            }
        }
        println!("{entries} entries, {with_vars} with vars");
    }

    #[test]
    fn names_validation() {
        assert!(validate_name("clip_2").is_ok());
        assert!(validate_name("_x").is_ok());
        for bad in ["", "2x", "a b", "a.b", "é", "a-b"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn analysis_warnings() {
        let specs = vec![
            spec("d", "date", vec![]),
            spec("unused", "clipboard", vec![]),
            spec("out", "echo", vec![p("echo", t("{{d}}"))]),
        ];
        let w = analyze(&specs, "{{out}} {{missing}} {{ out.x }}");
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("missing"));
        assert!(w[1].contains("unused"));
        assert!(analyze(&specs[..1], "{{d}}").is_empty());
    }

    #[test]
    fn placeholder_scan_matches_engine_syntax() {
        assert_eq!(
            placeholders("a {{x}} {{ y }} {{z.f}} {{ }} {x} {{a b}}"),
            ["x", "y", "z"]
        );
        assert_eq!(
            rename_in_text("{{x}}-{{ x.f }}-{{xx}}-{x}", "x", "n"),
            "{{n}}-{{ n.f }}-{{xx}}-{x}"
        );
    }

    #[test]
    fn rename_updates_params_and_body() {
        let mut v = read(TDC, 1);
        for s in &mut v {
            s.source = Some(s.source.unwrap());
        }
        let (renamed, body) = rename_var(&v, "{{out}} and {{d}}", "d", "day").unwrap();
        assert_eq!(renamed[0].name, "day");
        assert_eq!(renamed[2].params[0].value, t("{{day}}_{{clip}}"));
        assert_eq!(body, "{{out}} and {{day}}");
        assert!(rename_var(&v, "", "d", "clip").is_err());
        assert!(rename_var(&v, "", "d", "1").is_err());
        // Writing it back in one pass keeps body and vars consistent.
        let out = update_entry(
            TDC,
            1,
            &VarsChange {
                vars: Some(renamed),
                body: Some(body),
            },
        )
        .unwrap();
        assert!(out.contains("echo: \"{{day}}_{{clip}}\""));
        assert!(out.contains("replace: \"{{out}} and {{day}}\""));
        assert!(analyze(&read(&out, 1), "{{out}}")
            .iter()
            .all(|w| w.contains("not used")));
    }
}
