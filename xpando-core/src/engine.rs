//! Telling xpando's patched espanso engine from stock espanso.
//!
//! The patched engine (see docs/xpando/ENGINE.md) reports a version with an `-xpando` suffix,
//! e.g. `espanso 2.4.1-xpando`. Only this module's pure parsing lives here; the app runs
//! the binary and passes the output in.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    /// Upstream espanso: rejects `set_clipboard`.
    Stock,
    /// xpando's build: understands `set_clipboard`.
    Xpando,
    /// No espanso binary was found (or it did not answer).
    Missing,
}

/// What `engine_status` reports to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineStatus {
    pub kind: EngineKind,
    /// The binary that answered, when one did.
    pub path: Option<String>,
    pub version: Option<String>,
    /// Whether it is the engine shipped with the app.
    pub bundled: bool,
}

/// Classifies the output of `espanso --version`. Returns the kind and the version text
/// (without the program name). Output that does not look like espanso is `Missing`.
pub fn classify_version_output(output: &str) -> (EngineKind, Option<String>) {
    let line = output.lines().next().unwrap_or("").trim();
    let mut words = line.split_whitespace();
    let first = words.next().unwrap_or("");
    // Stock espanso prints "espanso 2.2.1". xpando's engine, renamed, prints just the version
    // ("2.4.1-xpando"), sometimes with the program name in front. Anything else is not an engine.
    let is_version =
        |w: &str| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.');
    let version = if first.eq_ignore_ascii_case("espanso") {
        words.next().map(str::to_string)
    } else if is_version(first) && words.next().is_none() {
        Some(first.to_string())
    } else {
        None
    };
    let Some(version) = version else {
        return (EngineKind::Missing, None);
    };
    let kind = if version.to_ascii_lowercase().contains("xpando") {
        EngineKind::Xpando
    } else {
        EngineKind::Stock
    };
    (kind, Some(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert_eq!(
            classify_version_output("espanso 2.4.1-xpando\n"),
            (EngineKind::Xpando, Some("2.4.1-xpando".into()))
        );
        assert_eq!(
            classify_version_output("espanso 2.2.1"),
            (EngineKind::Stock, Some("2.2.1".into()))
        );
        // xpando's renamed engine prints only the version
        assert_eq!(
            classify_version_output(
                "2.4.1-xpando
"
            ),
            (EngineKind::Xpando, Some("2.4.1-xpando".into()))
        );
        assert_eq!(classify_version_output("2.4.1").0, EngineKind::Stock);
        assert_eq!(classify_version_output("").0, EngineKind::Missing);
        assert_eq!(
            classify_version_output("python 3.12").0,
            EngineKind::Missing
        );
    }
}
