//! Commands about where xpando keeps its data: the data folder, the first-run import of an
//! existing espanso setup, changing the folder (with an optional copy) and sync warnings.
//! The logic and its tests live in `xpando_core::{datadir, import, syncsafe}`; this file only
//! locates the settings folder and passes values through. Errors are plain text for the UI.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use tauri::{AppHandle, Manager};
use xpando_core::datadir::{self, ChangeResult, DataInfo, Env, PathCheck, Settings};
use xpando_core::import::{self, ImportPreview};
use xpando_core::CopyReport;

/// Where the settings file lives: the app's own config folder, never the data folder, so it
/// stays put when the data folder is changed.
fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn settings(app: &AppHandle) -> Result<Settings, String> {
    datadir::load_settings(&settings_dir(app)?)
}

/// The data folder with `match/` and `config/` ready. Every command that reads or writes
/// phrases starts here. If the folder cannot be reached this is an error saying so: there is
/// no fallback to another folder.
pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(datadir::prepare(&Env::current(), &settings(app)?)?.path)
}

/// Where the data is, whether it can be reached, and any sync conflict copies.
#[tauri::command]
pub fn data_info(app: AppHandle) -> Result<DataInfo, String> {
    datadir::data_info(&Env::current(), &settings(&app)?)
}

/// What importing the user's espanso setup would copy, or `None` when there is nothing to
/// offer. On first run (`force` false) that also means: already answered, or xpando already
/// has data. `force` is the Settings button, which looks again whatever happened before.
#[tauri::command]
pub fn import_offer(app: AppHandle, force: bool) -> Result<Option<ImportPreview>, String> {
    let env = Env::current();
    let settings = settings(&app)?;
    if !force && settings.import_handled {
        return Ok(None);
    }
    let dest = datadir::prepare(&env, &settings)?.path;
    if !force && datadir::has_data(&dest) {
        return Ok(None);
    }
    for source in import::find_espanso_dirs(&env, &dest) {
        let preview = import::preview_import(&source, &dest)?;
        if !preview.files.is_empty() {
            return Ok(Some(preview));
        }
    }
    Ok(None)
}

fn mark_import_handled(app: &AppHandle) -> Result<(), String> {
    let dir = settings_dir(app)?;
    let mut s = datadir::load_settings(&dir)?;
    s.import_handled = true;
    datadir::save_settings(&dir, &s)
}

/// Copies the espanso files into the data folder. `source` must be one of the espanso folders
/// found on this computer; nothing in it is changed.
#[tauri::command]
pub fn import_espanso(app: AppHandle, source: String) -> Result<CopyReport, String> {
    let env = Env::current();
    let dest = data_dir(&app)?;
    let source = PathBuf::from(source);
    if !import::find_espanso_dirs(&env, &dest).contains(&source) {
        return Err("That is not an espanso folder xpando found.".into());
    }
    let report = import::run_import(&source, &dest)?;
    mark_import_handled(&app)?;
    Ok(report)
}

/// "Not now" on the first-run offer: do not ask again.
#[tauri::command]
pub fn skip_import(app: AppHandle) -> Result<(), String> {
    mark_import_handled(&app)
}

/// Checks a folder the user typed or picked, without changing anything.
#[tauri::command]
pub fn check_data_dir(app: AppHandle, path: String) -> Result<PathCheck, String> {
    let current = datadir::resolve(&Env::current(), &settings(&app)?)?.path;
    Ok(datadir::check_choice(&current, &path))
}

/// Starts using another data folder (`None` goes back to the default). With `copy`, the current
/// data is copied there first; the old folder is never changed. A copy with failures does not
/// switch.
#[tauri::command]
pub fn change_data_dir(
    app: AppHandle,
    path: Option<String>,
    copy: bool,
) -> Result<ChangeResult, String> {
    datadir::change_data_dir(&Env::current(), &settings_dir(&app)?, path.as_deref(), copy)
}

const PICKER_TITLE: &str = "Choose the folder for your xpando data";

/// Result of a picker run: `Ok(Some(path))` chosen, `Ok(None)` cancelled, `Err` could not run.
type PickResult = Result<Option<String>, String>;

const PICK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Cleans picker output: drops a BOM, line ends and surrounding whitespace; empty means none.
fn clean_picker_output(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}').trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Starts the command, waits up to `limit`, and returns (exit code, stdout, stderr).
pub(crate) fn run_with_timeout(
    mut cmd: Command,
    limit: std::time::Duration,
) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    use std::io::Read;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut b);
        }
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut b);
        }
        b
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The helper program took too long and was closed.".into());
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    };
    Ok((
        status.code().unwrap_or(-1),
        out_t.join().unwrap_or_default(),
        err_t.join().unwrap_or_default(),
    ))
}

/// Runs a picker program that exits 0 with the folder on stdout, and non-zero for cancel
/// (Linux and macOS pickers).
#[cfg(not(windows))]
fn run_picker(mut cmd: Command) -> PickResult {
    cmd.stdin(Stdio::null());
    let (code, out, _) = run_with_timeout(cmd, PICK_TIMEOUT)?;
    Ok(if code == 0 {
        clean_picker_output(&out)
    } else {
        None
    })
}

/// The system's folder picker. On Windows this is the modern "Select Folder" dialog (the one
/// Explorer-style apps use, with address bar, search and Quick Access), started through
/// PowerShell with a small COM snippet so xpando needs no extra package. On macOS it is
/// `osascript`, on Linux `zenity` or `kdialog`. `Ok(None)` means the user cancelled; `Err` is
/// a plain message when no picker could be shown, and the Settings text box still works.
#[tauri::command]
pub async fn pick_folder(window: tauri::WebviewWindow, start: Option<String>) -> PickResult {
    // On Windows the dialog is owned by the xpando window so it opens in front of it.
    #[cfg(windows)]
    let owner = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    #[cfg(not(windows))]
    let owner = {
        let _ = &window;
        0isize
    };
    tauri::async_runtime::spawn_blocking(move || pick_folder_blocking(start, owner))
        .await
        .map_err(|e| e.to_string())?
}

/// Standard base64 (for PowerShell's `-EncodedCommand`).
#[cfg(any(windows, test))]
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

/// The script as the UTF-16LE base64 that `-EncodedCommand` expects.
#[cfg(any(windows, test))]
fn encoded_picker_script() -> String {
    encode_powershell(WINDOWS_PICKER_SCRIPT)
}

/// A PowerShell script as the UTF-16LE base64 that `-EncodedCommand` expects.
#[cfg(any(windows, test))]
pub(crate) fn encode_powershell(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

/// The PowerShell script. The title, start folder and owner window arrive in environment
/// variables, never in the script text. Exit codes: 0 chosen (path on stdout, UTF-8), 2
/// cancelled, anything else a failure.
#[cfg(any(windows, test))]
const WINDOWS_PICKER_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
public static class XpandoPicker {
  [ComImport, Guid("DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7")] class FileOpenDialogCls {}
  [ComImport, Guid("43826d1e-e718-42ee-bc55-a1e261c37bfe"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IShellItem {
    void BindToHandler(IntPtr pbc, ref Guid bhid, ref Guid riid, out IntPtr ppv);
    void GetParent(out IShellItem ppsi);
    void GetDisplayName(uint sigdnName, out IntPtr ppszName);
    void GetAttributes(uint mask, out uint attrs);
    void Compare(IShellItem psi, uint hint, out int order);
  }
  [ComImport, Guid("42f85136-db7e-439c-85f1-e4075d135fc8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IFileOpenDialog {
    [PreserveSig] int Show(IntPtr hwndOwner);
    void SetFileTypes(uint c, IntPtr types);
    void SetFileTypeIndex(uint i);
    void GetFileTypeIndex(out uint i);
    void Advise(IntPtr events, out uint cookie);
    void Unadvise(uint cookie);
    void SetOptions(uint options);
    void GetOptions(out uint options);
    void SetDefaultFolder(IShellItem item);
    void SetFolder(IShellItem item);
    void GetFolder(out IShellItem item);
    void GetCurrentSelection(out IShellItem item);
    void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string name);
    void GetFileName([MarshalAs(UnmanagedType.LPWStr)] out string name);
    void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string title);
    void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string text);
    void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string text);
    void GetResult(out IShellItem item);
  }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHCreateItemFromParsingName(string path, IntPtr bc, ref Guid iid, out IShellItem item);
  [DllImport("ole32.dll")] static extern void CoTaskMemFree(IntPtr p);

  public static int Pick(string title, string start, long owner) {
    var dlg = (IFileOpenDialog)new FileOpenDialogCls();
    // FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST
    dlg.SetOptions(0x20 | 0x40 | 0x800);
    if (!string.IsNullOrEmpty(title)) dlg.SetTitle(title);
    if (!string.IsNullOrEmpty(start) && Directory.Exists(start)) {
      Guid iid = new Guid("43826d1e-e718-42ee-bc55-a1e261c37bfe");
      IShellItem folder;
      SHCreateItemFromParsingName(start, IntPtr.Zero, ref iid, out folder);
      dlg.SetFolder(folder);
    }
    int hr = dlg.Show(new IntPtr(owner));
    if (hr == unchecked((int)0x800704C7)) return 2;
    if (hr != 0) Marshal.ThrowExceptionForHR(hr);
    IShellItem item;
    dlg.GetResult(out item);
    IntPtr p;
    item.GetDisplayName(0x80058000, out p);
    string path = Marshal.PtrToStringUni(p);
    CoTaskMemFree(p);
    var bytes = new UTF8Encoding(false).GetBytes(path);
    using (var o = Console.OpenStandardOutput()) { o.Write(bytes, 0, bytes.Length); }
    return 0;
  }
}
'@
$owner = [long]0
if ($env:XPANDO_PICK_OWNER) { [void][long]::TryParse($env:XPANDO_PICK_OWNER, [ref]$owner) }
exit ([XpandoPicker]::Pick($env:XPANDO_PICK_TITLE, $env:XPANDO_PICK_START, $owner))
"#;

#[cfg(windows)]
fn pick_folder_blocking(start: Option<String>, owner: isize) -> PickResult {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encoded_picker_script(),
    ])
    .env("XPANDO_PICK_TITLE", PICKER_TITLE)
    .env("XPANDO_PICK_START", start.unwrap_or_default())
    .env("XPANDO_PICK_OWNER", owner.to_string())
    .stdin(Stdio::null())
    .creation_flags(0x0800_0000); // CREATE_NO_WINDOW (the dialog itself still shows)
    let (code, out, err) = run_with_timeout(cmd, PICK_TIMEOUT).map_err(|e| {
        format!("The folder dialog could not be opened ({e}). Type the folder path instead.")
    })?;
    match code {
        0 => Ok(clean_picker_output(&out)),
        2 => Ok(None),
        _ => {
            let detail = String::from_utf8_lossy(&err);
            let detail = detail
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim();
            Err(format!(
                "The folder dialog could not be opened. Type the folder path instead. {detail}"
            )
            .trim()
            .to_string())
        }
    }
}

#[cfg(target_os = "macos")]
fn pick_folder_blocking(_start: Option<String>, _owner: isize) -> PickResult {
    let mut cmd = Command::new("osascript");
    cmd.args([
        "-e",
        &format!("POSIX path of (choose folder with prompt \"{PICKER_TITLE}\")"),
    ]);
    run_picker(cmd).map(|p| p.map(|p| p.trim_end_matches('/').to_string()))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn pick_folder_blocking(start: Option<String>, _owner: isize) -> PickResult {
    let mut zenity = Command::new("zenity");
    zenity.args([
        "--file-selection",
        "--directory",
        &format!("--title={PICKER_TITLE}"),
    ]);
    if let Some(s) = &start {
        zenity.arg(format!("--filename={s}/"));
    }
    match run_picker(zenity) {
        Ok(Some(p)) => Ok(Some(p)),
        // zenity missing or cancelled: kdialog gets a try, then a clear message.
        _ => {
            let mut kdialog = Command::new("kdialog");
            kdialog.args(["--getexistingdirectory", start.as_deref().unwrap_or(".")]);
            run_picker(kdialog).map_err(|_| {
                "No folder dialog is available (zenity or kdialog). Type the folder path instead."
                    .to_string()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_picker_output() {
        assert_eq!(
            clean_picker_output(b"\xef\xbb\xbfC:\\a b\r\n"),
            Some("C:\\a b".into())
        );
        assert_eq!(clean_picker_output(b" \r\n"), None);
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn script_takes_values_from_environment_only() {
        assert!(WINDOWS_PICKER_SCRIPT.contains("$env:XPANDO_PICK_START"));
        assert!(!encoded_picker_script().is_empty());
    }
}

const SAMPLES_MARKER: &str = "xpando-samples-added";
const SAMPLES: &str = "matches:
  # Sample phrases to try. Edit or delete them any time.
  - trigger: \":hello\"
    replace: \"Hello, and thanks for getting in touch!\"
    xpando_folder: \"Samples\"

  - trigger: \":sig\"
    replace: \"Best regards,\nYour Name\"
    xpando_folder: \"Samples\"

  - trigger: \":date\"
    replace: \"{{today}}\"
    vars:
      - name: today
        type: date
        params:
          format: \"%B %d, %Y\"
    xpando_folder: \"Samples\"
";

/// On a first run with nothing to import, adds a few sample phrases in a sample folder (once).
/// Returns whether it did.
#[tauri::command]
pub fn seed_samples(app: AppHandle) -> Result<bool, String> {
    if import_offer(app.clone(), false)?.is_some() {
        return Ok(false);
    }
    let dir = data_dir(&app)?;
    if dir.join(SAMPLES_MARKER).exists() || datadir::has_data(&dir) {
        return Ok(false);
    }
    std::fs::write(dir.join("match").join("base.yml"), SAMPLES)
        .map_err(|e| format!("Could not add the sample phrases: {e}"))?;
    std::fs::write(dir.join(SAMPLES_MARKER), "").map_err(|e| e.to_string())?;
    Ok(true)
}
