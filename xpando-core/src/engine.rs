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
    if !words
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case("espanso"))
    {
        return (EngineKind::Missing, None);
    }
    let version = words.next().map(str::to_string);
    let kind = if version
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("xpando"))
    {
        EngineKind::Xpando
    } else {
        EngineKind::Stock
    };
    (kind, version)
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
        assert_eq!(classify_version_output("").0, EngineKind::Missing);
        assert_eq!(
            classify_version_output("python 3.12").0,
            EngineKind::Missing
        );
    }
}
