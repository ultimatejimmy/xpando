//! Safety helpers for data folders that live in a cloud-synced location (OneDrive, Google
//! Drive, Dropbox, Syncthing...). Sync tools can leave "conflicted copy" files next to the
//! original, and a synced folder can be missing (signed out, offline, disk not connected).
//!
//! Everything here only reads. Nothing is renamed, merged or deleted: the user decides.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

pub use crate::datadir::{check_dir, is_reachable, DirStatus};

/// A file that looks like a sync conflict copy of another file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictedCopy {
    /// Path relative to the data folder, with `/` separators, e.g. `match/base (1).yml`.
    pub file: String,
    /// The name of the file it is probably a copy of, e.g. `base.yml`.
    pub original: Option<String>,
    /// Whether that original is in the same folder.
    pub original_exists: bool,
}

/// Removes the first parenthesised group that contains `marker` (case-insensitive), together
/// with the space before it: `a (Bob's conflicted copy 2024).yml` becomes `a.yml`.
fn strip_group(name: &str, marker: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let at = lower.find(marker)?;
    // Byte offsets in `lower` can differ from `name` only for exotic Unicode; bail out then.
    if lower.len() != name.len() {
        return None;
    }
    let open = name[..at].rfind('(')?;
    let close = at + name[at..].find(')')?;
    let head = name[..open].trim_end_matches(' ');
    let tail = &name[close + 1..];
    Some(format!("{head}{tail}"))
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// If `file_name` looks like a sync conflict copy, returns `Some(original)`, where the
/// original is `None` when it cannot be worked out. Recognised, by tool:
///
/// * Dropbox and Nextcloud: `name (Jane's conflicted copy 2024-01-31).yml`,
///   `name (conflicted copy).yml`, `name (case conflict).yml`, `name (Selective Sync Conflict).yml`
/// * Syncthing: `name.sync-conflict-20240131-120000-ABCDEFG.yml`
/// * OneDrive: `name-DESKTOP-ABC123.yml`, `name-LAPTOP-ABC123.yml` (the computer name is added)
/// * Google Drive, browsers and Windows copies: `name (1).yml`
fn analyse(file_name: &str) -> Option<Option<String>> {
    let lower = file_name.to_lowercase();
    for marker in [
        "conflicted copy",
        "case conflict",
        "selective sync conflict",
    ] {
        if lower.contains(marker) {
            return Some(strip_group(file_name, marker));
        }
    }
    if let Some(at) = file_name.find(".sync-conflict-") {
        let (_, ext) = split_ext(&file_name[at + 1..]);
        return Some(Some(format!("{}{}", &file_name[..at], ext)));
    }
    let (stem, ext) = split_ext(file_name);
    for tag in ["-DESKTOP-", "-LAPTOP-"] {
        if let Some(at) = stem.find(tag) {
            if at > 0 {
                return Some(Some(format!("{}{}", &stem[..at], ext)));
            }
        }
    }
    // "name (1)": a space, an opening bracket, digits and a closing bracket at the end.
    if let Some(open) = stem.rfind(" (") {
        let inner = stem[open + 2..].strip_suffix(')');
        if open > 0 && inner.is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        {
            return Some(Some(format!("{}{}", &stem[..open], ext)));
        }
    }
    None
}

/// Whether a file name looks like a sync conflict copy (see [`analyse`] for the patterns).
/// "Looks like" because a person can also name a file `notes (1).yml`; callers should offer
/// the files for review, never delete them.
pub fn is_conflicted_copy(file_name: &str) -> bool {
    analyse(file_name).is_some()
}

/// The name of the file a conflict copy was made from, when it can be told.
pub fn original_name(file_name: &str) -> Option<String> {
    analyse(file_name).flatten()
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    )
}

fn scan(dir: &Path, rel: &str, out: &mut Vec<ConflictedCopy>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            scan(&path, &format!("{rel}/{name}"), out)?;
        } else if kind.is_file() && is_yaml(&path) {
            if let Some(original) = analyse(&name) {
                let original_exists = original.as_ref().is_some_and(|o| dir.join(o).is_file());
                out.push(ConflictedCopy {
                    file: format!("{rel}/{name}"),
                    original,
                    original_exists,
                });
            }
        }
    }
    Ok(())
}

/// Finds files in `<data_dir>/match` and `<data_dir>/config` that look like sync conflict
/// copies. Such files are loaded by the engine like any other file, so their triggers may
/// expand twice or an older version of a phrase may win.
pub fn find_conflicted_copies(data_dir: &Path) -> io::Result<Vec<ConflictedCopy>> {
    let mut out = Vec::new();
    for sub in ["match", "config"] {
        let dir = data_dir.join(sub);
        if dir.is_dir() {
            scan(&dir, sub, &mut out)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xpando-sync-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recognises_each_sync_tool() {
        for name in [
            "base (conflicted copy).yml",
            "base (Jane's conflicted copy 2024-01-31).yml",
            "base (case conflict).yml",
            "base (Selective Sync Conflict).yml",
            "base.sync-conflict-20240131-120000-ABCDEFG.yml",
            "base-DESKTOP-ABC123.yml",
            "base-LAPTOP-9XY.yml",
            "base (1).yml",
            "base (12).yaml",
            "_base (2).yml",
            "BASE (CONFLICTED COPY).yml",
        ] {
            assert!(is_conflicted_copy(name), "{name}");
        }
    }

    #[test]
    fn ordinary_names_are_not_flagged() {
        for name in [
            "base.yml",
            "_base.yml",
            "my notes.yml",
            "work (old).yml",
            "work ().yml",
            "(1).yml",
            "desktop-notes.yml",
            "DESKTOP-notes.yml",
            "a-DESKTOP.yml",
            "default.yml",
            "",
        ] {
            assert!(!is_conflicted_copy(name), "{name}");
        }
    }

    #[test]
    fn works_out_the_original() {
        let cases = [
            ("base (conflicted copy).yml", "base.yml"),
            ("base (Jane's conflicted copy 2024-01-31).yml", "base.yml"),
            ("base (case conflict 1).yml", "base.yml"),
            ("base.sync-conflict-20240131-120000-ABCDEFG.yml", "base.yml"),
            ("base-DESKTOP-ABC123.yml", "base.yml"),
            ("base (1).yml", "base.yml"),
            ("_base (3).yaml", "_base.yaml"),
            ("two words (conflicted copy).yml", "two words.yml"),
        ];
        for (copy, original) in cases {
            assert_eq!(original_name(copy).as_deref(), Some(original), "{copy}");
        }
        assert_eq!(original_name("base.yml"), None);
    }

    #[test]
    fn finds_conflicts_in_match_and_config_only() {
        let dir = temp_dir("find");
        fs::create_dir_all(dir.join("match/sub")).unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::create_dir_all(dir.join("other")).unwrap();
        fs::write(dir.join("match/base.yml"), "").unwrap();
        fs::write(dir.join("match/base (1).yml"), "").unwrap();
        fs::write(dir.join("match/sub/a (conflicted copy).yml"), "").unwrap();
        fs::write(dir.join("match/notes (1).txt"), "").unwrap();
        fs::write(dir.join("config/default-DESKTOP-X1.yml"), "").unwrap();
        fs::write(dir.join("other/x (1).yml"), "").unwrap();

        let found = find_conflicted_copies(&dir).unwrap();
        let files: Vec<_> = found.iter().map(|c| c.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "match/base (1).yml",
                "match/sub/a (conflicted copy).yml",
                "config/default-DESKTOP-X1.yml"
            ]
        );
        assert!(found[0].original_exists);
        assert_eq!(found[0].original.as_deref(), Some("base.yml"));
        assert!(!found[1].original_exists);
        assert!(!found[2].original_exists);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_or_missing_folders_have_no_conflicts() {
        let dir = temp_dir("empty");
        assert!(find_conflicted_copies(&dir).unwrap().is_empty());
        assert!(find_conflicted_copies(&dir.join("nope"))
            .unwrap()
            .is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reachability_is_reexported() {
        let dir = temp_dir("reach");
        assert!(is_reachable(&dir));
        assert!(!is_reachable(&dir.join("gone")));
        fs::remove_dir_all(&dir).unwrap();
    }
}
