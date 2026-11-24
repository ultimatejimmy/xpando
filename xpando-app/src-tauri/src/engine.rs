//! Commands about the espanso engine the app talks to: which binary it is (stock espanso or
//! xpando's patched build) and the one-click conversion of the PowerShell clipboard hack.
//! The logic lives in `xpando_core::engine` and `xpando_core::clipboard_hack`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tauri::AppHandle;
use xpando_core::clipboard_hack::{self, Preview};
use xpando_core::edit;
use xpando_core::engine::{classify_version_output, EngineKind, EngineStatus};
use xpando_core::Folder;

use crate::espanso_config_dir;

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Where to look, in order: the engine shipped with the app (Tauri's `externalBin` puts
/// `xpando-engine` next to the app executable; an `engine` folder is also checked), then
/// `espanso` on the PATH, then the usual stock install locations.
fn candidates() -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        out.push((dir.join(exe_name("xpando-engine")), true));
        out.push((dir.join("engine").join(exe_name("espanso")), true));
    }
    out.push((PathBuf::from(exe_name("espanso")), false));
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let programs = PathBuf::from(local).join("Programs");
            out.push((programs.join("Espanso").join("espanso.exe"), false));
        }
    } else if cfg!(target_os = "macos") {
        out.push((
            PathBuf::from("/Applications/Espanso.app/Contents/MacOS/espanso"),
            false,
        ));
    } else {
        out.push((PathBuf::from("/usr/local/bin/espanso"), false));
        out.push((PathBuf::from("/usr/bin/espanso"), false));
    }
    out
}

/// Runs `<path> --version`, giving up after a few seconds.
fn version_output(path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });
    let out = rx.recv_timeout(Duration::from_secs(5)).ok()?.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Finds the engine and says whether it is xpando's build. A stock espanso is reported only
/// if no xpando build is found, so a bundled engine always wins.
#[tauri::command]
pub fn engine_status() -> EngineStatus {
    let mut stock: Option<EngineStatus> = None;
    for (path, bundled) in candidates() {
        if path.is_absolute() && !path.is_file() {
            continue;
        }
        let Some(output) = version_output(&path) else {
            continue;
        };
        let (kind, version) = classify_version_output(&output);
        if kind == EngineKind::Missing {
            continue;
        }
        let status = EngineStatus {
            kind,
            path: Some(path.to_string_lossy().into_owned()),
            version,
            bundled,
        };
        if kind == EngineKind::Xpando {
            return status;
        }
        stock.get_or_insert(status);
    }
    stock.unwrap_or(EngineStatus {
        kind: EngineKind::Missing,
        path: None,
        version: None,
        bundled: false,
    })
}

/// What converting the PowerShell clipboard hack of a phrase would change, or `None` when the
/// phrase has none. Reads only; the file is not touched.
#[tauri::command]
pub fn preview_clipboard_hack(
    app: AppHandle,
    file: String,
    index: usize,
) -> Result<Option<Preview>, String> {
    let dir = espanso_config_dir(&app)?;
    let path = edit::resolve_match_file(&dir, &file)?;
    let text = fs::read_to_string(path).map_err(|e| format!("Could not read the file: {e}"))?;
    clipboard_hack::preview(&text, index)
}

/// Converts macro `name` of a phrase. Called only from the user's click on "Convert"; the hash
/// check refuses to write if the file changed since the preview.
#[tauri::command]
pub fn convert_clipboard_hack(
    app: AppHandle,
    file: String,
    index: usize,
    name: String,
    expected_hash: String,
) -> Result<Folder, String> {
    let dir = espanso_config_dir(&app)?;
    edit::edit_file(&dir, &file, &expected_hash, |text| {
        clipboard_hack::convert(text, index, &name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sidecar_is_looked_up_first() {
        let list = candidates();
        let (first, bundled) = &list[0];
        assert!(*bundled);
        // Tauri installs the externalBin `binaries/xpando-engine` as `xpando-engine[.exe]`
        // next to the app executable.
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            exe_name("xpando-engine")
        );
        assert_eq!(first.parent(), std::env::current_exe().unwrap().parent());
        assert!(list.iter().skip(2).all(|(_, b)| !*b));
    }
}
