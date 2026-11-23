//! Helper that turns the PowerShell clipboard hack into xpando's `set_clipboard` macro.
//!
//! PhraseExpress' `{#setclipboard}` was converted to a macro that runs PowerShell:
//!
//! ```yaml
//! - name: setclip
//!   type: shell
//!   params: { shell: powershell, cmd: "Set-Clipboard -Value $env:ESPANSO_OUT" }
//! ```
//!
//! This module only *finds* such macros and *computes* the converted file text. Nothing here
//! writes a file and nothing runs by itself: the app shows [`preview`] and calls [`convert`]
//! only when the user clicks. A macro that does not match exactly is never offered, so
//! anything unusual stays untouched.

use serde::Serialize;

use crate::advanced::entry_source;
use crate::vars::{self, ParamValue, VarParam, VarSpec};

/// Settings of a shell macro that do not change what the hack does.
const HARMLESS: &[&str] = &["shell", "cmd", "trim", "debug"];

/// One shell macro that only copies an earlier macro's output to the clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// Position of the macro in the phrase's `vars:` list.
    pub position: usize,
    /// Name of the shell macro (it keeps this name after the conversion).
    pub name: String,
    /// The text it puts on the clipboard, as a macro reference, e.g. `{{out}}`.
    pub text: String,
}

/// What the user sees before agreeing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub candidate: Candidate,
    /// The phrase's source lines now.
    pub before: Vec<String>,
    /// The phrase's source lines after the conversion.
    pub after: Vec<String>,
}

fn text_param<'a>(spec: &'a VarSpec, key: &str) -> Option<&'a str> {
    spec.params
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match &p.value {
            ParamValue::Text(t) => Some(t.as_str()),
            _ => None,
        })
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].trim();
        }
    }
    s
}

fn is_powershell(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let n = n.strip_suffix(".exe").unwrap_or(&n);
    n == "powershell" || n == "pwsh"
}

/// Removes a `powershell -NoProfile -Command "..."` wrapper. Returns the inner command and
/// whether there was a wrapper.
fn strip_wrapper(cmd: &str) -> (&str, bool) {
    let cmd = cmd.trim();
    let first = cmd.split_whitespace().next().unwrap_or("");
    if !is_powershell(first) {
        return (cmd, false);
    }
    let mut rest = cmd[first.len()..].trim_start();
    loop {
        let tok = rest.split_whitespace().next().unwrap_or("");
        match tok.to_ascii_lowercase().as_str() {
            "-noprofile" | "-nologo" | "-noninteractive" => rest = rest[tok.len()..].trim_start(),
            "-c" | "-command" => return (unquote(rest[tok.len()..].trim_start()), true),
            _ => return (cmd, false),
        }
    }
}

/// The macro name an `$env:ESPANSO_<NAME>` token refers to, upper-cased.
fn env_ref(token: &str) -> Option<String> {
    let t = unquote(token).trim_matches(|c| c == '\\' || c == '"');
    let (head, rest) = (t.get(..13)?, t.get(13..)?);
    if !head.eq_ignore_ascii_case("$env:espanso_")
        || rest.is_empty()
        || !rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(rest.to_ascii_uppercase())
}

/// Recognises `Set-Clipboard [-Value] $env:ESPANSO_X` and `$env:ESPANSO_X | Set-Clipboard`.
fn parse_command(cmd: &str) -> Option<String> {
    let is_set = |t: &str| {
        let t = t.to_ascii_lowercase();
        t == "set-clipboard" || t == "scb"
    };
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    match toks.as_slice() {
        [c, v] if is_set(c) => env_ref(v),
        [c, flag, v]
            if is_set(c)
                && (flag.eq_ignore_ascii_case("-value") || flag.eq_ignore_ascii_case("-v")) =>
        {
            env_ref(v)
        }
        [v, "|", c] if is_set(c) => env_ref(v),
        _ => None,
    }
}

fn candidate_of(specs: &[VarSpec], position: usize) -> Option<Candidate> {
    let spec = &specs[position];
    if spec.kind != "shell"
        || spec
            .params
            .iter()
            .any(|p| !HARMLESS.contains(&p.key.as_str()))
    {
        return None;
    }
    let cmd = text_param(spec, "cmd")?;
    let (inner, wrapped) = strip_wrapper(cmd);
    if let Some(sh) = text_param(spec, "shell") {
        if !wrapped && !is_powershell(sh) {
            return None;
        }
    }
    let wanted = parse_command(unquote(inner))?;
    // It must refer to a macro defined before it, otherwise ESPANSO_X would be empty.
    let target = specs[..position]
        .iter()
        .find(|s| s.name.to_ascii_uppercase() == wanted)?;
    Some(Candidate {
        position,
        name: spec.name.clone(),
        text: format!("{{{{{}}}}}", target.name),
    })
}

/// Finds every macro of phrase `index` that is the PowerShell clipboard hack. An error means
/// the phrase's macros cannot be edited by the app (same rule as the macro editor).
pub fn find(text: &str, index: usize) -> Result<Vec<Candidate>, String> {
    let specs = vars::read_vars(text, index)?;
    Ok((0..specs.len())
        .filter_map(|i| candidate_of(&specs, i))
        .collect())
}

/// The file text with macro `name` of phrase `index` converted. Only that macro changes.
pub fn convert(text: &str, index: usize, name: &str) -> Result<String, String> {
    let mut specs = vars::read_vars(text, index)?;
    let cand = (0..specs.len())
        .filter_map(|i| candidate_of(&specs, i))
        .find(|c| c.name == name)
        .ok_or("This macro is not a PowerShell clipboard command any more.")?;
    let old = &specs[cand.position];
    specs[cand.position] = VarSpec {
        name: old.name.clone(),
        kind: "set_clipboard".into(),
        params: vec![VarParam {
            key: "text".into(),
            value: ParamValue::Text(cand.text),
        }],
        source: old.source,
    };
    vars::update_vars(text, index, &specs)
}

/// Before/after source of the phrase for the first candidate, or `None` when there is
/// nothing to convert. Pure: nothing is written.
pub fn preview(text: &str, index: usize) -> Result<Option<Preview>, String> {
    let Some(candidate) = find(text, index)?.into_iter().next() else {
        return Ok(None);
    };
    let new_text = convert(text, index, &candidate.name)?;
    let lines = |t: &str| entry_source(t, index).map(|e| e.lines);
    match (lines(text), lines(&new_text)) {
        (Some(before), Some(after)) => Ok(Some(Preview {
            candidate,
            before,
            after,
        })),
        _ => Err("The phrase cannot be previewed.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(cmd: &str, shell: Option<&str>) -> String {
        let shell = shell
            .map(|s| format!("          shell: {s}\n"))
            .unwrap_or_default();
        format!(
            "matches:\n  - trigger: \"TDC\"\n    replace: \"{{{{out}}}}\"\n    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y%m%d\"\n      - name: out\n        type: echo\n        params:\n          echo: \"{{{{d}}}}\"\n      # keep me\n      - name: setclip\n        type: shell\n        params:\n{shell}          cmd: {cmd}\n  - trigger: \":z\"\n    replace: \"last\"\n"
        )
    }

    fn tdc() -> String {
        file(
            "\"Set-Clipboard -Value $env:ESPANSO_OUT\"",
            Some("powershell"),
        )
    }

    #[test]
    fn finds_the_standard_hack() {
        let c = find(&tdc(), 0).unwrap();
        assert_eq!(
            c,
            vec![Candidate {
                position: 2,
                name: "setclip".into(),
                text: "{{out}}".into()
            }]
        );
    }

    #[test]
    fn finds_common_spellings() {
        for (cmd, shell) in [
            ("\"Set-Clipboard $env:ESPANSO_OUT\"", Some("pwsh")),
            ("\"set-clipboard -v $env:espanso_out\"", None),
            ("\"$env:ESPANSO_OUT | Set-Clipboard\"", Some("powershell")),
            (
                "\"Set-Clipboard -Value \\\"$env:ESPANSO_OUT\\\"\"",
                Some("powershell"),
            ),
            (
                "'Set-Clipboard -Value $env:ESPANSO_OUT'",
                Some("powershell"),
            ),
            (
                "\"powershell -NoProfile -Command \\\"Set-Clipboard -Value $env:ESPANSO_OUT\\\"\"",
                Some("cmd"),
            ),
        ] {
            assert_eq!(find(&file(cmd, shell), 0).unwrap().len(), 1, "{cmd}");
        }
    }

    #[test]
    fn ignores_everything_else() {
        for (cmd, shell) in [
            ("\"Get-Date | Set-Clipboard\"", Some("powershell")),
            (
                "\"Set-Clipboard -Value $env:ESPANSO_OUT; calc\"",
                Some("powershell"),
            ),
            (
                "\"Set-Clipboard -Value $env:ESPANSO_NOPE\"",
                Some("powershell"),
            ),
            (
                "\"Set-Clipboard -Value $env:ESPANSO_SETCLIP\"",
                Some("powershell"),
            ),
            ("\"Set-Clipboard -Value $env:ESPANSO_OUT\"", Some("bash")),
            ("\"echo hi\"", Some("powershell")),
        ] {
            assert!(find(&file(cmd, shell), 0).unwrap().is_empty(), "{cmd}");
        }
        assert!(find(&tdc(), 1).unwrap().is_empty());
    }

    #[test]
    fn extra_settings_block_the_offer() {
        let t = tdc().replace(
            "          shell: powershell\n",
            "          shell: powershell\n          ignore_error: true\n",
        );
        assert!(find(&t, 0).unwrap().is_empty());
    }

    #[test]
    fn converts_only_the_one_macro() {
        let text = tdc();
        let out = convert(&text, 0, "setclip").unwrap();
        assert!(out.contains("type: set_clipboard"));
        assert!(out.contains("text: \"{{out}}\""));
        assert!(!out.contains("powershell"));
        assert!(out.contains("# keep me"));
        assert!(out.contains("trigger: \":z\""));
        let read = vars::read_vars(&out, 0).unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(read[2].name, "setclip");
        assert_eq!(read[2].kind, "set_clipboard");
        assert_eq!(read[0].kind, "date");
        assert!(find(&out, 0).unwrap().is_empty());
        // The first two macros and the other phrase are byte-identical.
        let head = |t: &str| t.split("      - name: setclip").next().unwrap().to_string();
        assert_eq!(head(&text), head(&out));
        assert!(out.ends_with("    replace: \"last\"\n"));
    }

    #[test]
    fn convert_refuses_what_it_would_not_offer() {
        let text = file("\"Get-Date | Set-Clipboard\"", Some("powershell"));
        assert!(convert(&text, 0, "setclip").is_err());
        assert!(convert(&tdc(), 0, "out").is_err());
        assert!(convert(&tdc(), 0, "missing").is_err());
    }

    #[test]
    fn preview_shows_both_sides_and_changes_nothing() {
        let text = tdc();
        let p = preview(&text, 0).unwrap().unwrap();
        assert!(p.before.iter().any(|l| l.contains("Set-Clipboard")));
        assert!(p.after.iter().any(|l| l.contains("set_clipboard")));
        assert!(preview(&text, 1).unwrap().is_none());
        assert_eq!(text, tdc());
    }
}
