//! Core logic for xpando: discovers espanso match files and (later) reads and
//! edits phrases while keeping the YAML files as the single source of truth.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
}
