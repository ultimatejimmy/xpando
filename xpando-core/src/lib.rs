//! Core logic for xpando: discovers espanso match files and reads phrases
//! (read-only) while keeping the YAML files as the single source of truth.

pub mod advanced;
pub mod conflicts;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_norway::Value;

pub mod edit;
pub mod vars;

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
    /// The `# comment` lines directly above the entry, joined with newlines.
    pub comment: Option<String>,
    /// Position of this entry in the file's `matches:` list, counting entries that were
    /// skipped as unparseable. Use it, not the list position, to address the entry.
    pub index: usize,
    /// 1-based line of the entry's `-` in the file, when the line scan was reliable.
    pub line: Option<usize>,
}

/// A folder is one match file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub name: String,
    /// Path relative to `<config_dir>/match`, with `/` separators.
    pub file: String,
    pub phrases: Vec<Phrase>,
    pub warnings: Vec<String>,
    /// Hash of the file's text as loaded; pass it back to `edit::save_file` to detect changes.
    pub hash: String,
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
    let (kind, content) = body.ok_or("no replace, html, markdown, image_path or form")?;
    let content = content.trim_end_matches('\n').to_string();
    Ok(Phrase {
        triggers,
        is_regex,
        label,
        kind: kind.to_string(),
        preview: make_preview(&content),
        content,
        vars: get("vars").map(parse_vars).unwrap_or_default(),
        comment: None,
        index: 0,
        line: None,
    })
}

/// Where one entry of `matches:` sits in the raw file text. All line numbers are 0-based
/// indexes into `text.lines()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySpan {
    /// First line of the entry's own comment block, or `start_line` when it has none.
    pub first_line: usize,
    /// The line holding the entry's `-`.
    pub start_line: usize,
    /// One past the entry's last line: the next entry's `first_line`, or the end of the
    /// `matches:` block for the last entry.
    pub end_line: usize,
    /// The `# comment` lines directly above the entry, joined with newlines.
    pub comment: Option<String>,
}

/// Locates every entry of `matches:` in the raw text of a match file.
///
/// YAML parsers drop comments and positions, so this scans the raw lines. It returns
/// `None` when it cannot find a plain block list (flow-style lists, no `matches:` key).
/// Callers must compare `len()` with the parsed entry count before trusting it, so a
/// comment or edit is never applied to the wrong phrase.
pub fn scan_entries(text: &str) -> Option<Vec<EntrySpan>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.trim_end() == "matches:")? + 1;
    let indent_of = |l: &str| l.len() - l.trim_start().len();
    let is_entry = |l: &str| l.trim_start() == "-" || l.trim_start().starts_with("- ");

    let mut entry_indent = None;
    let mut starts = Vec::new();
    let mut block_end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 && !t.starts_with('-') {
            block_end = i; // next top-level key
            break;
        }
        let base = *entry_indent.get_or_insert(indent);
        if indent == base && is_entry(line) {
            starts.push(i);
        }
    }

    let mut spans: Vec<EntrySpan> = starts
        .iter()
        .map(|&s| {
            let mut block = Vec::new();
            let mut j = s;
            let entry_indent = indent_of(lines[s]);
            // Only comments at or left of the entry's own indent describe it; a deeper
            // comment sits inside the previous entry's body.
            while j > 0
                && lines[j - 1].trim_start().starts_with('#')
                && indent_of(lines[j - 1]) <= entry_indent
            {
                j -= 1;
                let c = lines[j].trim_start().trim_start_matches('#');
                block.push(c.strip_prefix(' ').unwrap_or(c).trim_end().to_string());
            }
            block.reverse();
            EntrySpan {
                first_line: j,
                start_line: s,
                end_line: 0,
                comment: (!block.is_empty()).then(|| block.join("\n")),
            }
        })
        .collect();
    for i in 0..spans.len() {
        spans[i].end_line = spans.get(i + 1).map_or(block_end, |n| n.first_line);
    }
    Some(spans)
}

/// Text form of a YAML value: scalars as-is, lists and maps as trimmed YAML.
fn value_text(v: &Value) -> String {
    scalar_string(v).unwrap_or_else(|| {
        serde_norway::to_string(v)
            .unwrap_or_default()
            .trim()
            .to_string()
    })
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
    // Only trust the line scan when it found exactly as many entries as the YAML parser.
    let spans = scan_entries(text)
        .filter(|s| s.len() == entries.len())
        .unwrap_or_default();
    for (i, entry) in entries.iter().enumerate() {
        match parse_entry(entry) {
            Ok(mut p) => {
                p.index = i;
                if let Some(span) = spans.get(i) {
                    p.line = Some(span.start_line + 1);
                    p.comment = span.comment.clone();
                }
                phrases.push(p);
            }
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
        let (phrases, warnings, hash) = match fs::read_to_string(&path) {
            Ok(text) => {
                let (p, w) = parse_match_text(&text);
                (p, w, edit::content_hash(text.as_bytes()))
            }
            Err(e) => (
                Vec::new(),
                vec![format!("cannot read file: {e}")],
                String::new(),
            ),
        };
        folders.push(Folder {
            name: folder_name(&path),
            file,
            phrases,
            warnings,
            hash,
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
            .map(|p| {
                p.strip_prefix(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
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
        assert_eq!(
            p[0].vars[0].params,
            [("format".to_string(), "%Y%m%d".to_string())]
        );
        assert!(p[0].vars[1].params.is_empty());
        assert_eq!(
            p[0].vars[2].params[0],
            ("echo".to_string(), "{{d}}_{{clip}}".to_string())
        );
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
    fn comments_above_entries_become_descriptions() {
        let text = "# file header\n\nmatches:\n  # report end year\n  - trigger: a\n    replace: x\n\n  # not attached: blank line below\n\n  - trigger: b\n    replace: y\n  # line one\n  # line two\n  - trigger: c\n    replace: z\n    # inner comment, not a description\n  - trigger: d\n    replace: w\n";
        let (p, w) = parse_match_text(text);
        assert!(w.is_empty());
        assert_eq!(p[0].comment.as_deref(), Some("report end year"));
        assert_eq!(p[1].comment, None);
        assert_eq!(p[2].comment.as_deref(), Some("line one\nline two"));
        assert_eq!(p[3].comment, None);
    }

    #[test]
    fn comments_stay_aligned_when_an_entry_is_skipped() {
        let text = "matches:\n  # first\n  - trigger: a\n    replace: x\n  # broken\n  - just text\n  # third\n  - trigger: c\n    replace: z\n";
        let (p, w) = parse_match_text(text);
        assert_eq!(w.len(), 1);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].comment.as_deref(), Some("first"));
        assert_eq!(p[1].comment.as_deref(), Some("third"));
    }

    #[test]
    fn entries_know_their_index_line_and_span() {
        // Entry 1 is unparseable and skipped, so list position and entry index differ.
        let text = "matches:\n  # first\n  - trigger: a\n    replace: x\n  - just text\n\n  - trigger: c\n    replace: z\nglobal_vars: []\n";
        let (p, w) = parse_match_text(text);
        assert_eq!(w.len(), 1);
        assert_eq!((p[0].index, p[0].line), (0, Some(3)));
        assert_eq!((p[1].index, p[1].line), (2, Some(7)));

        let spans = scan_entries(text).unwrap();
        assert_eq!(spans.len(), 3);
        // Entry 0 spans its comment line through the line before entry 1.
        assert_eq!(
            (spans[0].first_line, spans[0].start_line, spans[0].end_line),
            (1, 2, 4)
        );
        // The last entry ends where the next top-level key starts.
        assert_eq!(spans[2].end_line, 8);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[spans[2].end_line], "global_vars: []");
    }

    #[test]
    fn unusual_layout_gets_no_comments_rather_than_wrong_ones() {
        // Flow-style list: the line scan cannot find the entries, so it attaches nothing.
        let text = "matches:\n  # note\n  [{trigger: a, replace: x}]\nglobal_vars: []\n";
        let (p, _) = parse_match_text(text);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].comment, None);
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
        fs::write(
            dir.join("match/_base.yml"),
            "matches:\n  - trigger: a\n    replace: b\n",
        )
        .unwrap();
        fs::write(dir.join("match/bad.yml"), "matches: [x").unwrap();
        let f = load_folders(&dir).unwrap();
        assert_eq!(f[0].name, "base");
        assert_eq!(f[1].name, "bad");
        assert_eq!(f[0].phrases.len(), 1);
        assert_eq!(f[1].warnings.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
