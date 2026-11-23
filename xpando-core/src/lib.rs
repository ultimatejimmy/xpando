//! Core logic for xpando: discovers espanso match files and reads phrases
//! (read-only) while keeping the YAML files as the single source of truth.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_norway::Value;

/// Returns every `*.yml` / `*.yaml` match file under `<config_dir>/match`,
/// recursively, sorted by path. A missing `match` directory yields an empty list.
pub fn discover_match_files(config_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let root = config_dir.join("match");
    let mut files = Vec::new();
    if root.is_dir() {
        collect(&root, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        ) {
            out.push(path);
        }
    }
    Ok(())
}

/// One `vars:` entry: a dynamic value that `{{name}}` placeholders refer to
/// (date, clipboard, shell, echo, choice, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Var {
    pub name: String,
    /// The variable type, e.g. `date`, `clipboard`, `shell`, `echo`.
    pub kind: String,
    /// `params:` as ordered key/value pairs; nested values are rendered as YAML text.
    pub params: Vec<(String, String)>,
}

/// One phrase (a single entry of `matches:`), read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Phrase {
    /// Trigger strings; for a regex match this holds the pattern.
    pub triggers: Vec<String>,
    pub is_regex: bool,
    pub label: Option<String>,
    /// Which body field was used: `replace`, `html`, `markdown`, `image_path` or `form`.
    pub kind: String,
    /// Full body text.
    pub content: String,
    /// Plain-text preview: whitespace collapsed, about 120 characters.
    pub preview: String,
    /// Variables the body's `{{placeholders}}` are built from, in file order.
    pub vars: Vec<Var>,
}

/// A folder is one match file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub name: String,
    /// Path relative to `<config_dir>/match`, with `/` separators.
    pub file: String,
    pub phrases: Vec<Phrase>,
    pub warnings: Vec<String>,
}

const PREVIEW_CHARS: usize = 120;

fn make_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > PREVIEW_CHARS {
        let cut: String = collapsed.chars().take(PREVIEW_CHARS).collect();
        format!("{}...", cut.trim_end())
    } else {
        collapsed
    }
}

fn folder_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let name = stem.trim_start_matches('_');
    if name.is_empty() {
        stem.to_string()
    } else {
        name.to_string()
    }
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn parse_entry(entry: &Value) -> Result<Phrase, String> {
    let map = entry.as_mapping().ok_or("entry is not a mapping")?;
    let get = |k: &str| map.get(k);

    let mut triggers = Vec::new();
    let mut is_regex = false;
    if let Some(t) = get("trigger") {
        triggers.push(scalar_string(t).ok_or("`trigger` must be a string")?);
    }
    if let Some(ts) = get("triggers") {
        let seq = ts.as_sequence().ok_or("`triggers` must be a list")?;
        for t in seq {
            triggers.push(scalar_string(t).ok_or("`triggers` must contain strings")?);
        }
    }
    if triggers.is_empty() {
        if let Some(r) = get("regex") {
            triggers.push(scalar_string(r).ok_or("`regex` must be a string")?);
            is_regex = true;
        }
    }
    if triggers.is_empty() {
        return Err("no trigger, triggers or regex".into());
    }

    let label = get("label").and_then(scalar_string);

    let mut body = None;
    for kind in ["replace", "html", "markdown", "image_path", "form"] {
        if let Some(v) = get(kind) {
            let text = scalar_string(v).ok_or_else(|| format!("`{kind}` must be a string"))?;
            body = Some((kind, text));
            break;
        }
    }
    let (kind, content) =
        body.ok_or("no replace, html, markdown, image_path or form")?;
    let content = content.trim_end_matches('\n').to_string();
    Ok(Phrase {
        triggers,
        is_regex,
        label,
        kind: kind.to_string(),
        preview: make_preview(&content),
        content,
        vars: get("vars").map(parse_vars).unwrap_or_default(),
    })
}

/// Text form of a YAML value: scalars as-is, lists and maps as trimmed YAML.
fn value_text(v: &Value) -> String {
    scalar_string(v)
        .unwrap_or_else(|| serde_norway::to_string(v).unwrap_or_default().trim().to_string())
}

/// Reads a `vars:` list. Lenient: items that are not mappings or have no name are skipped.
fn parse_vars(vars: &Value) -> Vec<Var> {
    let Some(seq) = vars.as_sequence() else {
        return Vec::new();
    };
    seq.iter()
        .filter_map(|item| {
            let map = item.as_mapping()?;
            let name = map.get("name").and_then(scalar_string)?;
            let kind = map.get("type").and_then(scalar_string).unwrap_or_default();
            let params = map
                .get("params")
                .and_then(Value::as_mapping)
                .map(|p| {
                    p.iter()
                        .filter_map(|(k, v)| Some((scalar_string(k)?, value_text(v))))
                        .collect()
                })
                .unwrap_or_default();
            Some(Var { name, kind, params })
        })
        .collect()
}

/// Parses the text of one match file. Unparseable entries are skipped and
/// described in the returned warnings; a file that is not valid YAML yields
/// no phrases and one warning.
pub fn parse_match_text(text: &str) -> (Vec<Phrase>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut phrases = Vec::new();
    let doc: Value = match serde_norway::from_str(text) {
        Ok(v) => v,
        Err(e) => return (phrases, vec![format!("invalid YAML: {e}")]),
    };
    let entries = match &doc {
        Value::Null => return (phrases, warnings),
        Value::Mapping(m) => match m.get("matches") {
            None | Some(Value::Null) => return (phrases, warnings),
            Some(Value::Sequence(s)) => s,
            Some(_) => return (phrases, vec!["`matches` is not a list".into()]),
        },
        _ => return (phrases, vec!["file is not a YAML mapping".into()]),
    };
    for (i, entry) in entries.iter().enumerate() {
        match parse_entry(entry) {
            Ok(p) => phrases.push(p),
            Err(e) => warnings.push(format!("match #{}: {e}", i + 1)),
        }
    }
    (phrases, warnings)
}

/// Reads every match file under `<config_dir>/match` into folders. Read-only.
pub fn load_folders(config_dir: &Path) -> io::Result<Vec<Folder>> {
    let root = config_dir.join("match");
    let mut folders = Vec::new();
    for path in discover_match_files(config_dir)? {
        let file = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let (phrases, warnings) = match fs::read_to_string(&path) {
            Ok(text) => parse_match_text(&text),
            Err(e) => (Vec::new(), vec![format!("cannot read file: {e}")]),
        };
        folders.push(Folder {
            name: folder_name(&path),
            file,
            phrases,
            warnings,
        });
    }
    Ok(folders)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xpando-core-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_nested_yaml_files_only() {
        let dir = temp_dir("nested");
        fs::create_dir_all(dir.join("match/work")).unwrap();
        fs::write(dir.join("match/base.yml"), "").unwrap();
        fs::write(dir.join("match/work/sales.yaml"), "").unwrap();
        fs::write(dir.join("match/notes.txt"), "").unwrap();

        let files = discover_match_files(&dir).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(names, ["match/base.yml", "match/work/sales.yaml"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_match_dir_is_empty() {
        let dir = temp_dir("missing");
        assert!(discover_match_files(&dir).unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_and_multiple_triggers() {
        let (p, w) = parse_match_text(
            "matches:\n  - trigger: \":hi\"\n    replace: \"Hello\"\n    label: Greeting\n  - triggers: [\":a\", \":b\"]\n    replace: X\n",
        );
        assert!(w.is_empty());
        assert_eq!(p[0].triggers, [":hi"]);
        assert_eq!(p[0].label.as_deref(), Some("Greeting"));
        assert_eq!(p[1].triggers, [":a", ":b"]);
    }

    #[test]
    fn regex_match() {
        let (p, _) = parse_match_text("matches:\n  - regex: ':h(\\d+)'\n    replace: x\n");
        assert!(p[0].is_regex);
        assert_eq!(p[0].triggers, [":h(\\d+)"]);
    }

    #[test]
    fn multiline_preview_collapses_and_truncates() {
        let long = "word ".repeat(50);
        let text = format!(
            "matches:\n  - trigger: t\n    replace: |\n      line one\n      line two\n  - trigger: l\n    replace: \"{long}\"\n"
        );
        let (p, _) = parse_match_text(&text);
        assert_eq!(p[0].content, "line one\nline two");
        assert_eq!(p[0].preview, "line one line two");
        assert!(p[1].preview.ends_with("..."));
        assert!(p[1].preview.chars().count() <= PREVIEW_CHARS + 3);
    }

    #[test]
    fn html_markdown_image_form_and_vars() {
        let (p, w) = parse_match_text(
            "matches:\n  - trigger: h\n    html: \"<b>x</b>\"\n  - trigger: m\n    markdown: \"**x**\"\n  - trigger: i\n    image_path: /a/b.png\n  - trigger: f\n    form: \"Hi [[name]]\"\n    form_fields:\n      name:\n        type: text\n  - trigger: v\n    replace: \"{{d}}\"\n    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y\"\n",
        );
        assert!(w.is_empty());
        let kinds: Vec<_> = p.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, ["html", "markdown", "image_path", "form", "replace"]);
    }

    #[test]
    fn malformed_entry_is_skipped_with_warning() {
        let (p, w) = parse_match_text(
            "matches:\n  - trigger: ok\n    replace: fine\n  - replace: no trigger\n  - just a string\n  - trigger: nobody\n",
        );
        assert_eq!(p.len(), 1);
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn vars_are_parsed_in_order_with_params() {
        let (p, w) = parse_match_text(
            "matches:\n  - trigger: TDC\n    replace: \"{{out}}\"\n    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y%m%d\"\n      - name: clip\n        type: clipboard\n      - name: out\n        type: echo\n        params:\n          echo: \"{{d}}_{{clip}}\"\n      - name: s\n        type: shell\n        params:\n          shell: powershell\n          cmd: run it\n  - trigger: plain\n    replace: hi\n",
        );
        assert!(w.is_empty());
        let names: Vec<_> = p[0].vars.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["d", "clip", "out", "s"]);
        assert_eq!(p[0].vars[0].kind, "date");
        assert_eq!(p[0].vars[0].params, [("format".to_string(), "%Y%m%d".to_string())]);
        assert!(p[0].vars[1].params.is_empty());
        assert_eq!(p[0].vars[2].params[0], ("echo".to_string(), "{{d}}_{{clip}}".to_string()));
        // Param order follows the file.
        assert_eq!(p[0].vars[3].params[0].0, "shell");
        assert_eq!(p[0].vars[3].params[1].0, "cmd");
        assert!(p[1].vars.is_empty());
    }

    #[test]
    fn nested_param_values_and_bad_vars_are_tolerated() {
        let (p, w) = parse_match_text(
            "matches:\n  - trigger: c\n    replace: \"{{x}}\"\n    vars:\n      - name: x\n        type: choice\n        params:\n          values: [a, b]\n      - just a string\n      - type: date\n",
        );
        assert!(w.is_empty());
        assert_eq!(p[0].vars.len(), 1);
        assert_eq!(p[0].vars[0].params[0].0, "values");
        assert!(p[0].vars[0].params[0].1.contains("- a"));
    }

    #[test]
    fn empty_and_invalid_files() {
        assert_eq!(parse_match_text(""), (vec![], vec![]));
        assert!(parse_match_text("matches: []").1.is_empty());
        let (p, w) = parse_match_text("matches: [unclosed");
        assert!(p.is_empty());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn loads_folders_with_names() {
        let dir = temp_dir("folders");
        fs::create_dir_all(dir.join("match")).unwrap();
        fs::write(dir.join("match/_base.yml"), "matches:\n  - trigger: a\n    replace: b\n").unwrap();
        fs::write(dir.join("match/bad.yml"), "matches: [x").unwrap();
        let f = load_folders(&dir).unwrap();
        assert_eq!(f[0].name, "base");
        assert_eq!(f[1].name, "bad");
        assert_eq!(f[0].phrases.len(), 1);
        assert_eq!(f[1].warnings.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
