//! Read-only commands behind the app's "Advanced" mode.

use std::fs;
use std::path::{Component, Path, PathBuf};

use tauri::AppHandle;

use crate::data::data_dir;

/// Resolves `file` (relative to `<config>/match`) to a real file inside that directory.
/// Rejects absolute paths, `..`, and symlinks that leave the directory.
fn resolve_match_file(config_dir: &Path, file: &str) -> Result<PathBuf, String> {
    let rel = Path::new(file);
    if file.is_empty()
        || rel.is_absolute()
        || !rel.components().all(|c| matches!(c, Component::Normal(_)))
        || !matches!(
            rel.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        )
    {
        return Err("invalid match file path".into());
    }
    let root = config_dir
        .join("match")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = root.join(rel).canonicalize().map_err(|e| e.to_string())?;
    if path.starts_with(&root) {
        Ok(path)
    } else {
        Err("invalid match file path".into())
    }
}

/// The raw source of one phrase (its comment block and entry lines), with real line numbers.
/// `None` when the file cannot be scanned reliably.
#[tauri::command]
pub fn get_entry_source(
    app: AppHandle,
    file: String,
    index: usize,
) -> Result<Option<xpando_core::advanced::EntrySource>, String> {
    let path = resolve_match_file(&data_dir(&app)?, &file)?;
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(xpando_core::advanced::entry_source(&text, index))
}

/// Trigger conflicts across all match files.
#[tauri::command]
pub fn list_conflicts(app: AppHandle) -> Result<Vec<xpando_core::conflicts::Conflict>, String> {
    let dir = data_dir(&app)?;
    xpando_core::conflicts::find_conflicts_in_dir(&dir).map_err(|e| e.to_string())
}

/// Where espanso's config directory is, for the status bar.
#[tauri::command]
pub fn config_path(app: AppHandle) -> Result<String, String> {
    Ok(data_dir(&app)?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xpando-adv-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("match/sub")).unwrap();
        fs::write(dir.join("match/a.yml"), "matches: []\n").unwrap();
        fs::write(dir.join("match/sub/b.yaml"), "matches: []\n").unwrap();
        fs::write(dir.join("secret.yml"), "x: 1\n").unwrap();
        dir
    }

    #[test]
    fn accepts_files_inside_match() {
        let dir = setup("ok");
        assert!(resolve_match_file(&dir, "a.yml").is_ok());
        assert!(resolve_match_file(&dir, "sub/b.yaml").is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_escapes_and_odd_paths() {
        let dir = setup("bad");
        for bad in [
            "../secret.yml",
            "sub/../../secret.yml",
            "",
            "a.txt",
            "missing.yml",
            "/etc/passwd.yml",
            "C:\\x.yml",
            "./a.yml",
        ] {
            assert!(resolve_match_file(&dir, bad).is_err(), "{bad}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
