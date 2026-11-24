//! Folders: the match files of espanso's `match/` directory and the apps they apply to.
//!
//! A folder is one `match/<name>.yml`. Which apps it applies to is decided by espanso's
//! `config/*.yml` files (app filters plus the match files each one loads). See
//! `docs/xpando/FOLDERS.md` for the rules this module implements and the layouts it refuses.
//!
//! Safety philosophy, as in `edit`: raw-text edits that keep every comment and unrelated
//! key, a re-parse that proves the edit did exactly what was meant, an in-memory check of
//! the result before anything is written, atomic writes, and a rollback if a step fails.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};

use super::edit::{content_hash, save_file, SaveError, CONFLICT_PREFIX};

const INCLUDE_KEYS: [&str; 4] = ["includes", "extra_includes", "excludes", "extra_excludes"];
const FILTER_KEYS: [&str; 4] = ["filter_exec", "filter_title", "filter_class", "filter_os"];
/// espanso's built-in include patterns (relative to the `config` directory).
const STANDARD_INCLUDES: [&str; 2] = ["../match/**/[!_]*.yml", "../match/**/[!_]*.yaml"];
const MAX_NAME_CHARS: usize = 64;

// ---------------------------------------------------------------------------------------
// Public data
// ---------------------------------------------------------------------------------------

/// Which apps a folder applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// Every app (loaded by the default config and by every app-specific config).
    All,
    /// Only the apps in `Scope::apps`.
    Only,
    /// Every app except the ones in `Scope::apps`.
    Except,
    /// No config loads it: its phrases never expand.
    Nowhere,
}

/// One app-specific config (`config/<name>.yml`) and its filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppScope {
    /// File name inside `config/`, e.g. `word.yml`.
    pub config: String,
    /// The file name without extension, e.g. `word`.
    pub name: String,
    /// The `label:` of the config, if it has one.
    pub label: Option<String>,
    /// Executable names when `filter_exec` is a simple list; empty otherwise.
    pub exe_names: Vec<String>,
    /// The raw `filter_exec` regular expression.
    pub exec_pattern: Option<String>,
    /// True when `exec_pattern` is something xpando can show and edit as a list of names.
    pub exec_simple: bool,
    /// The raw `filter_title` regular expression.
    pub title_pattern: Option<String>,
    /// The title text when `title_pattern` is a plain "contains" match.
    pub title_text: Option<String>,
    pub class_pattern: Option<String>,
    pub os: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope {
    pub kind: ScopeKind,
    pub apps: Vec<AppScope>,
}

/// Everything the UI needs to know about one folder besides its phrases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderInfo {
    /// Path relative to `match/`, as in `Folder::file`.
    pub file: String,
    pub name: String,
    pub scope: Scope,
    /// Why the folder cannot be renamed or deleted by xpando, if it cannot.
    pub locked: Option<String>,
    /// Why its apps cannot be changed by xpando, if they cannot.
    pub scope_locked: Option<String>,
    /// Config files (names inside `config/`) that name this folder in an include or exclude.
    pub configs: Vec<String>,
    /// Match files that import this folder.
    pub imported_by: Vec<String>,
}

/// One file touched (or to be touched) by an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// Path from the espanso config directory, `/` separators, e.g. `config/word.yml`.
    pub file: String,
    /// `create`, `edit`, `rename` or `delete`.
    pub action: String,
    /// One plain-language sentence.
    pub detail: String,
    /// Path of the backup kept for this change, if any.
    pub backup: Option<String>,
}

/// What an operation did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub changes: Vec<Change>,
    /// The folder's file after the operation (`None` after a delete).
    pub file: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum TitleChange {
    /// Leave the window-title filter as it is.
    #[default]
    Keep,
    /// Remove the window-title filter.
    Clear,
    /// Match windows whose title contains this text (case-insensitive).
    Contains(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopeSpec {
    All,
    Apps {
        /// Executable names; `None` leaves the current executable filter alone.
        #[serde(default)]
        exes: Option<Vec<String>>,
        #[serde(default)]
        title: TitleChange,
    },
}

// ---------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Checks a folder name typed by the user and returns it trimmed.
pub fn validate_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Type a name for the folder.".into());
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "The name is too long (at most {MAX_NAME_CHARS} characters)."
        ));
    }
    if n.contains(['/', '\\']) || n.contains("..") {
        return Err("The name cannot contain slashes or \"..\".".into());
    }
    if n.starts_with('_') {
        return Err(
            "The name cannot start with an underscore: xpando adds it itself for folders \
             that apply to only some apps."
                .into(),
        );
    }
    if n.starts_with('-') || n.starts_with('.') || n.ends_with('.') {
        return Err("The name cannot start with \"-\" or \".\", or end with \".\".".into());
    }
    if let Some(bad) = n
        .chars()
        .find(|c| !(c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | ',')))
    {
        return Err(format!(
            "The name cannot contain \"{bad}\". Use letters, digits, spaces, \",\", \"-\" and \"_\"."
        ));
    }
    if n.contains("  ") {
        return Err("The name cannot contain two spaces in a row.".into());
    }
    let stem = n.split('.').next().unwrap_or("").to_lowercase();
    if RESERVED.contains(&stem.trim_end().to_string().as_str()) {
        return Err(format!("\"{n}\" is a reserved name on Windows."));
    }
    Ok(n.to_string())
}

fn split_ext(file: &str) -> Option<(&str, &str)> {
    let dot = file.rfind('.')?;
    let ext = &file[dot..];
    matches!(ext, ".yml" | ".yaml").then(|| (&file[..dot], ext))
}

/// The name a folder shows: the file name without extension and without a leading `_`.
fn display_name(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file);
    let stem = split_ext(base).map_or(base, |(s, _)| s);
    let n = stem.trim_start_matches('_');
    if n.is_empty() { stem } else { n }.to_string()
}

// ---------------------------------------------------------------------------------------
// Layout: everything the analysis needs, in memory
// ---------------------------------------------------------------------------------------

/// A snapshot of the espanso config directory. Plans are checked against a modified copy of
/// this before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// The config directory as a string with `/` separators (for absolute include paths).
    pub root: String,
    /// `config/*.yml|yaml` (top level only, as espanso loads them): file name -> text.
    pub configs: BTreeMap<String, String>,
    /// Config files that could not be read as UTF-8 text.
    pub unreadable: BTreeSet<String>,
    /// Every match file under `match/`, as paths relative to it with `/` separators.
    pub match_files: BTreeSet<String>,
    /// Raw `imports:` entries of each match file.
    pub imports: BTreeMap<String, Vec<String>>,
    /// Every other file directly in `match/` and `config/` (backups), as `match/x.bak`.
    pub other_files: BTreeSet<String>,
}

impl Layout {
    pub fn from_disk(config_dir: &Path) -> Result<Layout, String> {
        let mut l = Layout {
            root: config_dir.to_string_lossy().replace('\\', "/"),
            ..Layout::default()
        };
        let cfg_dir = config_dir.join("config");
        if cfg_dir.is_dir() {
            for entry in fs::read_dir(&cfg_dir).map_err(|e| format!("Cannot read config: {e}"))? {
                let path = entry.map_err(|e| e.to_string())?.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !path.is_file() {
                    continue;
                }
                if split_ext(name).is_some() {
                    match fs::read(&path).map(String::from_utf8) {
                        Ok(Ok(text)) => {
                            l.configs.insert(name.to_string(), text);
                        }
                        _ => {
                            l.unreadable.insert(name.to_string());
                        }
                    }
                } else {
                    l.other_files.insert(format!("config/{name}"));
                }
            }
        }
        let match_dir = config_dir.join("match");
        for path in super::discover_match_files(config_dir).map_err(|e| e.to_string())? {
            let rel = path
                .strip_prefix(&match_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if let Ok(text) = fs::read_to_string(&path) {
                let imports = read_imports(&text);
                if !imports.is_empty() {
                    l.imports.insert(rel.clone(), imports);
                }
            }
            l.match_files.insert(rel);
        }
        if match_dir.is_dir() {
            for entry in fs::read_dir(&match_dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if path.is_file() && split_ext(name).is_none() {
                        l.other_files.insert(format!("match/{name}"));
                    }
                }
            }
        }
        Ok(l)
    }
}

fn read_imports(text: &str) -> Vec<String> {
    let Ok(Value::Mapping(m)) = serde_norway::from_str::<Value>(text) else {
        return Vec::new();
    };
    match m.get("imports") {
        Some(Value::Sequence(s)) => s
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------------------
// Globs and paths (espanso resolves include/exclude entries as globs relative to `config/`)
// ---------------------------------------------------------------------------------------

fn has_meta(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

fn glob_segment(p: &[char], t: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('*') => (0..=t.len()).any(|i| glob_segment(&p[1..], &t[i..])),
        Some('?') => !t.is_empty() && glob_segment(&p[1..], &t[1..]),
        Some('[') => match p.iter().skip(2).position(|&c| c == ']') {
            Some(rel) => {
                let close = rel + 2;
                let (neg, body) = match p.get(1) {
                    Some('!') => (true, &p[2..close]),
                    _ => (false, &p[1..close]),
                };
                let Some(&c) = t.first() else { return false };
                let mut hit = false;
                let mut i = 0;
                while i < body.len() {
                    if i + 2 < body.len() && body[i + 1] == '-' {
                        hit |= body[i] <= c && c <= body[i + 2];
                        i += 3;
                    } else {
                        hit |= body[i] == c;
                        i += 1;
                    }
                }
                hit != neg && glob_segment(&p[close + 1..], &t[1..])
            }
            None => t.first() == Some(&'[') && glob_segment(&p[1..], &t[1..]),
        },
        Some(&c) => t.first() == Some(&c) && glob_segment(&p[1..], &t[1..]),
    }
}

fn glob_path(p: &[String], t: &[String]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some(s) if s == "**" => {
            // A trailing `**` only matches directories, never a file.
            p.len() > 1 && (0..=t.len()).any(|i| glob_path(&p[1..], &t[i..]))
        }
        Some(s) => {
            !t.is_empty()
                && glob_segment(
                    &s.chars().collect::<Vec<_>>(),
                    &t[0].chars().collect::<Vec<_>>(),
                )
                && glob_path(&p[1..], &t[1..])
        }
    }
}

/// Splits an include/exclude entry into path segments below the config directory's parent
/// (so `../match/x.yml` becomes `["match", "x.yml"]`). `abs` is true for absolute entries
/// that point inside the config directory. `None` when it cannot point inside it.
fn entry_segments(entry: &str, root: &str) -> Option<(Vec<String>, bool)> {
    let e = entry.replace('\\', "/");
    let bytes = e.as_bytes();
    let absolute = e.starts_with('/') || (bytes.len() > 2 && bytes[1] == b':' && bytes[2] == b'/');
    let (mut stack, rest, abs): (Vec<String>, &str, bool) = if absolute {
        let (el, rl) = (e.to_lowercase(), root.trim_end_matches('/').to_lowercase());
        if rl.is_empty() || !el.starts_with(&format!("{rl}/")) {
            return None;
        }
        (Vec::new(), e.get(rl.len() + 1..)?, true)
    } else {
        (vec!["config".to_string()], e.as_str(), false)
    };
    for seg in rest.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            s => stack.push(s.to_string()),
        }
    }
    Some((stack, abs))
}

/// Whether `entry` (from a config in the config directory) matches `match/<file>`.
fn entry_matches(entry: &str, file: &str, root: &str) -> Option<bool> {
    let (segs, _) = entry_segments(entry, root)?;
    let target: Vec<String> = std::iter::once("match".to_string())
        .chain(file.split('/').map(String::from))
        .collect();
    Some(glob_path(&segs, &target))
}

#[cfg(test)]
fn standard_matches(file: &str) -> bool {
    STANDARD_INCLUDES
        .iter()
        .any(|p| entry_matches(p, file, "").unwrap_or(false))
}

// ---------------------------------------------------------------------------------------
// Parsed configs and analysis
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawConfig {
    includes: Option<Vec<String>>,
    extra_includes: Option<Vec<String>>,
    excludes: Option<Vec<String>>,
    extra_excludes: Option<Vec<String>>,
    use_standard_includes: Option<bool>,
    filter_exec: Option<String>,
    filter_title: Option<String>,
    filter_class: Option<String>,
    filter_os: Option<String>,
    label: Option<String>,
}

impl RawConfig {
    fn list(&self, key: &str) -> &[String] {
        match key {
            "includes" => self.includes.as_deref(),
            "extra_includes" => self.extra_includes.as_deref(),
            "excludes" => self.excludes.as_deref(),
            _ => self.extra_excludes.as_deref(),
        }
        .unwrap_or(&[])
    }

    fn has_filter(&self) -> bool {
        self.filter_exec.is_some()
            || self.filter_title.is_some()
            || self.filter_class.is_some()
            || self.filter_os.is_some()
    }

    /// espanso fills every field a custom config leaves out from `default.yml`.
    fn inherit(&self, d: &RawConfig) -> RawConfig {
        RawConfig {
            includes: self.includes.clone().or_else(|| d.includes.clone()),
            extra_includes: self
                .extra_includes
                .clone()
                .or_else(|| d.extra_includes.clone()),
            excludes: self.excludes.clone().or_else(|| d.excludes.clone()),
            extra_excludes: self
                .extra_excludes
                .clone()
                .or_else(|| d.extra_excludes.clone()),
            use_standard_includes: self.use_standard_includes.or(d.use_standard_includes),
            filter_exec: self.filter_exec.clone().or_else(|| d.filter_exec.clone()),
            filter_title: self.filter_title.clone().or_else(|| d.filter_title.clone()),
            filter_class: self.filter_class.clone().or_else(|| d.filter_class.clone()),
            filter_os: self.filter_os.clone().or_else(|| d.filter_os.clone()),
            label: self.label.clone().or_else(|| d.label.clone()),
        }
    }
}

fn mapping_of(text: &str) -> Result<Mapping, String> {
    match serde_norway::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Null) => Ok(Mapping::new()),
        Ok(Value::Mapping(m)) => Ok(m),
        Ok(_) => Err("the file is not a mapping of settings".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn str_list(m: &Mapping, key: &str) -> Result<Option<Vec<String>>, String> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Sequence(s)) => s
            .iter()
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("`{key}` must be a list of text"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("`{key}` must be a list")),
    }
}

fn opt_str(m: &Mapping, key: &str) -> Result<Option<String>, String> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{key}` must be text")),
    }
}

fn parse_config(text: &str) -> Result<RawConfig, String> {
    let m = mapping_of(text)?;
    let use_standard_includes = match m.get("use_standard_includes") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err("`use_standard_includes` must be true or false".into()),
    };
    Ok(RawConfig {
        includes: str_list(&m, "includes")?,
        extra_includes: str_list(&m, "extra_includes")?,
        excludes: str_list(&m, "excludes")?,
        extra_excludes: str_list(&m, "extra_excludes")?,
        use_standard_includes,
        filter_exec: opt_str(&m, "filter_exec")?,
        filter_title: opt_str(&m, "filter_title")?,
        filter_class: opt_str(&m, "filter_class")?,
        filter_os: opt_str(&m, "filter_os")?,
        label: opt_str(&m, "label")?,
    })
}

#[derive(Debug, Clone)]
struct CfgInfo {
    file: String,
    is_default: bool,
    /// The config's own settings; `Err` when espanso would skip the file.
    own: Result<RawConfig, String>,
    /// Own settings with the defaults filled in.
    eff: Option<RawConfig>,
    loads: BTreeSet<String>,
}

impl CfgInfo {
    fn active_custom(&self) -> bool {
        !self.is_default && self.eff.as_ref().is_some_and(RawConfig::has_filter)
    }
}

struct Analysis {
    configs: Vec<CfgInfo>,
}

fn default_name(l: &Layout) -> Option<String> {
    ["default.yml", "default.yaml"]
        .iter()
        .find(|n| l.configs.contains_key(**n) || l.unreadable.contains(**n))
        .map(|s| s.to_string())
}

fn resolve_import(importer: &str, import: &str) -> Option<String> {
    let norm = import.replace('\\', "/");
    if norm.starts_with('/') || norm.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let mut stack: Vec<&str> = importer.split('/').collect();
    stack.pop();
    for seg in norm.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            s => stack.push(s),
        }
    }
    Some(stack.join("/"))
}

fn load_set(raw: &RawConfig, l: &Layout) -> BTreeSet<String> {
    let mut inc: Vec<&str> = Vec::new();
    if raw.use_standard_includes != Some(false) {
        inc.extend(STANDARD_INCLUDES);
    }
    for k in ["includes", "extra_includes"] {
        inc.extend(raw.list(k).iter().map(String::as_str));
    }
    let exc: Vec<&str> = ["excludes", "extra_excludes"]
        .iter()
        .flat_map(|k| raw.list(k).iter().map(String::as_str))
        .collect();
    let hit = |pats: &[&str], f: &str| {
        pats.iter()
            .any(|p| entry_matches(p, f, &l.root).unwrap_or(false))
    };
    let mut set: BTreeSet<String> = l
        .match_files
        .iter()
        .filter(|f| hit(&inc, f) && !hit(&exc, f))
        .cloned()
        .collect();
    // Files imported by loaded files are loaded too.
    let mut queue: Vec<String> = set.iter().cloned().collect();
    while let Some(f) = queue.pop() {
        for imp in l.imports.get(&f).into_iter().flatten() {
            if let Some(r) = resolve_import(&f, imp) {
                if l.match_files.contains(&r) && set.insert(r.clone()) {
                    queue.push(r);
                }
            }
        }
    }
    set
}

fn analyze(l: &Layout) -> Analysis {
    let dname = default_name(l);
    let default_raw = dname
        .as_ref()
        .and_then(|n| l.configs.get(n))
        .and_then(|t| parse_config(t).ok());
    let mut configs = Vec::new();
    for (name, text) in &l.configs {
        let is_default = dname.as_deref() == Some(name.as_str());
        let own = parse_config(text);
        let eff = match (&own, is_default) {
            (Ok(o), true) => Some(o.clone()),
            (Ok(o), false) => Some(match &default_raw {
                Some(d) => o.inherit(d),
                None => o.clone(),
            }),
            _ => None,
        };
        let loads = eff.as_ref().map_or_else(BTreeSet::new, |e| load_set(e, l));
        configs.push(CfgInfo {
            file: name.clone(),
            is_default,
            own,
            eff,
            loads,
        });
    }
    for name in &l.unreadable {
        configs.push(CfgInfo {
            file: name.clone(),
            is_default: dname.as_deref() == Some(name.as_str()),
            own: Err("the file is not valid UTF-8 text".into()),
            eff: None,
            loads: BTreeSet::new(),
        });
    }
    Analysis { configs }
}

fn unescape_regex(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                let n = it.next()?;
                if n.is_alphanumeric() {
                    return None;
                }
                out.push(n);
            }
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '#'
            | '&' | '~' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '.'
                | '+'
                | '*'
                | '?'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '{'
                | '}'
                | '^'
                | '$'
                | '#'
                | '&'
                | '-'
                | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Reads executable names from `(?i)(a|b)`-style patterns, including the one xpando writes.
fn parse_exec_pattern(p: &str) -> Option<Vec<String>> {
    let mut s = p.strip_prefix("(?i)")?;
    let anchored = s.starts_with(r"(^|[\\/])") && s.ends_with(r"(\.exe)?$");
    if anchored {
        s = &s[r"(^|[\\/])".len()..s.len() - r"(\.exe)?$".len()];
    }
    let s = s
        .strip_prefix('(')
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(s);
    let names: Option<Vec<String>> = s
        .split('|')
        .map(|a| unescape_regex(a).filter(|n| !n.is_empty()))
        .collect();
    names
}

fn parse_title_pattern(p: &str) -> Option<String> {
    unescape_regex(p.strip_prefix("(?i)")?).filter(|t| !t.is_empty())
}

fn app_scope(c: &CfgInfo) -> AppScope {
    let eff = c.eff.clone().unwrap_or_default();
    let name = split_ext(&c.file)
        .map_or(c.file.as_str(), |(s, _)| s)
        .to_string();
    let names = eff.filter_exec.as_deref().and_then(parse_exec_pattern);
    AppScope {
        config: c.file.clone(),
        name,
        label: eff.label.clone(),
        exec_simple: names.is_some(),
        exe_names: names.unwrap_or_default(),
        exec_pattern: eff.filter_exec.clone(),
        title_text: eff.filter_title.as_deref().and_then(parse_title_pattern),
        title_pattern: eff.filter_title.clone(),
        class_pattern: eff.filter_class.clone(),
        os: eff.filter_os.clone(),
    }
}

impl Analysis {
    fn default_loads(&self, file: &str) -> bool {
        self.configs
            .iter()
            .any(|c| c.is_default && c.loads.contains(file))
    }

    fn scope(&self, file: &str) -> Scope {
        let customs: Vec<&CfgInfo> = self.configs.iter().filter(|c| c.active_custom()).collect();
        let loading: Vec<&CfgInfo> = customs
            .iter()
            .copied()
            .filter(|c| c.loads.contains(file))
            .collect();
        let (kind, who) = if self.default_loads(file) {
            if loading.len() == customs.len() {
                (ScopeKind::All, Vec::new())
            } else {
                let rest = customs
                    .iter()
                    .copied()
                    .filter(|c| !c.loads.contains(file))
                    .collect();
                (ScopeKind::Except, rest)
            }
        } else if loading.is_empty() {
            (ScopeKind::Nowhere, Vec::new())
        } else {
            (ScopeKind::Only, loading)
        };
        Scope {
            kind,
            apps: who.into_iter().map(app_scope).collect(),
        }
    }
}

/// A scope reduced to what matters to espanso (not config file names), for before/after checks.
fn signature(s: &Scope) -> (ScopeKind, BTreeSet<[Option<String>; 4]>) {
    (
        s.kind,
        s.apps
            .iter()
            .map(|a| {
                [
                    a.exec_pattern.clone(),
                    a.title_pattern.clone(),
                    a.class_pattern.clone(),
                    a.os.clone(),
                ]
            })
            .collect(),
    )
}

/// A config entry that names or matches a folder.
#[derive(Debug, Clone)]
struct Ref {
    config: String,
    key: &'static str,
    entry: String,
    /// A plain path (no wildcards), which xpando can rewrite or remove.
    literal: bool,
    /// An absolute path inside the config directory.
    abs: bool,
}

fn refs_to(l: &Layout, a: &Analysis, file: &str) -> Vec<Ref> {
    let mut out = Vec::new();
    for c in &a.configs {
        let Ok(own) = &c.own else { continue };
        for key in INCLUDE_KEYS {
            for entry in own.list(key) {
                if let Some((_, abs)) = entry_segments(entry, &l.root) {
                    if entry_matches(entry, file, &l.root) == Some(true) {
                        out.push(Ref {
                            config: c.file.clone(),
                            key,
                            entry: entry.clone(),
                            literal: !has_meta(entry),
                            abs,
                        });
                    }
                }
            }
        }
    }
    out
}

fn imported_by(l: &Layout, file: &str) -> Vec<String> {
    let mut by = Vec::new();
    for (importer, imps) in &l.imports {
        if imps
            .iter()
            .any(|i| resolve_import(importer, i).as_deref() == Some(file))
        {
            by.push(importer.clone());
        }
    }
    by
}

fn manageable(file: &str) -> Result<(), String> {
    if file.contains('/') {
        return if file.starts_with("packages/") {
            Err("It belongs to an installed espanso package. Change packages with espanso, not here."
                .into())
        } else {
            Err("It is inside a sub-folder of match/, which xpando does not manage yet.".into())
        };
    }
    if split_ext(file).is_none() {
        return Err("It is not a .yml or .yaml file.".into());
    }
    Ok(())
}

/// Why this folder's name or existence cannot be changed (shared by rename and delete).
fn lock_reason(l: &Layout, a: &Analysis, file: &str) -> Option<String> {
    if let Err(e) = manageable(file) {
        return Some(e);
    }
    let imp = imported_by(l, file);
    if !imp.is_empty() {
        return Some(format!(
            "It is imported by {}. Remove that import first (in Advanced mode, YAML tab).",
            imp.join(", ")
        ));
    }
    if let Some(r) = refs_to(l, a, file).iter().find(|r| r.abs) {
        return Some(format!(
            "config/{} refers to it with an absolute path, which xpando does not rewrite.",
            r.config
        ));
    }
    if let Some(c) = a
        .configs
        .iter()
        .find(|c| c.own.is_err() && !c.is_default && l.unreadable.contains(&c.file))
    {
        return Some(format!(
            "config/{} cannot be read, so xpando cannot tell whether it uses this folder.",
            c.file
        ));
    }
    None
}

/// Why the apps of this folder cannot be changed by xpando.
fn scope_lock_reason(l: &Layout, a: &Analysis, file: &str) -> Option<String> {
    if let Some(r) = lock_reason(l, a, file) {
        return Some(r);
    }
    for r in refs_to(l, a, file) {
        let is_default = a.configs.iter().any(|c| c.is_default && c.file == r.config);
        if r.key.contains("exclude") {
            return Some(format!(
                "config/{} excludes it (\"{}\"). Edit that file by hand.",
                r.config, r.entry
            ));
        }
        if !r.literal {
            return Some(format!(
                "config/{} loads it through the pattern \"{}\". Edit that file by hand.",
                r.config, r.entry
            ));
        }
        if is_default {
            return Some(format!(
                "config/{} (the default config) lists it directly. Edit that file by hand.",
                r.config
            ));
        }
    }
    if let Some(c) = a.configs.iter().find(|c| c.own.is_err()) {
        return Some(format!(
            "config/{} cannot be read as settings, so xpando cannot tell how it applies.",
            c.file
        ));
    }
    for c in &a.configs {
        if let Ok(o) = &c.own {
            if c.is_default && o.has_filter() {
                return Some(
                    "default.yml has app filters, so every other config inherits them. \
                     Edit that file by hand."
                        .into(),
                );
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------------------
// Public read API
// ---------------------------------------------------------------------------------------

pub fn folder_infos(l: &Layout) -> Vec<FolderInfo> {
    let a = analyze(l);
    l.match_files
        .iter()
        .map(|file| {
            let mut configs: Vec<String> =
                refs_to(l, &a, file).into_iter().map(|r| r.config).collect();
            configs.dedup();
            FolderInfo {
                file: file.clone(),
                name: display_name(file),
                scope: a.scope(file),
                locked: lock_reason(l, &a, file),
                scope_locked: scope_lock_reason(l, &a, file),
                configs,
                imported_by: imported_by(l, file),
            }
        })
        .collect()
}

/// Scope and lock information for every match file.
pub fn list_folder_info(config_dir: &Path) -> Result<Vec<FolderInfo>, String> {
    Ok(folder_infos(&Layout::from_disk(config_dir)?))
}

// ---------------------------------------------------------------------------------------
// Line-based config editing
// ---------------------------------------------------------------------------------------

struct Doc {
    bom: bool,
    eol: &'static str,
    ends_nl: bool,
    lines: Vec<String>,
}

impl Doc {
    fn parse(text: &str) -> Result<Doc, String> {
        let (bom, body) = match text.strip_prefix('\u{feff}') {
            Some(b) => (true, b),
            None => (false, text),
        };
        let crlf = body.matches("\r\n").count();
        let lf = body.matches('\n').count();
        if crlf > 0 && crlf != lf {
            return Err("The file mixes Windows and Unix line endings.".into());
        }
        if body.matches('\r').count() != crlf {
            return Err("The file contains a stray carriage return.".into());
        }
        let eol = if crlf > 0 { "\r\n" } else { "\n" };
        let ends_nl = body.is_empty() || body.ends_with('\n');
        let mut lines: Vec<String> = body.split(eol).map(String::from).collect();
        if body.is_empty() || ends_nl {
            lines.pop();
        }
        Ok(Doc {
            bom,
            eol,
            ends_nl,
            lines,
        })
    }

    fn render(&self) -> String {
        let mut out = String::new();
        if self.bom {
            out.push('\u{feff}');
        }
        out.push_str(&self.lines.join(self.eol));
        if self.ends_nl && !self.lines.is_empty() {
            out.push_str(self.eol);
        }
        out
    }
}

#[derive(Debug, Clone)]
struct Region {
    key: String,
    start: usize,
    /// One past the last line that belongs to the key's value.
    end: usize,
}

fn key_of(line: &str) -> Option<(&str, &str)> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    let rest = &line[colon + 1..];
    let ok_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    (ok_key && (rest.is_empty() || rest.starts_with(' '))).then_some((key, rest))
}

fn regions(lines: &[String]) -> Result<Vec<Region>, String> {
    let mut out: Vec<Region> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let first = line.chars().next();
        let blank = line.trim().is_empty();
        if blank || first == Some('#') {
            continue;
        }
        if first == Some('\t') {
            return Err(format!("Line {} starts with a tab.", i + 1));
        }
        if first == Some(' ') || (first == Some('-') && !line.starts_with("---")) {
            match out.last_mut() {
                Some(r) => {
                    if !line.trim_start().starts_with('#') {
                        r.end = i + 1;
                    }
                }
                None => return Err(format!("Line {} is not part of any setting.", i + 1)),
            }
            continue;
        }
        if line.trim_end() == "---" && out.is_empty() {
            continue;
        }
        match key_of(line) {
            Some((k, _)) => {
                if out.iter().any(|r| r.key == k) {
                    return Err(format!("The setting `{k}` appears twice."));
                }
                out.push(Region {
                    key: k.to_string(),
                    start: i,
                    end: i + 1,
                });
            }
            None => return Err(format!("Line {} has a layout xpando does not edit.", i + 1)),
        }
    }
    Ok(out)
}

/// Splits `text` (after a key or a list dash) into the scalar and what trails it.
fn split_scalar(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim_start();
    let complex = || "a value with a layout xpando does not edit".to_string();
    let end = match s.chars().next() {
        Some('"') => {
            let mut esc = false;
            let mut end = None;
            for (i, c) in s.char_indices().skip(1) {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    end = Some(i + 1);
                    break;
                }
            }
            end.ok_or_else(complex)?
        }
        Some('\'') => {
            let b = s.as_bytes();
            let mut i = 1;
            loop {
                match b.get(i) {
                    None => return Err(complex()),
                    Some(b'\'') if b.get(i + 1) == Some(&b'\'') => i += 2,
                    Some(b'\'') => break i + 1,
                    Some(_) => i += 1,
                }
            }
        }
        Some('[' | '{' | '|' | '>' | '&' | '*' | '!' | '%' | '@' | '`') | None => {
            return Err(complex())
        }
        Some(_) => s.find(" #").unwrap_or(s.len()),
    };
    let (scalar, _) = s.split_at(end);
    let scalar_end = scalar.trim_end();
    let trailer = &s[scalar_end.len()..];
    if !(trailer.trim().is_empty() || trailer.trim_start().starts_with('#')) {
        return Err(complex());
    }
    Ok((scalar_end, trailer))
}

fn scalar_value(raw: &str) -> Result<String, String> {
    match raw.chars().next() {
        Some('"' | '\'') => {
            serde_norway::from_str::<String>(raw).map_err(|e| format!("cannot read `{raw}`: {e}"))
        }
        _ => Ok(raw.to_string()),
    }
}

fn dq(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn plain_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
        && !s.starts_with('-')
        && !matches!(
            s,
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
        )
}

/// Writes `value` quoted like `like` (a scalar as it appears in the file).
fn quote_like(like: &str, value: &str) -> String {
    match like.chars().next() {
        Some('\'') => sq(value),
        Some('"') => dq(value),
        _ if plain_safe(value) => value.to_string(),
        _ => dq(value),
    }
}

#[derive(Debug)]
struct Item {
    line: usize,
    /// Text of the line before the scalar (indent, dash and gap).
    lead: String,
    scalar: String,
    trailer: String,
    value: String,
}

fn read_items(doc: &Doc, r: &Region) -> Result<Vec<Item>, String> {
    let head = &doc.lines[r.start];
    let (_, rest) = key_of(head).ok_or("unreadable line")?;
    let rest = rest.trim();
    if !(rest.is_empty() || rest.starts_with('#')) {
        return Err(format!(
            "`{}` is written on one line; xpando edits lists with one item per line.",
            r.key
        ));
    }
    let mut items = Vec::new();
    for (i, line) in doc.lines.iter().enumerate().take(r.end).skip(r.start + 1) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let after = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("-\t"))
            .ok_or_else(|| {
                format!(
                    "`{}` has a layout xpando does not edit (line {}).",
                    r.key,
                    i + 1
                )
            })?;
        let lead_len = line.len() - after.trim_start().len();
        let (scalar, trailer) = split_scalar(after)?;
        items.push(Item {
            line: i,
            lead: line[..lead_len].to_string(),
            scalar: scalar.to_string(),
            trailer: trailer.to_string(),
            value: scalar_value(scalar)?,
        });
    }
    Ok(items)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigEdit {
    RemoveEntry {
        key: String,
        entry: String,
    },
    ReplaceEntry {
        key: String,
        old: String,
        new: String,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    AddEntry {
        key: String,
        value: String,
    },
    /// Sets (`Some`) or removes (`None`) a one-line text setting such as `filter_exec`.
    SetText {
        key: String,
        value: Option<String>,
    },
}

fn apply_to_doc(doc: &mut Doc, edit: &ConfigEdit) -> Result<(), String> {
    let rs = regions(&doc.lines)?;
    match edit {
        ConfigEdit::RemoveEntry { key, entry } => {
            let r = rs.iter().find(|r| &r.key == key).ok_or("setting missing")?;
            let items = read_items(doc, r)?;
            let drop: Vec<usize> = items
                .iter()
                .filter(|i| &i.value == entry)
                .map(|i| i.line)
                .collect();
            if drop.is_empty() {
                return Err(format!("`{entry}` is not in `{key}`."));
            }
            let all = drop.len() == items.len();
            let mut kill: BTreeSet<usize> = drop.into_iter().collect();
            if all {
                kill.insert(r.start);
            }
            let mut i = 0;
            doc.lines.retain(|_| {
                i += 1;
                !kill.contains(&(i - 1))
            });
        }
        ConfigEdit::ReplaceEntry { key, old, new } => {
            let r = rs.iter().find(|r| &r.key == key).ok_or("setting missing")?;
            let items = read_items(doc, r)?;
            let mut hit = false;
            for it in items.iter().filter(|i| &i.value == old) {
                doc.lines[it.line] =
                    format!("{}{}{}", it.lead, quote_like(&it.scalar, new), it.trailer);
                hit = true;
            }
            if !hit {
                return Err(format!("`{old}` is not in `{key}`."));
            }
        }
        ConfigEdit::AddEntry { key, value } => match rs.iter().find(|r| &r.key == key) {
            Some(r) => {
                let items = read_items(doc, r)?;
                let (at, lead, like) = match items.last() {
                    Some(last) => (last.line + 1, last.lead.clone(), last.scalar.clone()),
                    None => (r.start + 1, "  - ".to_string(), "\"".to_string()),
                };
                doc.lines
                    .insert(at, format!("{lead}{}", quote_like(&like, value)));
            }
            None => {
                let at = rs.last().map_or(doc.lines.len(), |r| r.end);
                doc.lines.insert(at, format!("  - {}", dq(value)));
                doc.lines.insert(at, format!("{key}:"));
                doc.ends_nl = true;
            }
        },
        ConfigEdit::SetText { key, value } => {
            let existing = rs.iter().find(|r| &r.key == key);
            match (existing, value) {
                (Some(r), Some(v)) => {
                    let head = doc.lines[r.start].clone();
                    let (_, rest) = key_of(&head).ok_or("unreadable line")?;
                    let (_, trailer) = split_scalar(rest)?;
                    if r.end != r.start + 1 {
                        return Err(format!("`{key}` spans several lines."));
                    }
                    doc.lines[r.start] = format!("{key}: {}{trailer}", sq(v));
                }
                (Some(r), None) => {
                    if r.end != r.start + 1 {
                        return Err(format!("`{key}` spans several lines."));
                    }
                    doc.lines.remove(r.start);
                }
                (None, Some(v)) => {
                    // After the last filter setting, else after the last setting.
                    let after_filter = rs
                        .iter()
                        .filter(|r| r.key.starts_with("filter_"))
                        .map(|r| r.end)
                        .max();
                    let at = after_filter
                        .or_else(|| rs.last().map(|r| r.end))
                        .unwrap_or(doc.lines.len());
                    doc.lines.insert(at, format!("{key}: {}", sq(v)));
                    doc.ends_nl = true;
                }
                (None, None) => {}
            }
        }
    }
    Ok(())
}

fn same_map(a: &Mapping, b: &Mapping) -> bool {
    a.len() == b.len() && a.iter().all(|(k, v)| b.get(k) == Some(v))
}

fn apply_to_map(m: &mut Mapping, edit: &ConfigEdit) {
    match edit {
        ConfigEdit::RemoveEntry { key, entry } => {
            if let Some(Value::Sequence(s)) = m.get_mut(key.as_str()) {
                s.retain(|v| v.as_str() != Some(entry));
                if s.is_empty() {
                    m.remove(key.as_str());
                }
            }
        }
        ConfigEdit::ReplaceEntry { key, old, new } => {
            if let Some(Value::Sequence(s)) = m.get_mut(key.as_str()) {
                for v in s.iter_mut().filter(|v| v.as_str() == Some(old)) {
                    *v = Value::String(new.clone());
                }
            }
        }
        ConfigEdit::AddEntry { key, value } => match m.get_mut(key.as_str()) {
            Some(Value::Sequence(s)) => s.push(Value::String(value.clone())),
            _ => {
                m.insert(
                    Value::String(key.clone()),
                    Value::Sequence(vec![Value::String(value.clone())]),
                );
            }
        },
        ConfigEdit::SetText { key, value } => match value {
            Some(v) => {
                m.insert(Value::String(key.clone()), Value::String(v.clone()));
            }
            None => {
                m.remove(key.as_str());
            }
        },
    }
}

/// Applies `edits` to the text of a config file, keeping comments and every other line, and
/// proves with a re-parse that the result is the old settings plus exactly those edits.
fn edit_config(text: &str, edits: &[ConfigEdit]) -> Result<String, String> {
    let old = mapping_of(text).map_err(|e| format!("The file is not valid YAML: {e}"))?;
    let mut doc = Doc::parse(text)?;
    let rs = regions(&doc.lines)?;
    let scanned: BTreeSet<&str> = rs.iter().map(|r| r.key.as_str()).collect();
    let parsed: BTreeSet<&str> = old.keys().filter_map(Value::as_str).collect();
    if scanned != parsed || old.len() != parsed.len() {
        return Err("The file's layout is not one xpando can edit safely.".into());
    }
    let mut expected = old.clone();
    for e in edits {
        apply_to_doc(&mut doc, e)?;
        apply_to_map(&mut expected, e);
    }
    let new_text = doc.render();
    let got = mapping_of(&new_text).map_err(|e| format!("The edit would break the file: {e}"))?;
    if !same_map(&got, &expected) {
        return Err("The edit would change more than intended, so it was not made.".into());
    }
    Ok(new_text)
}

/// True when the config would hold nothing but app filters.
fn only_filters_left(text: &str) -> bool {
    mapping_of(text).is_ok_and(|m| {
        m.keys()
            .filter_map(Value::as_str)
            .all(|k| FILTER_KEYS.contains(&k) || k == "label")
    })
}

// ---------------------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Op {
    WriteConfig {
        name: String,
        text: String,
    },
    CreateConfig {
        name: String,
        text: String,
    },
    /// Moves the config to a backup name.
    RemoveConfig {
        name: String,
        backup: String,
    },
    RenameConfig {
        from: String,
        to: String,
    },
    CreateMatch {
        name: String,
        text: String,
    },
    RemoveMatch {
        name: String,
        backup: String,
    },
    RenameMatch {
        from: String,
        to: String,
    },
    /// Replaces the text of a match file that still hashes to `expect`.
    WriteMatch {
        name: String,
        text: String,
        expect: String,
    },
}

#[derive(Debug)]
struct Plan {
    ops: Vec<Op>,
    changes: Vec<Change>,
    after: Layout,
}

impl Plan {
    fn new(before: &Layout) -> Plan {
        Plan {
            ops: Vec::new(),
            changes: Vec::new(),
            after: before.clone(),
        }
    }

    fn change(&mut self, file: String, action: &str, detail: String, backup: Option<String>) {
        self.changes.push(Change {
            file,
            action: action.into(),
            detail,
            backup,
        });
    }

    fn write_config(&mut self, name: &str, text: String, detail: String) {
        let first_backup = format!("config/{name}.bak");
        let backup =
            (!self.after.other_files.contains(&first_backup)).then(|| first_backup.clone());
        self.after.other_files.insert(first_backup);
        self.after.configs.insert(name.to_string(), text.clone());
        self.ops.push(Op::WriteConfig {
            name: name.to_string(),
            text,
        });
        self.change(format!("config/{name}"), "edit", detail, backup);
    }

    fn create_config(&mut self, name: &str, text: String, detail: String) {
        self.after.configs.insert(name.to_string(), text.clone());
        self.ops.push(Op::CreateConfig {
            name: name.to_string(),
            text,
        });
        self.change(format!("config/{name}"), "create", detail, None);
    }

    fn remove_config(&mut self, name: &str, detail: String) {
        let backup = free_name(&self.after, "config", name, ".deleted");
        self.after.configs.remove(name);
        self.after.other_files.insert(format!("config/{backup}"));
        self.ops.push(Op::RemoveConfig {
            name: name.to_string(),
            backup: backup.clone(),
        });
        self.change(
            format!("config/{name}"),
            "delete",
            detail,
            Some(format!("config/{backup}")),
        );
    }

    fn rename_config(&mut self, from: &str, to: &str) {
        if let Some(t) = self.after.configs.remove(from) {
            self.after.configs.insert(to.to_string(), t);
        }
        self.ops.push(Op::RenameConfig {
            from: from.to_string(),
            to: to.to_string(),
        });
        self.change(
            format!("config/{from}"),
            "rename",
            format!("Renamed to config/{to} to keep matching the folder's name."),
            None,
        );
    }

    fn rename_match(&mut self, from: &str, to: &str) {
        self.after.match_files.remove(from);
        self.after.match_files.insert(to.to_string());
        if let Some(i) = self.after.imports.remove(from) {
            self.after.imports.insert(to.to_string(), i);
        }
        self.ops.push(Op::RenameMatch {
            from: from.to_string(),
            to: to.to_string(),
        });
        self.change(
            format!("match/{from}"),
            "rename",
            format!("Renamed to match/{to}."),
            None,
        );
    }

    /// Replaces the text of match file `name`, which must still be `original` when applied.
    fn write_match(&mut self, name: &str, text: String, original: &str, detail: String) {
        let first_backup = format!("match/{name}.bak");
        let backup =
            (!self.after.other_files.contains(&first_backup)).then(|| first_backup.clone());
        self.after.other_files.insert(first_backup);
        self.ops.push(Op::WriteMatch {
            name: name.to_string(),
            text,
            expect: content_hash(original.as_bytes()),
        });
        self.change(format!("match/{name}"), "edit", detail, backup);
    }

    fn create_match(&mut self, name: &str, text: String) {
        self.after.match_files.insert(name.to_string());
        self.ops.push(Op::CreateMatch {
            name: name.to_string(),
            text,
        });
        self.change(
            format!("match/{name}"),
            "create",
            "A new, empty folder.".into(),
            None,
        );
    }

    fn remove_match(&mut self, name: &str) {
        let backup = free_name(&self.after, "match", name, ".deleted");
        self.after.match_files.remove(name);
        self.after.imports.remove(name);
        self.after.other_files.insert(format!("match/{backup}"));
        self.ops.push(Op::RemoveMatch {
            name: name.to_string(),
            backup: backup.clone(),
        });
        self.change(
            format!("match/{name}"),
            "delete",
            "The folder and all its phrases are removed.".into(),
            Some(format!("match/{backup}")),
        );
    }
}

/// `<name>.deleted.bak`, or `<name>.deleted-2.bak`, ... whichever does not exist yet.
fn free_name(l: &Layout, dir: &str, name: &str, tag: &str) -> String {
    let taken = |n: &str| {
        let p = format!("{dir}/{n}");
        l.other_files.iter().any(|f| f.eq_ignore_ascii_case(&p))
    };
    let first = format!("{name}{tag}.bak");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|i| format!("{name}{tag}-{i}.bak"))
        .find(|n| !taken(n))
        .unwrap_or(first)
}

fn name_taken(l: &Layout, name: &str, except: Option<&str>) -> Option<String> {
    let want = name.to_lowercase();
    l.match_files
        .iter()
        .filter(|f| !f.contains('/') && Some(f.as_str()) != except)
        .find(|f| display_name(f).to_lowercase() == want)
        .cloned()
}

fn config_file_taken(l: &Layout, file: &str) -> bool {
    let lower = file.to_lowercase();
    l.configs
        .keys()
        .chain(l.unreadable.iter())
        .any(|c| c.to_lowercase() == lower)
}

fn ensure_known(l: &Layout, file: &str) -> Result<(), String> {
    if l.match_files.contains(file) {
        Ok(())
    } else {
        Err(format!("\"{file}\" is not a folder."))
    }
}

fn include_edit_text(r: &Ref) -> String {
    format!("`{}` in `{}`", r.entry, r.key)
}

/// Edits (and where a config ends up holding only filters, removes) the configs so that none
/// of `refs` remains. `rename_to` rewrites the entries to the new file name instead.
fn drop_or_rewrite_refs(
    plan: &mut Plan,
    before: &Layout,
    refs: &[Ref],
    file: &str,
    rename_to: Option<&str>,
) -> Result<(), String> {
    let mut by_config: BTreeMap<&str, Vec<&Ref>> = BTreeMap::new();
    for r in refs {
        by_config.entry(r.config.as_str()).or_default().push(r);
    }
    for (config, rs) in by_config {
        let text = before.configs.get(config).ok_or("config missing")?;
        let mut edits = Vec::new();
        for r in &rs {
            if let Some(new_file) = rename_to {
                let old_base = file.rsplit('/').next().unwrap_or(file);
                let cut = r
                    .entry
                    .strip_suffix(old_base)
                    .filter(|p| p.is_empty() || p.ends_with(['/', '\\']))
                    .ok_or_else(|| {
                        format!(
                            "config/{config} names the folder in a way xpando will not rewrite ({}).",
                            include_edit_text(r)
                        )
                    })?;
                edits.push(ConfigEdit::ReplaceEntry {
                    key: r.key.to_string(),
                    old: r.entry.clone(),
                    new: format!("{cut}{new_file}"),
                });
            } else {
                edits.push(ConfigEdit::RemoveEntry {
                    key: r.key.to_string(),
                    entry: r.entry.clone(),
                });
            }
        }
        let new_text = edit_config(text, &edits).map_err(|e| format!("config/{config}: {e}"))?;
        let is_default = default_name(before).as_deref() == Some(config);
        if rename_to.is_none() && !is_default && only_filters_left(&new_text) {
            plan.remove_config(
                config,
                "It only held this folder's app filters, so it is no longer needed.".into(),
            );
        } else if rename_to.is_some() {
            plan.write_config(
                config,
                new_text,
                "The folder's new file name is used here.".into(),
            );
        } else {
            plan.write_config(
                config,
                new_text,
                "The folder is no longer listed here; every other setting and comment stays."
                    .into(),
            );
        }
    }
    Ok(())
}

// ---- create -----------------------------------------------------------------------------

fn plan_create(l: &Layout, name: &str) -> Result<(Plan, String), String> {
    let name = validate_name(name)?;
    if let Some(f) = name_taken(l, &name, None) {
        return Err(format!(
            "A folder named \"{}\" already exists (match/{f}).",
            display_name(&f)
        ));
    }
    let file = format!("{name}.yml");
    if l.other_files.contains(&format!("match/{file}")) {
        return Err(format!("match/{file} already exists."));
    }
    let mut p = Plan::new(l);
    p.create_match(&file, "matches:\n".to_string());
    Ok((p, file))
}

// ---- rename -----------------------------------------------------------------------------

fn plan_rename(l: &Layout, file: &str, new_name: &str) -> Result<(Plan, String), String> {
    ensure_known(l, file)?;
    let a = analyze(l);
    if let Some(why) = lock_reason(l, &a, file) {
        return Err(format!("Cannot rename: {why}"));
    }
    let new_name = validate_name(new_name)?;
    let (stem, ext) = split_ext(file).ok_or("not a match file")?;
    let prefix = if stem.starts_with('_') { "_" } else { "" };
    let new_file = format!("{prefix}{new_name}{ext}");
    if new_file == file {
        return Err("That is already the folder's name.".into());
    }
    if let Some(f) = name_taken(l, &new_name, Some(file)) {
        return Err(format!(
            "A folder named \"{}\" already exists (match/{f}).",
            display_name(&f)
        ));
    }
    if l.other_files.contains(&format!("match/{new_file}")) {
        return Err(format!("match/{new_file} already exists."));
    }
    let before_scope = signature(&a.scope(file));
    let refs = refs_to(l, &a, file);
    let mut p = Plan::new(l);
    // Rewrite literal references; wildcard references need no edit.
    let literal: Vec<Ref> = refs.iter().filter(|r| r.literal).cloned().collect();
    drop_or_rewrite_refs(&mut p, l, &literal, file, Some(&new_file))?;
    p.rename_match(file, &new_file);
    // A config named after the folder and used only by it follows the new name.
    let old_display = display_name(file);
    let owners: BTreeSet<&str> = refs.iter().map(|r| r.config.as_str()).collect();
    if owners.len() == 1 {
        let c = *owners.iter().next().unwrap_or(&"");
        if let Some((cstem, cext)) = split_ext(c) {
            let is_default = default_name(l).as_deref() == Some(c);
            let solo = dedicated(l, &a, c, file);
            let target = format!("{new_name}{cext}");
            if !is_default
                && solo
                && cstem.to_lowercase() == old_display.to_lowercase()
                && !config_file_taken(l, &target)
            {
                p.rename_config(c, &target);
            }
        }
    }
    let after = analyze(&p.after);
    if signature(&after.scope(&new_file)) != before_scope {
        return Err(
            "After renaming, the folder would apply to different apps (a pattern in a config \
             no longer matches it). Nothing was changed."
                .into(),
        );
    }
    Ok((p, new_file))
}

/// True when config `c` mentions no other match file than `file` in its include lists.
fn dedicated(l: &Layout, a: &Analysis, c: &str, file: &str) -> bool {
    let Some(info) = a.configs.iter().find(|x| x.file == c) else {
        return false;
    };
    let Ok(own) = &info.own else { return false };
    INCLUDE_KEYS.iter().all(|k| {
        own.list(k)
            .iter()
            .all(|e| !has_meta(e) && entry_matches(e, file, &l.root) == Some(true))
    })
}

// ---- delete -----------------------------------------------------------------------------

fn plan_delete(l: &Layout, file: &str) -> Result<Plan, String> {
    ensure_known(l, file)?;
    let a = analyze(l);
    if let Some(why) = lock_reason(l, &a, file) {
        return Err(format!("Cannot delete: {why}"));
    }
    let refs: Vec<Ref> = refs_to(l, &a, file)
        .into_iter()
        .filter(|r| r.literal)
        .collect();
    let mut p = Plan::new(l);
    drop_or_rewrite_refs(&mut p, l, &refs, file, None)?;
    p.remove_match(file);
    Ok(p)
}

// ---- scope ------------------------------------------------------------------------------

/// The `filter_exec` regular expression for a list of executable names.
pub fn exec_pattern(names: &[String]) -> Result<String, String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut alts = Vec::new();
    for raw in names {
        let mut n = raw.trim();
        if n.to_lowercase().ends_with(".exe") {
            n = &n[..n.len() - 4];
        }
        let n = n.trim();
        if n.is_empty() {
            return Err("An app name is empty.".into());
        }
        if n.chars().count() > 64 {
            return Err(format!("\"{n}\" is too long for an app name."));
        }
        if let Some(bad) = n.chars().find(|c| {
            !(c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | '+' | '(' | ')'))
        }) {
            return Err(format!(
                "\"{n}\" contains \"{bad}\". Type only the program's file name, such as winword."
            ));
        }
        if seen.insert(n.to_lowercase()) {
            alts.push(regex_escape(n));
        }
    }
    if alts.is_empty() {
        return Err("Add at least one app.".into());
    }
    Ok(format!(r"(?i)(^|[\\/])({})(\.exe)?$", alts.join("|")))
}

/// The `filter_title` regular expression for "title contains `text`".
pub fn title_pattern(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("The window title text is empty.".into());
    }
    if t.chars().count() > 120 || t.chars().any(char::is_control) {
        return Err("The window title text is too long or has control characters.".into());
    }
    Ok(format!("(?i){}", regex_escape(t)))
}

fn config_text(file: &str, name: &str, exec: &Option<String>, title: &Option<String>) -> String {
    let mut s = format!("# Apps for the \"{name}\" folder (match/{file}). Written by xpando.\n");
    if let Some(e) = exec {
        s.push_str(&format!("filter_exec: {}\n", sq(e)));
    }
    if let Some(t) = title {
        s.push_str(&format!("filter_title: {}\n", sq(t)));
    }
    s.push_str(&format!(
        "extra_includes:\n  - {}\n",
        dq(&format!("../match/{file}"))
    ));
    s
}

fn plan_scope(l: &Layout, file: &str, spec: &ScopeSpec) -> Result<(Plan, String), String> {
    ensure_known(l, file)?;
    let a = analyze(l);
    if let Some(why) = scope_lock_reason(l, &a, file) {
        return Err(format!("Cannot change the apps: {why}"));
    }
    let (stem, ext) = split_ext(file).ok_or("not a match file")?;
    let name = display_name(file);
    let refs = refs_to(l, &a, file);
    let mut p = Plan::new(l);
    let current_file;
    match spec {
        ScopeSpec::All => {
            if a.scope(file).kind == ScopeKind::All {
                return Err("This folder already applies to all apps.".into());
            }
            drop_or_rewrite_refs(&mut p, l, &refs, file, None)?;
            current_file = if stem.starts_with('_') {
                let new_file = format!("{name}{ext}");
                if let Some(f) = name_taken(l, &name, Some(file)) {
                    return Err(format!(
                        "Cannot rename match/{file} to match/{new_file}: match/{f} already exists."
                    ));
                }
                if l.other_files.contains(&format!("match/{new_file}")) {
                    return Err(format!("match/{new_file} already exists."));
                }
                p.rename_match(file, &new_file);
                new_file
            } else {
                file.to_string()
            };
            let after = analyze(&p.after);
            if after.scope(&current_file).kind != ScopeKind::All {
                return Err(
                    "Even after removing it from the configs, espanso would not load this \
                     folder for every app (check use_standard_includes and the exclude lists \
                     in your configs). Nothing was changed."
                        .into(),
                );
            }
        }
        ScopeSpec::Apps { exes, title } => {
            let in_place = match refs.as_slice() {
                [r] if dedicated(l, &a, &r.config, file) => Some(r.config.clone()),
                _ => None,
            };
            let own = in_place
                .as_ref()
                .and_then(|c| a.configs.iter().find(|x| &x.file == c))
                .and_then(|c| c.own.as_ref().ok().cloned());
            let exec = match exes {
                Some(v) if v.is_empty() => None,
                Some(v) => Some(exec_pattern(v)?),
                None => own.as_ref().and_then(|o| o.filter_exec.clone()),
            };
            let title_new = match title {
                TitleChange::Keep => own.as_ref().and_then(|o| o.filter_title.clone()),
                TitleChange::Clear => None,
                TitleChange::Contains(t) => Some(title_pattern(t)?),
            };
            if exec.is_none() && title_new.is_none() {
                return Err("Choose at least one app or a window title.".into());
            }
            // The file must start with an underscore so the standard includes skip it.
            let target_file = if stem.starts_with('_') {
                file.to_string()
            } else {
                let new_file = format!("_{stem}{ext}");
                if l.match_files.contains(&new_file)
                    || l.other_files.contains(&format!("match/{new_file}"))
                {
                    return Err(format!(
                        "Cannot rename match/{file} to match/{new_file}: that file already exists."
                    ));
                }
                new_file
            };
            let renamed = target_file != file;
            match &in_place {
                Some(c) => {
                    let text = l.configs.get(c).ok_or("config missing")?;
                    let mut edits = Vec::new();
                    let (cur_exec, cur_title) = own.as_ref().map_or((None, None), |o| {
                        (o.filter_exec.clone(), o.filter_title.clone())
                    });
                    if exec != cur_exec {
                        edits.push(ConfigEdit::SetText {
                            key: "filter_exec".into(),
                            value: exec.clone(),
                        });
                    }
                    if title_new != cur_title {
                        edits.push(ConfigEdit::SetText {
                            key: "filter_title".into(),
                            value: title_new.clone(),
                        });
                    }
                    if renamed {
                        let r = &refs[0];
                        let old_base = file;
                        let cut = r
                            .entry
                            .strip_suffix(old_base)
                            .filter(|x| x.is_empty() || x.ends_with(['/', '\\']))
                            .ok_or_else(|| {
                                format!(
                                    "config/{c} names the folder in a way xpando will not rewrite ({}).",
                                    include_edit_text(r)
                                )
                            })?;
                        edits.push(ConfigEdit::ReplaceEntry {
                            key: r.key.to_string(),
                            old: r.entry.clone(),
                            new: format!("{cut}{target_file}"),
                        });
                    }
                    if !edits.is_empty() {
                        let new_text =
                            edit_config(text, &edits).map_err(|e| format!("config/{c}: {e}"))?;
                        p.write_config(
                            c,
                            new_text,
                            "The app filters were changed; every comment and other setting stays."
                                .into(),
                        );
                    }
                }
                None => {
                    let cfg = format!("{name}{}", if ext == ".yaml" { ".yaml" } else { ".yml" });
                    drop_or_rewrite_refs(&mut p, l, &refs, file, None)?;
                    if config_file_taken(&p.after, &cfg) {
                        return Err(format!(
                            "config/{cfg} already exists and does not belong to this folder. \
                             Rename that file or this folder first."
                        ));
                    }
                    p.create_config(
                        &cfg,
                        config_text(&target_file, &name, &exec, &title_new),
                        "New app-specific config that loads only this folder.".into(),
                    );
                }
            }
            if renamed {
                p.rename_match(file, &target_file);
            }
            current_file = target_file;
            let after = analyze(&p.after);
            let sc = after.scope(&current_file);
            let want = [exec.clone(), title_new.clone()];
            let ok = sc.kind == ScopeKind::Only
                && sc.apps.len() == 1
                && sc.apps[0].exec_pattern == want[0]
                && sc.apps[0].title_pattern == want[1];
            if !ok {
                return Err(
                    "After the change espanso would not apply this folder to exactly the chosen \
                     apps (another setting in your configs interferes). Nothing was changed."
                        .into(),
                );
            }
        }
    }
    Ok((p, current_file))
}

// ---------------------------------------------------------------------------------------
// Applying a plan to disk
// ---------------------------------------------------------------------------------------

enum Undo {
    Restore(PathBuf, Vec<u8>),
    Remove(PathBuf),
    Rename(PathBuf, PathBuf),
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    let r = fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()
    });
    if let Err(e) = r.and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}

fn rename_checked(from: &Path, to: &Path) -> io::Result<()> {
    let same_file = from.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase();
    if to.exists() && !same_file {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    fs::rename(from, to)
}

fn run_op(dir: &Path, before: &Layout, op: &Op, undo: &mut Vec<Undo>) -> Result<(), String> {
    let cfg = |n: &str| dir.join("config").join(n);
    let mat = |n: &str| dir.join("match").join(n);
    let io = |e: io::Error| e.to_string();
    match op {
        Op::WriteConfig { name, text } => {
            let path = cfg(name);
            let original = before.configs.get(name).ok_or("config missing")?;
            let bytes = fs::read(&path).map_err(io)?;
            undo.push(Undo::Restore(path.clone(), bytes));
            save_file(&path, text, &content_hash(original.as_bytes())).map_err(|e| match e {
                SaveError::Conflict => format!(
                    "{CONFLICT_PREFIX} config/{name} was changed outside xpando. Reload and try again."
                ),
                e => e.to_string(),
            })?;
        }
        Op::CreateConfig { name, text } => {
            create_new(&cfg(name), text.as_bytes()).map_err(io)?;
            undo.push(Undo::Remove(cfg(name)));
        }
        Op::RemoveConfig { name, backup } => {
            let bytes = fs::read(cfg(name)).map_err(io)?;
            create_new(&cfg(backup), &bytes).map_err(io)?;
            undo.push(Undo::Remove(cfg(backup)));
            fs::remove_file(cfg(name)).map_err(io)?;
            undo.push(Undo::Restore(cfg(name), bytes));
        }
        Op::RenameConfig { from, to } => {
            rename_checked(&cfg(from), &cfg(to)).map_err(io)?;
            undo.push(Undo::Rename(cfg(to), cfg(from)));
        }
        Op::CreateMatch { name, text } => {
            create_new(&mat(name), text.as_bytes()).map_err(io)?;
            undo.push(Undo::Remove(mat(name)));
        }
        Op::RemoveMatch { name, backup } => {
            let bytes = fs::read(mat(name)).map_err(io)?;
            create_new(&mat(backup), &bytes).map_err(io)?;
            undo.push(Undo::Remove(mat(backup)));
            fs::remove_file(mat(name)).map_err(io)?;
            undo.push(Undo::Restore(mat(name), bytes));
        }
        Op::WriteMatch { name, text, expect } => {
            let path = mat(name);
            let bytes = fs::read(&path).map_err(io)?;
            undo.push(Undo::Restore(path.clone(), bytes));
            save_file(&path, text, expect).map_err(|e| match e {
                SaveError::Conflict => format!(
                    "{CONFLICT_PREFIX} match/{name} was changed outside xpando. Reload and try again."
                ),
                e => e.to_string(),
            })?;
        }
        Op::RenameMatch { from, to } => {
            rename_checked(&mat(from), &mat(to)).map_err(io)?;
            undo.push(Undo::Rename(mat(to), mat(from)));
        }
    }
    Ok(())
}

fn rollback(undo: Vec<Undo>) -> Vec<String> {
    let mut failed = Vec::new();
    for u in undo.into_iter().rev() {
        let r = match &u {
            Undo::Restore(p, b) => atomic_write(p, b),
            Undo::Remove(p) => fs::remove_file(p),
            Undo::Rename(a, b) => fs::rename(a, b),
        };
        if let Err(e) = r {
            failed.push(e.to_string());
        }
    }
    failed
}

/// Which files differ between what a plan predicted and what is on disk, for the error message.
fn describe_layout_diff(want: &Layout, got: &Layout) -> String {
    let mut parts: Vec<String> = Vec::new();
    for f in want.match_files.difference(&got.match_files) {
        parts.push(format!("match/{f} is missing"));
    }
    for f in got.match_files.difference(&want.match_files) {
        parts.push(format!("match/{f} appeared"));
    }
    for f in want.configs.keys().chain(got.configs.keys()) {
        match (want.configs.get(f), got.configs.get(f)) {
            (Some(a), Some(b)) if a != b => parts.push(format!("config/{f} differs")),
            (Some(_), None) => parts.push(format!("config/{f} is missing")),
            (None, Some(_)) => parts.push(format!("config/{f} appeared")),
            _ => {}
        }
    }
    for f in want.unreadable.symmetric_difference(&got.unreadable) {
        parts.push(format!("config/{f} could not be read"));
    }
    if want.imports != got.imports {
        parts.push("the imports of a match file differ".to_string());
    }
    parts.sort();
    parts.dedup();
    if parts.is_empty() {
        "something else changed".to_string()
    } else {
        parts.join(", ")
    }
}

/// Runs every step of `plan`. If a step fails, or the disk afterwards is not what the plan
/// predicted, every step is undone (the one-time `.bak` copies stay).
fn execute(dir: &Path, before: &Layout, plan: &Plan) -> Result<(), String> {
    let mut undo = Vec::new();
    let mut result = Ok(());
    for op in &plan.ops {
        if let Err(e) = run_op(dir, before, op, &mut undo) {
            result = Err(e);
            break;
        }
    }
    if result.is_ok() {
        // The disk must now look exactly like the layout that was checked.
        match Layout::from_disk(dir) {
            Ok(now) => {
                let mut want = plan.after.clone();
                let mut got = now;
                want.other_files.clear();
                got.other_files.clear();
                want.root.clear();
                got.root.clear();
                if want != got {
                    result = Err(format!(
                        "The files on disk changed while saving ({}).",
                        describe_layout_diff(&want, &got)
                    ));
                }
            }
            Err(e) => result = Err(e),
        }
    }
    if let Err(e) = result {
        let failed = rollback(undo);
        return Err(if failed.is_empty() {
            format!("{e} Nothing was changed.")
        } else {
            format!(
                "{e} Some files could not be put back ({}). Check the .bak files next to them.",
                failed.join("; ")
            )
        });
    }
    Ok(())
}

fn check_hash(dir: &Path, file: &str, expected: &str) -> Result<(), String> {
    if expected.is_empty() {
        return Ok(());
    }
    let bytes = fs::read(dir.join("match").join(file))
        .map_err(|e| format!("Could not read match/{file}: {e}"))?;
    if content_hash(&bytes) != expected {
        return Err(SaveError::Conflict.to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------------------

/// Creates `match/<name>.yml` with an empty `matches:` list.
pub fn create_folder(config_dir: &Path, name: &str) -> Result<Outcome, String> {
    let l = Layout::from_disk(config_dir)?;
    if !config_dir.join("match").is_dir() {
        return Err("The espanso config folder has no match directory.".into());
    }
    let (plan, file) = plan_create(&l, name)?;
    execute(config_dir, &l, &plan)?;
    Ok(Outcome {
        changes: plan.changes,
        file: Some(file),
    })
}

pub fn preview_rename(
    config_dir: &Path,
    file: &str,
    new_name: &str,
) -> Result<Vec<Change>, String> {
    let l = Layout::from_disk(config_dir)?;
    Ok(plan_rename(&l, file, new_name)?.0.changes)
}

pub fn rename_folder(
    config_dir: &Path,
    file: &str,
    new_name: &str,
    expected_hash: &str,
) -> Result<Outcome, String> {
    let l = Layout::from_disk(config_dir)?;
    let (plan, new_file) = plan_rename(&l, file, new_name)?;
    check_hash(config_dir, file, expected_hash)?;
    execute(config_dir, &l, &plan)?;
    Ok(Outcome {
        changes: plan.changes,
        file: Some(new_file),
    })
}

pub fn preview_delete(config_dir: &Path, file: &str) -> Result<Vec<Change>, String> {
    let l = Layout::from_disk(config_dir)?;
    Ok(plan_delete(&l, file)?.changes)
}

/// Deletes a folder. Does nothing unless `confirm` is true.
pub fn delete_folder(
    config_dir: &Path,
    file: &str,
    confirm: bool,
    expected_hash: &str,
) -> Result<Outcome, String> {
    if !confirm {
        return Err("Deleting a folder needs confirmation.".into());
    }
    let l = Layout::from_disk(config_dir)?;
    let plan = plan_delete(&l, file)?;
    check_hash(config_dir, file, expected_hash)?;
    execute(config_dir, &l, &plan)?;
    Ok(Outcome {
        changes: plan.changes,
        file: None,
    })
}

pub fn preview_scope(
    config_dir: &Path,
    file: &str,
    spec: &ScopeSpec,
) -> Result<Vec<Change>, String> {
    let l = Layout::from_disk(config_dir)?;
    Ok(plan_scope(&l, file, spec)?.0.changes)
}

pub fn set_scope(
    config_dir: &Path,
    file: &str,
    spec: &ScopeSpec,
    expected_hash: &str,
) -> Result<Outcome, String> {
    let l = Layout::from_disk(config_dir)?;
    let (plan, new_file) = plan_scope(&l, file, spec)?;
    check_hash(config_dir, file, expected_hash)?;
    execute(config_dir, &l, &plan)?;
    Ok(Outcome {
        changes: plan.changes,
        file: Some(new_file),
    })
}

mod phrases;
pub use phrases::{
    auto_folder_name, exe_key, preview_phrase_where, set_phrase_where, MoveConflict, MoveOutcome,
    MovePreview, PhraseWhere,
};

#[cfg(test)]
mod tests;
