//! Read-only helpers behind the app's "Advanced" mode.

use serde::Serialize;
use serde_norway::Value;

use crate::{parse_entry, scan_entries};

/// The exact source text of one entry of `matches:`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntrySource {
    /// 1-based line in the file of `lines[0]` (the start of the entry's comment block).
    pub first_line: usize,
    /// The entry's comment block and its own lines, without trailing blank lines.
    pub lines: Vec<String>,
    /// Whether the entry parses as a phrase.
    pub valid: bool,
    /// Why the entry does not parse, when `valid` is false.
    pub error: Option<String>,
}

/// Returns the raw source of entry `index` of `matches:` in `text`.
///
/// `None` when the file is not valid YAML, has no block-style `matches:` list, the line scan
/// disagrees with the YAML parser about the number of entries, or `index` is out of range.
pub fn entry_source(text: &str, index: usize) -> Option<EntrySource> {
    let doc: Value = serde_norway::from_str(text).ok()?;
    let entries = doc.as_mapping()?.get("matches")?.as_sequence()?;
    let spans = scan_entries(text)?;
    if spans.len() != entries.len() {
        return None;
    }
    let span = spans.get(index)?;
    let all: Vec<&str> = text.lines().collect();
    let end = span.end_line.min(all.len());
    let mut lines: Vec<String> = all[span.first_line..end]
        .iter()
        .map(|l| l.to_string())
        .collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let parsed = parse_entry(&entries[index]);
    Some(EntrySource {
        first_line: span.first_line + 1,
        lines,
        valid: parsed.is_ok(),
        error: parsed.err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# header\nmatches:\n  # Greeting\n  # second line\n  - trigger: \":hi\"\n    replace: Hello\n\n  - trigger: \":bye\"\n    replace: |\n      Bye\n      now\n\nglobal_vars:\n  - name: x\n";

    #[test]
    fn first_entry_includes_comment_block() {
        let s = entry_source(FILE, 0).unwrap();
        assert_eq!(s.first_line, 3);
        assert_eq!(
            s.lines,
            [
                "  # Greeting",
                "  # second line",
                "  - trigger: \":hi\"",
                "    replace: Hello"
            ]
        );
        assert!(s.valid);
        assert_eq!(s.error, None);
    }

    #[test]
    fn last_entry_stops_at_next_top_level_key() {
        let s = entry_source(FILE, 1).unwrap();
        assert_eq!(s.first_line, 8);
        assert_eq!(s.lines.last().unwrap(), "      now");
        assert_eq!(s.lines.len(), 4);
    }

    #[test]
    fn line_numbers_match_the_file() {
        let s = entry_source(FILE, 1).unwrap();
        let file_lines: Vec<&str> = FILE.lines().collect();
        assert_eq!(file_lines[s.first_line - 1], s.lines[0]);
    }

    #[test]
    fn out_of_range_index_is_none() {
        assert!(entry_source(FILE, 2).is_none());
    }

    #[test]
    fn unparseable_entry_is_reported_invalid() {
        let s = entry_source("matches:\n  - trigger: \":x\"\n", 0).unwrap();
        assert!(!s.valid);
        assert!(s.error.is_some());
    }

    #[test]
    fn invalid_yaml_is_none() {
        assert!(entry_source("matches:\n  - [unclosed\n", 0).is_none());
    }

    #[test]
    fn no_matches_key_is_none() {
        assert!(entry_source("foo: 1\n", 0).is_none());
        assert!(entry_source("", 0).is_none());
    }

    #[test]
    fn flow_style_list_is_none() {
        assert!(entry_source("matches: [{trigger: a, replace: b}]\n", 0).is_none());
    }

    #[test]
    fn crlf_text_is_handled() {
        let t = "matches:\r\n  - trigger: \":a\"\r\n    replace: b\r\n";
        let s = entry_source(t, 0).unwrap();
        assert_eq!(s.lines, ["  - trigger: \":a\"", "    replace: b"]);
    }
}
