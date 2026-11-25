//! Commands for the app picker: which programs are running right now, which are installed, and
//! a file dialog to choose a program by hand.
//!
//! On Windows this runs short PowerShell scripts (hidden window, a time limit, the script passed
//! with `-EncodedCommand` so no quoting issues, never any user text inside the script). On macOS
//! and Linux "running now" uses `ps`. Nothing here changes any file. Everything a script prints
//! is treated as untrusted text: names are checked and cleaned before they reach the UI.
//!
//! Only the Windows paths have been exercised on a real machine; the macOS and Linux paths are
//! written but untested (see docs/wiki/2.5-Folders-and-Apps.md).

use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use xpando_core::serde::Serialize;

/// One program the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(crate = "xpando_core::serde")]
pub struct Program {
    /// The program's file name, for example `WINWORD.EXE`.
    pub exe: String,
    /// A friendly name when one is known (the file description or shortcut name), else the
    /// file name without `.exe`.
    pub name: String,
}

const MAX_EXE_CHARS: usize = 64;
const MAX_NAME_CHARS: usize = 80;

/// The characters xpando's folder filter accepts in a program name.
fn exe_ok(exe: &str) -> bool {
    let stem = exe
        .strip_suffix(".exe")
        .or_else(|| exe.strip_suffix(".EXE"))
        .unwrap_or(exe);
    !stem.trim().is_empty()
        && exe.chars().count() <= MAX_EXE_CHARS
        && exe
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | '+' | '(' | ')'))
}

fn clean_name(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).collect();
    t.trim().chars().take(MAX_NAME_CHARS).collect()
}

fn stem(exe: &str) -> &str {
    let lower = exe.to_lowercase();
    if lower.ends_with(".exe") {
        &exe[..exe.len() - 4]
    } else {
        exe
    }
}

/// Parses `exe<TAB>name<TAB>path` lines into distinct programs (by lower-case file name),
/// sorted by friendly name. Lines that are malformed or whose file name could not be used in a
/// folder filter are dropped; `skip` removes programs that are not useful choices.
fn parse_lines(text: &str, skip: impl Fn(&str, &str) -> bool) -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let mut parts = line.split('\t');
        let exe = parts.next().unwrap_or("").trim();
        let name = clean_name(parts.next().unwrap_or(""));
        if !exe_ok(exe) {
            continue;
        }
        let name = if name.is_empty() {
            stem(exe).to_string()
        } else {
            name
        };
        if skip(exe, &name) {
            continue;
        }
        match out.iter_mut().find(|p| p.exe.eq_ignore_ascii_case(exe)) {
            Some(p) => {
                // Keep the first friendly name, unless it was only the file name.
                if p.name == stem(&p.exe) && name != stem(exe) {
                    p.name = name;
                }
            }
            None => out.push(Program {
                exe: exe.to_string(),
                name,
            }),
        }
    }
    out.sort_by_key(|p| (p.name.to_lowercase(), p.exe.to_lowercase()));
    out
}

fn skip_running(exe: &str, _name: &str) -> bool {
    // UWP windows are hosted by this program; the real app is not visible from outside.
    exe.eq_ignore_ascii_case("ApplicationFrameHost.exe")
}

fn skip_installed(exe: &str, name: &str) -> bool {
    let e = exe.to_lowercase();
    let n = name.to_lowercase();
    e.starts_with("unins")
        || e.contains("uninstall")
        || n.contains("uninstall")
        || e.starts_with("setup")
        || e.contains("installer")
}

// ---------------------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------------------

/// Programs with a visible window. Output: `exe<TAB>description<TAB>path`.
#[cfg(any(windows, test))]
const RUNNING_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle } | ForEach-Object {
  $path = $null; $desc = $null
  try { $path = $_.MainModule.FileName } catch {}
  try { $desc = $_.MainModule.FileVersionInfo.FileDescription } catch {}
  if ($path) { $exe = [System.IO.Path]::GetFileName($path) } else { $exe = $_.ProcessName + '.exe' }
  ($exe + "`t" + $desc + "`t" + $path) -replace "[\r\n]", ' '
}
"#;

/// Programs found through Start Menu shortcuts and the App Paths registry keys.
/// Output: `exe<TAB>name<TAB>path`.
#[cfg(any(windows, test))]
const INSTALLED_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
function Emit($exe, $name, $path) {
  if ($exe) { ($exe + "`t" + $name + "`t" + $path) -replace "[\r\n]", ' ' }
}
$sh = New-Object -ComObject WScript.Shell
$roots = @("$env:ProgramData\Microsoft\Windows\Start Menu\Programs", "$env:APPDATA\Microsoft\Windows\Start Menu\Programs")
foreach ($r in $roots) {
  if (Test-Path $r) {
    Get-ChildItem -Path $r -Filter *.lnk -Recurse -File | ForEach-Object {
      $t = $sh.CreateShortcut($_.FullName).TargetPath
      if ($t -and $t.ToLower().EndsWith('.exe') -and (Test-Path $t)) {
        Emit ([System.IO.Path]::GetFileName($t)) $_.BaseName $t
      }
    }
  }
}
foreach ($hive in 'HKLM:', 'HKCU:') {
  Get-ChildItem "$hive\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths" | ForEach-Object {
    $exe = $_.PSChildName
    if ($exe -and $exe.ToLower().EndsWith('.exe')) {
      $p = (Get-ItemProperty $_.PSPath).'(default)'
      if ($p) { $p = $p.Trim('"') }
      $desc = $null
      if ($p -and (Test-Path $p)) { try { $desc = (Get-Item $p).VersionInfo.FileDescription } catch {} }
      Emit $exe $desc $p
    }
  }
}
"#;

#[cfg(windows)]
fn run_powershell(script: &str, limit: Duration) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &crate::data::encode_powershell(script),
    ])
    .stdin(Stdio::null())
    .creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let (code, out, err) = crate::data::run_with_timeout(cmd, limit)?;
    if code != 0 {
        let detail = String::from_utf8_lossy(&err);
        let detail = detail.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        return Err(format!("PowerShell could not list the programs. {detail}")
            .trim()
            .to_string());
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(windows)]
fn running_blocking() -> Result<Vec<Program>, String> {
    let text = run_powershell(RUNNING_SCRIPT, Duration::from_secs(20))?;
    Ok(parse_lines(&text, skip_running))
}

#[cfg(windows)]
fn installed_blocking() -> Result<Vec<Program>, String> {
    let text = run_powershell(INSTALLED_SCRIPT, Duration::from_secs(60))?;
    Ok(parse_lines(&text, skip_installed))
}

// ---------------------------------------------------------------------------------------
// macOS and Linux (not tested on those systems)
// ---------------------------------------------------------------------------------------

/// Turns `ps -o comm=` output into programs. On macOS only apps inside an `.app` bundle count
/// and the bundle name is the friendly name; on Linux every distinct process name is listed.
#[cfg(any(not(windows), test))]
fn parse_ps(text: &str, mac: bool) -> Vec<Program> {
    let mut lines = String::new();
    for raw in text.lines() {
        let path = raw.trim();
        if path.is_empty() {
            continue;
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        let friendly = if mac {
            let Some(app) = path.split('/').find(|s| s.ends_with(".app")) else {
                continue;
            };
            app.trim_end_matches(".app").to_string()
        } else {
            String::new()
        };
        lines.push_str(&format!("{file}\t{friendly}\t{path}\n"));
    }
    parse_lines(&lines, |_, _| false)
}

#[cfg(not(windows))]
fn running_blocking() -> Result<Vec<Program>, String> {
    let mut cmd = Command::new("ps");
    cmd.args(["-axo", "comm="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let out = cmd.output().map_err(|e| {
        format!("Could not list running programs ({e}). Type the program name instead.")
    })?;
    Ok(parse_ps(
        &String::from_utf8_lossy(&out.stdout),
        cfg!(target_os = "macos"),
    ))
}

#[cfg(not(windows))]
fn installed_blocking() -> Result<Vec<Program>, String> {
    Err("Listing installed programs is only available on Windows.".into())
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

/// Programs with a window open right now.
#[tauri::command]
pub async fn list_running_programs() -> Result<Vec<Program>, String> {
    tauri::async_runtime::spawn_blocking(running_blocking)
        .await
        .map_err(|e| e.to_string())?
}

static INSTALLED: OnceLock<Mutex<Option<Vec<Program>>>> = OnceLock::new();

/// Programs found in the Start Menu and the registry. The first call takes a few seconds; the
/// answer is kept until xpando closes unless `refresh` is true.
#[tauri::command]
pub async fn list_installed_programs(refresh: bool) -> Result<Vec<Program>, String> {
    let cache = INSTALLED.get_or_init(|| Mutex::new(None));
    if !refresh {
        if let Some(list) = cache.lock().map_err(|e| e.to_string())?.clone() {
            return Ok(list);
        }
    }
    let list = tauri::async_runtime::spawn_blocking(installed_blocking)
        .await
        .map_err(|e| e.to_string())??;
    *cache.lock().map_err(|e| e.to_string())? = Some(list.clone());
    Ok(list)
}

// ---------------------------------------------------------------------------------------
// Program icons (Windows only; other systems show the generic icon)
// ---------------------------------------------------------------------------------------

/// Finds each program named in the `XPANDO_ICON_EXES` environment variable (one file name per
/// line, already checked by xpando) among the running programs, the App Paths registry keys and
/// the Start Menu shortcuts, and prints `exe<TAB>base64 PNG` for each one whose icon it can read.
/// The names travel in the environment, never inside the script text.
#[cfg(any(windows, test))]
const ICON_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Drawing
$want = @($env:XPANDO_ICON_EXES -split "[\r\n]+" | Where-Object { $_ })
# Folders often name a program without ".exe" ("firefox"), so look it up by its file name.
$keys = @{}
foreach ($w in $want) { $k = $w.ToLower(); if (-not $k.EndsWith('.exe')) { $k = $k + '.exe' }; $keys[$w] = $k }
$paths = @{}
Get-Process | ForEach-Object {
  try {
    $p = $_.MainModule.FileName
    if ($p) { $k = [System.IO.Path]::GetFileName($p).ToLower(); if (-not $paths.ContainsKey($k)) { $paths[$k] = $p } }
  } catch {}
}
foreach ($hive in 'HKLM:', 'HKCU:') {
  Get-ChildItem "$hive\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths" | ForEach-Object {
    $k = $_.PSChildName.ToLower()
    if (-not $paths.ContainsKey($k)) {
      $p = (Get-ItemProperty $_.PSPath).'(default)'
      if ($p) { $p = $p.Trim('"'); if (Test-Path $p) { $paths[$k] = $p } }
    }
  }
}
$missing = @($want | Where-Object { -not $paths.ContainsKey($keys[$_]) })
if ($missing.Count -gt 0) {
  $sh = New-Object -ComObject WScript.Shell
  $roots = @("$env:ProgramData\Microsoft\Windows\Start Menu\Programs", "$env:APPDATA\Microsoft\Windows\Start Menu\Programs")
  foreach ($r in $roots) {
    if (Test-Path $r) {
      Get-ChildItem -Path $r -Filter *.lnk -Recurse -File | ForEach-Object {
        $t = $sh.CreateShortcut($_.FullName).TargetPath
        if ($t -and $t.ToLower().EndsWith('.exe') -and (Test-Path $t)) {
          $k = [System.IO.Path]::GetFileName($t).ToLower()
          if (-not $paths.ContainsKey($k)) { $paths[$k] = $t }
        }
      }
    }
  }
}
foreach ($w in $want) {
  $p = $null
  $p = $paths[$keys[$w]]
  if (-not $p) { continue }
  try {
    $bmp = [System.Drawing.Icon]::ExtractAssociatedIcon($p).ToBitmap()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $w + "`t" + [Convert]::ToBase64String($ms.ToArray())
  } catch {}
}
"#;

/// Largest icon text accepted from the script. Real icons are a few KB.
const MAX_ICON_CHARS: usize = 200_000;

/// Reads `exe<TAB>base64` lines into `(exe, data URL)` pairs. Only requested programs and plain
/// base64 text are accepted, so nothing else the script prints can reach the page.
#[cfg(any(windows, test))]
fn parse_icon_lines(text: &str, asked: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((exe, data)) = line.trim().split_once('\t') else {
            continue;
        };
        let data = data.trim();
        let Some(asked_name) = asked.iter().find(|a| a.eq_ignore_ascii_case(exe.trim())) else {
            continue;
        };
        let plain = !data.is_empty()
            && data.len() <= MAX_ICON_CHARS
            && data
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
        if plain {
            out.push((asked_name.clone(), format!("data:image/png;base64,{data}")));
        }
    }
    out
}

/// What was learned so far, by lower-case file name; `None` means "no icon found".
static ICONS: OnceLock<Mutex<std::collections::HashMap<String, Option<String>>>> = OnceLock::new();

#[cfg(windows)]
fn icons_blocking(exes: &[String]) -> Result<Vec<(String, String)>, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &crate::data::encode_powershell(ICON_SCRIPT),
    ])
    .env("XPANDO_ICON_EXES", exes.join("\n"))
    .stdin(Stdio::null())
    .creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let (code, out, _) = crate::data::run_with_timeout(cmd, Duration::from_secs(60))?;
    if code != 0 {
        return Err("Could not read the program icons.".into());
    }
    Ok(parse_icon_lines(&String::from_utf8_lossy(&out), exes))
}

#[cfg(not(windows))]
fn icons_blocking(_exes: &[String]) -> Result<Vec<(String, String)>, String> {
    Ok(Vec::new())
}

/// Small icons for the given program file names, as `data:` URLs. Programs without a findable
/// icon are left out of the answer. Each program is looked up once until xpando closes.
#[tauri::command]
pub async fn app_icons(
    exes: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let cache = ICONS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut wanted: Vec<String> = Vec::new();
    for e in exes {
        if exe_ok(&e) && !wanted.iter().any(|w| w.eq_ignore_ascii_case(&e)) {
            wanted.push(e);
        }
    }
    let todo: Vec<String> = {
        let known = cache.lock().map_err(|e| e.to_string())?;
        wanted
            .iter()
            .filter(|e| !known.contains_key(&e.to_lowercase()))
            .cloned()
            .collect()
    };
    if !todo.is_empty() {
        let found = {
            let todo = todo.clone();
            tauri::async_runtime::spawn_blocking(move || icons_blocking(&todo))
                .await
                .map_err(|e| e.to_string())??
        };
        let mut known = cache.lock().map_err(|e| e.to_string())?;
        for e in &todo {
            let hit = found
                .iter()
                .find(|(f, _)| f.eq_ignore_ascii_case(e))
                .map(|(_, d)| d.clone());
            known.insert(e.to_lowercase(), hit);
        }
    }
    let known = cache.lock().map_err(|e| e.to_string())?;
    Ok(wanted
        .into_iter()
        .filter_map(|e| {
            known
                .get(&e.to_lowercase())
                .cloned()
                .flatten()
                .map(|d| (e, d))
        })
        .collect())
}

/// Windows: the "Open" dialog for a program file. Prints `path<TAB>description` and exits 0;
/// exit code 2 is "cancelled". With `XPANDO_PICK_DRY` set it builds the dialog and exits 3
/// without showing it (used to check that the script compiles).
#[cfg(any(windows, test))]
const FILE_PICKER_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
public static class XpandoFilePicker {
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
  [DllImport("ole32.dll")] static extern void CoTaskMemFree(IntPtr p);

  public static int Pick(string title, long owner, bool dry) {
    var dlg = (IFileOpenDialog)new FileOpenDialogCls();
    // FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST
    dlg.SetOptions(0x40 | 0x800 | 0x1000);
    if (!string.IsNullOrEmpty(title)) dlg.SetTitle(title);
    // Two COMDLG_FILTERSPEC entries: Programs (*.exe) and All files.
    int w = IntPtr.Size;
    IntPtr spec = Marshal.AllocCoTaskMem(w * 4);
    Marshal.WriteIntPtr(spec, 0, Marshal.StringToCoTaskMemUni("Programs (*.exe)"));
    Marshal.WriteIntPtr(spec, w, Marshal.StringToCoTaskMemUni("*.exe"));
    Marshal.WriteIntPtr(spec, 2 * w, Marshal.StringToCoTaskMemUni("All files (*.*)"));
    Marshal.WriteIntPtr(spec, 3 * w, Marshal.StringToCoTaskMemUni("*.*"));
    dlg.SetFileTypes(2, spec);
    dlg.SetFileTypeIndex(1);
    if (dry) return 3;
    int hr = dlg.Show(new IntPtr(owner));
    if (hr == unchecked((int)0x800704C7)) return 2;
    if (hr != 0) Marshal.ThrowExceptionForHR(hr);
    IShellItem item;
    dlg.GetResult(out item);
    IntPtr p;
    item.GetDisplayName(0x80058000, out p);
    string path = Marshal.PtrToStringUni(p);
    CoTaskMemFree(p);
    string desc = "";
    try { desc = FileVersionInfo.GetVersionInfo(path).FileDescription ?? ""; } catch {}
    var bytes = new UTF8Encoding(false).GetBytes(path + "\t" + desc.Replace("\t", " ").Replace("\r", " ").Replace("\n", " "));
    using (var o = Console.OpenStandardOutput()) { o.Write(bytes, 0, bytes.Length); }
    return 0;
  }
}
'@
$owner = [long]0
if ($env:XPANDO_PICK_OWNER) { [void][long]::TryParse($env:XPANDO_PICK_OWNER, [ref]$owner) }
exit ([XpandoFilePicker]::Pick($env:XPANDO_PICK_TITLE, $owner, [bool]$env:XPANDO_PICK_DRY))
"#;

const PICK_TITLE: &str = "Choose a program";
const PICK_TIMEOUT: Duration = Duration::from_secs(300);

/// Turns the picker's `path<TAB>description` output into a program. Only `.exe` files are
/// accepted on Windows.
#[cfg(any(windows, test))]
fn program_from_pick(out: &str) -> Result<Program, String> {
    let line = out.trim_start_matches('\u{feff}').trim();
    let mut parts = line.splitn(2, '\t');
    let path = parts.next().unwrap_or("").trim();
    let desc = clean_name(parts.next().unwrap_or(""));
    let file = path.rsplit(['\\', '/']).next().unwrap_or("");
    if !file.to_lowercase().ends_with(".exe") {
        return Err("Choose a program file (it ends in .exe).".into());
    }
    if !exe_ok(file) {
        return Err(format!(
            "xpando cannot use the file name \"{file}\" (it has characters a folder filter does not accept)."
        ));
    }
    Ok(Program {
        exe: file.to_string(),
        name: if desc.is_empty() {
            stem(file).to_string()
        } else {
            desc
        },
    })
}

#[cfg(windows)]
fn pick_blocking(owner: isize, dry: bool) -> Result<Option<Program>, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &crate::data::encode_powershell(FILE_PICKER_SCRIPT),
    ])
    .env("XPANDO_PICK_TITLE", PICK_TITLE)
    .env("XPANDO_PICK_OWNER", owner.to_string())
    .stdin(Stdio::null())
    .creation_flags(0x0800_0000); // CREATE_NO_WINDOW (the dialog itself still shows)
    if dry {
        cmd.env("XPANDO_PICK_DRY", "1");
    }
    let (code, out, err) = crate::data::run_with_timeout(cmd, PICK_TIMEOUT)
        .map_err(|e| format!("The file dialog could not be opened ({e})."))?;
    match code {
        0 => program_from_pick(&String::from_utf8_lossy(&out)).map(Some),
        2 => Ok(None),
        3 if dry => Ok(None),
        _ => {
            let detail = String::from_utf8_lossy(&err);
            let detail = detail.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            Err(format!("The file dialog could not be opened. {detail}")
                .trim()
                .to_string())
        }
    }
}

#[cfg(target_os = "macos")]
fn pick_blocking(_owner: isize, _dry: bool) -> Result<Option<Program>, String> {
    Err("Browsing for a program is not available on macOS yet. Pick it from \"Open now\" or type its name in Advanced mode.".into())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn pick_blocking(_owner: isize, _dry: bool) -> Result<Option<Program>, String> {
    let out = Command::new("zenity")
        .args(["--file-selection", &format!("--title={PICK_TITLE}")])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| {
            "No file dialog is available (zenity). Pick the program from \"Open now\" or type its name in Advanced mode."
                .to_string()
        })?;
    if !out.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let file = path.rsplit('/').next().unwrap_or("");
    if !exe_ok(file) {
        return Err("xpando cannot use that file name.".into());
    }
    Ok(Some(Program {
        exe: file.to_string(),
        name: file.to_string(),
    }))
}

/// Opens the system file dialog and returns the chosen program, or `None` when cancelled.
#[tauri::command]
pub async fn pick_program(window: tauri::WebviewWindow) -> Result<Option<Program>, String> {
    #[cfg(windows)]
    let owner = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    #[cfg(not(windows))]
    let owner = {
        let _ = &window;
        0isize
    };
    tauri::async_runtime::spawn_blocking(move || pick_blocking(owner, false))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_lines_keep_only_asked_programs_and_plain_base64() {
        let asked = vec!["Word.exe".to_string(), "x.exe".to_string()];
        let text = "word.EXE	QUJD+/=
other.exe	QUJD
x.exe	<script>
x.exe	
nonsense
";
        assert_eq!(
            parse_icon_lines(text, &asked),
            vec![(
                "Word.exe".to_string(),
                "data:image/png;base64,QUJD+/=".to_string()
            )]
        );
    }

    use super::*;

    #[test]
    fn parses_distinct_programs_and_drops_bad_lines() {
        let text = "\u{feff}WINWORD.EXE\tMicrosoft Word\tC:\\a\\WINWORD.EXE\n\
                    winword.exe\t\t\n\
                    notepad.exe\t\tC:\\Windows\\notepad.exe\n\
                    bad;name.exe\tx\t\n\
                    \n\
                    ApplicationFrameHost.exe\tApplication Frame Host\t\n\
                    ../evil.exe\t\t\n";
        let got = parse_lines(text, skip_running);
        assert_eq!(
            got,
            vec![
                Program {
                    exe: "WINWORD.EXE".into(),
                    name: "Microsoft Word".into()
                },
                Program {
                    exe: "notepad.exe".into(),
                    name: "notepad".into()
                },
            ]
        );
    }

    #[test]
    fn a_later_friendly_name_replaces_a_bare_file_name() {
        let got = parse_lines("excel.exe\t\t\nEXCEL.EXE\tMicrosoft Excel\t\n", |_, _| {
            false
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Microsoft Excel");
    }

    #[test]
    fn installed_list_skips_uninstallers() {
        let text = "unins000.exe\tUninstall Foo\t\nfoo.exe\tFoo\t\nsetup.exe\tSetup\t\n";
        let got = parse_lines(text, skip_installed);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].exe, "foo.exe");
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  A\u{7}B\n"), "AB");
        assert_eq!(clean_name(&"x".repeat(200)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn ps_output_becomes_programs() {
        let mac = "/Applications/Safari.app/Contents/MacOS/Safari\n/usr/sbin/cfprefsd\n";
        let got = parse_ps(mac, true);
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].exe.as_str(), got[0].name.as_str()),
            ("Safari", "Safari")
        );
        let linux = "bash\nfirefox\nfirefox\n";
        assert_eq!(parse_ps(linux, false).len(), 2);
    }

    #[test]
    fn picked_file_must_be_a_program() {
        let p = program_from_pick("C:\\Program Files\\Foo\\foo.exe\tFoo Editor\r\n").unwrap();
        assert_eq!((p.exe.as_str(), p.name.as_str()), ("foo.exe", "Foo Editor"));
        assert!(program_from_pick("C:\\x\\readme.txt\t").is_err());
        assert!(program_from_pick("C:\\x\\a;b.exe\t").is_err());
        let bare = program_from_pick("D:/tools/bar.EXE").unwrap();
        assert_eq!(bare.name, "bar");
    }

    /// Builds the real dialog (COM setup, file filters) without showing it.
    #[cfg(windows)]
    #[test]
    fn file_dialog_script_compiles_and_sets_up_on_windows() {
        assert_eq!(pick_blocking(0, true), Ok(None));
    }

    /// Runs the real scripts. What they find depends on the machine, so only success is checked.
    #[cfg(windows)]
    #[test]
    fn listing_scripts_run_on_windows() {
        let running = running_blocking().expect("running programs");
        let installed = installed_blocking().expect("installed programs");
        eprintln!("running: {}, installed: {}", running.len(), installed.len());
        assert!(installed.iter().all(|p| exe_ok(&p.exe)));
    }

    #[test]
    fn scripts_take_values_from_the_environment_only() {
        assert!(FILE_PICKER_SCRIPT.contains("$env:XPANDO_PICK_TITLE"));
        assert!(!RUNNING_SCRIPT.contains("{0}"));
        assert!(INSTALLED_SCRIPT.contains("App Paths"));
    }
}
