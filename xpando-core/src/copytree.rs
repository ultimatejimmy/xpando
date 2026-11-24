//! Copying files between two folders without ever overwriting or modifying anything.
//! Shared by the espanso import and by "copy my data to the new folder".

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::syncsafe;

/// A file that was left out or could not be copied, and why. Worded for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// Path relative to the source folder, with `/` separators.
    pub file: String,
    pub reason: String,
}

/// What a copy did. Nothing in the source is ever changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CopyReport {
    pub copied: Vec<String>,
    /// Files that already existed at the destination and were left untouched.
    pub already_there: Vec<String>,
    /// Files deliberately not copied (sync conflict copies, links, other file types).
    pub skipped: Vec<Problem>,
    /// Files that should have been copied but could not be.
    pub failed: Vec<Problem>,
}

/// A file found by [`walk`].
pub(crate) struct Found {
    /// Relative path with `/` separators.
    pub rel: String,
    pub size: u64,
}

pub(crate) fn is_yaml(name: &str) -> bool {
    matches!(
        Path::new(name).extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    )
}

/// Lists the regular files under `root/<sub>` for each of `subs` (or all of `root` when `subs`
/// is empty), sorted by path. Links are never followed; they and sync conflict copies go to
/// `skipped`, as do non-YAML files when `yaml_only`.
pub(crate) fn walk(
    root: &Path,
    subs: &[&str],
    yaml_only: bool,
    skipped: &mut Vec<Problem>,
    failed: &mut Vec<Problem>,
) -> Vec<Found> {
    let mut out = Vec::new();
    if subs.is_empty() {
        walk_dir(root, "", yaml_only, &mut out, skipped, failed);
    } else {
        for sub in subs {
            let dir = root.join(sub);
            if dir.is_dir() {
                walk_dir(&dir, sub, yaml_only, &mut out, skipped, failed);
            }
        }
    }
    out
}

fn join_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}/{name}")
    }
}

fn walk_dir(
    dir: &Path,
    rel: &str,
    yaml_only: bool,
    out: &mut Vec<Found>,
    skipped: &mut Vec<Problem>,
    failed: &mut Vec<Problem>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            failed.push(Problem {
                file: rel.to_string(),
                reason: format!("The folder could not be read: {e}"),
            });
            return;
        }
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let file = join_rel(rel, &name);
        let Ok(kind) = entry.file_type() else {
            failed.push(Problem {
                file,
                reason: "Could not tell what kind of file this is".into(),
            });
            continue;
        };
        if kind.is_symlink() {
            skipped.push(Problem {
                file,
                reason: "Shortcuts and links are not followed".into(),
            });
        } else if kind.is_dir() {
            walk_dir(&path, &file, yaml_only, out, skipped, failed);
        } else if yaml_only && !is_yaml(&name) {
            skipped.push(Problem {
                file,
                reason: "Not a YAML file".into(),
            });
        } else if is_yaml(&name) && syncsafe::is_conflicted_copy(&name) {
            skipped.push(Problem {
                file,
                reason: "Looks like a sync conflict copy of another file".into(),
            });
        } else {
            match entry.metadata() {
                Ok(m) => out.push(Found {
                    rel: file,
                    size: m.len(),
                }),
                Err(e) => failed.push(Problem {
                    file,
                    reason: format!("Could not read the file: {e}"),
                }),
            }
        }
    }
}

/// Copies one file to `dest`, never overwriting. Returns `Ok(false)` when `dest` exists.
/// A partly written destination is removed again if the copy fails.
pub(crate) fn copy_new(src: &Path, dest: &Path) -> io::Result<bool> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = match OpenOptions::new().write(true).create_new(true).open(dest) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };
    let result = fs::File::open(src).and_then(|mut input| io::copy(&mut input, &mut out));
    if let Err(e) = result {
        drop(out);
        let _ = fs::remove_file(dest);
        return Err(e);
    }
    Ok(true)
}

/// Copies every found file from `src_root` to `dest_root`, recording the outcome.
pub(crate) fn copy_found(
    found: &[Found],
    src_root: &Path,
    dest_root: &Path,
    report: &mut CopyReport,
) {
    for f in found {
        let rel: PathBuf = f.rel.split('/').collect();
        match copy_new(&src_root.join(&rel), &dest_root.join(&rel)) {
            Ok(true) => report.copied.push(f.rel.clone()),
            Ok(false) => report.already_there.push(f.rel.clone()),
            Err(e) => report.failed.push(Problem {
                file: f.rel.clone(),
                reason: format!("Could not copy: {e}"),
            }),
        }
    }
}

/// Resolves a path that may not exist yet by canonicalising its nearest existing ancestor.
fn resolve(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(real) = cur.canonicalize() {
            return rest.iter().rev().fold(real, |p, part| p.join(part));
        }
        match (cur.parent().map(Path::to_path_buf), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                cur = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Refuses folders that are the same or inside each other, where copying would loop or the
/// result would be confusing.
pub(crate) fn check_not_nested(source: &Path, dest: &Path) -> Result<(), String> {
    let (s, d) = (resolve(source), resolve(dest));
    if s == d {
        return Err("The source and the destination are the same folder.".into());
    }
    if d.starts_with(&s) {
        return Err("The destination is inside the source folder.".into());
    }
    if s.starts_with(&d) {
        return Err("The source folder is inside the destination folder.".into());
    }
    Ok(())
}
