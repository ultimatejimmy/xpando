//! Opens the xpando desktop app from the engine (tray menu, search box).

/// Opens the xpando app, which is installed next to this engine. When it is already running
/// (Windows) its window is brought to the front instead of starting a second copy.
pub fn open_xpando_app() {
    use log::{error, warn};
    use std::process::{Command, Stdio};

    let exe_name = if cfg!(windows) {
        "xpando-app.exe"
    } else {
        "xpando-app"
    };
    let Some(app) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe_name)))
        .filter(|p| p.is_file())
    else {
        warn!("unable to find the xpando app next to the engine");
        return;
    };

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const NO_WINDOW: u32 = 0x0800_0000;
        let running = Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {exe_name}"), "/NH"])
            .creation_flags(NO_WINDOW)
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(exe_name));
        if running {
            let _ = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    "(New-Object -ComObject WScript.Shell).AppActivate('xpando') | Out-Null",
                ])
                .creation_flags(NO_WINDOW)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            return;
        }
    }

    if let Err(err) = Command::new(&app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        error!("unable to open the xpando app: {err}");
    }
}
