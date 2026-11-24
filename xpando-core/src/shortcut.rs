//! The keyboard shortcut that opens the search box: the `search_shortcut` line of
//! `config/default.yml`. Only that one line is read or changed; the rest of the file is kept
//! byte for byte.

/// What the engine uses when the file has no `search_shortcut` line.
pub const DEFAULT_SHORTCUT: &str = "ALT+SPACE";

const KEY: &str = "search_shortcut";

fn is_key_line(line: &str) -> bool {
    // Top-level only: no indentation, so a nested key of the same name is left alone.
    line.strip_prefix(KEY)
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

fn value_of(line: &str) -> String {
    let after = line[KEY.len()..].trim_start();
    let after = after.strip_prefix(':').unwrap_or(after).trim();
    let after = match after.chars().next() {
        Some(q @ ('"' | '\'')) => after[1..].split(q).next().unwrap_or("").to_string(),
        _ => after.split(" #").next().unwrap_or("").trim().to_string(),
    };
    after
}

/// The shortcut in effect for this `default.yml` text, upper-case, or `OFF`.
pub fn read_search_shortcut(yaml: &str) -> String {
    for line in yaml.lines() {
        if is_key_line(line) {
            let v = value_of(line);
            if v.is_empty() {
                break;
            }
            return if v.eq_ignore_ascii_case("off") {
                "OFF".to_string()
            } else {
                v.to_uppercase()
            };
        }
    }
    DEFAULT_SHORTCUT.to_string()
}

/// Checks a shortcut such as `CTRL+SPACE`: one or more modifiers and one key, joined by `+`.
/// `OFF` is allowed. Returns the cleaned-up value, or a message in plain words.
pub fn validate_shortcut(value: &str) -> Result<String, String> {
    let v = value.trim().to_uppercase();
    if v == "OFF" {
        return Ok(v);
    }
    let parts: Vec<&str> = v.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric())) {
        return Err("That is not a valid shortcut.".to_string());
    }
    let (key, mods) = parts.split_last().ok_or("That is not a valid shortcut.")?;
    const MODS: [&str; 7] = ["CTRL", "CONTROL", "ALT", "SHIFT", "CMD", "META", "SUPER"];
    if mods.is_empty() {
        return Err("Include Ctrl, Alt, Shift or Windows along with the key.".to_string());
    }
    if mods.iter().any(|m| !MODS.contains(m)) || MODS.contains(key) {
        return Err("Press one or more of Ctrl, Alt or Shift, then a key.".to_string());
    }
    if *mods == ["SHIFT"] {
        return Err("Shift alone would clash with typing. Add Ctrl or Alt.".to_string());
    }
    Ok(parts.join("+"))
}

/// `yaml` with the `search_shortcut` line set to `value` (already validated). The line is
/// replaced in place, or added at the end when there is none.
pub fn set_search_shortcut(yaml: &str, value: &str) -> String {
    let new_line = format!("{KEY}: \"{value}\"");
    let mut out = String::with_capacity(yaml.len() + new_line.len() + 1);
    let mut done = false;
    for piece in yaml.split_inclusive('\n') {
        let body = piece.trim_end_matches(['\r', '\n']);
        if !done && is_key_line(body) {
            out.push_str(&new_line);
            out.push_str(&piece[body.len()..]);
            done = true;
        } else {
            out.push_str(piece);
        }
    }
    if !done {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&new_line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_default_when_missing() {
        assert_eq!(read_search_shortcut(""), "ALT+SPACE");
        assert_eq!(read_search_shortcut("toggle_key: ALT\n"), "ALT+SPACE");
        assert_eq!(read_search_shortcut("  search_shortcut: \"X+Y\"\n"), "ALT+SPACE");
    }

    #[test]
    fn reads_values() {
        assert_eq!(read_search_shortcut("search_shortcut: ctrl+space\n"), "CTRL+SPACE");
        assert_eq!(read_search_shortcut("a: 1\nsearch_shortcut: \"ALT+K\" # x\n"), "ALT+K");
        assert_eq!(read_search_shortcut("search_shortcut: off\n"), "OFF");
        assert_eq!(read_search_shortcut("search_shortcut: 'CTRL+J'\n"), "CTRL+J");
    }

    #[test]
    fn sets_in_place_and_keeps_the_rest() {
        let src = "# hi\r\nsearch_shortcut: ALT+SPACE\r\ntoggle_key: ALT\r\n";
        assert_eq!(
            set_search_shortcut(src, "CTRL+SPACE"),
            "# hi\r\nsearch_shortcut: \"CTRL+SPACE\"\r\ntoggle_key: ALT\r\n"
        );
    }

    #[test]
    fn adds_when_missing() {
        assert_eq!(set_search_shortcut("", "CTRL+K"), "search_shortcut: \"CTRL+K\"\n");
        assert_eq!(
            set_search_shortcut("a: 1", "CTRL+K"),
            "a: 1\nsearch_shortcut: \"CTRL+K\"\n"
        );
    }

    #[test]
    fn validates() {
        assert_eq!(validate_shortcut(" ctrl + space "), Ok("CTRL+SPACE".to_string()));
        assert_eq!(validate_shortcut("off"), Ok("OFF".to_string()));
        assert!(validate_shortcut("SPACE").is_err());
        assert!(validate_shortcut("SHIFT+A").is_err());
        assert!(validate_shortcut("CTRL+").is_err());
        assert!(validate_shortcut("CTRL+ALT").is_err());
        assert!(validate_shortcut("CTRL+\"x\"").is_err());
        assert!(validate_shortcut("FOO+K").is_err());
    }
}
