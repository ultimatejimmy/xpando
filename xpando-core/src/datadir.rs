//! Where xpando keeps its data.
//!
//! xpando owns a data folder holding `match/` and `config/`. By default it is a folder named
//! `xpando` in the OS's app-config location, so nothing needs to be set up. The user may
//! choose another folder (for example inside OneDrive); that choice lives in a small
//! `settings.yml` in a *different* folder (the app's own config folder), because it has to be
//! readable before the data folder is known and must not move with the data.
//!
//! Nothing here falls back silently: if the chosen folder cannot be reached, callers get a
//! [`DirStatus`] to show, never a different folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::copytree::{self, CopyReport};
use crate::syncsafe::{find_conflicted_copies, ConflictedCopy};

/// The folder name used under the OS config location.
pub const APP_DIR_NAME: &str = "xpando";
const SETTINGS_FILE: &str = "settings.yml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// The parts of the process environment that decide where folders are. Passed in so that
/// tests use made-up paths and never look at the real user folders.
#[derive(Debug, Clone)]
pub struct Env {
    pub os: Os,
    /// `%APPDATA%` (Windows).
    pub appdata: Option<PathBuf>,
    /// The user's home folder.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME` (Linux).
    pub xdg_config_home: Option<PathBuf>,
    /// `$ESPANSO_CONFIG_DIR`, when the user set one for their espanso.
    pub espanso_config_dir: Option<PathBuf>,
}

fn var_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

impl Env {
    /// Reads the real environment of this process.
    pub fn current() -> Env {
        let os = Os::current();
        Env {
            os,
            appdata: var_path("APPDATA"),
            home: if os == Os::Windows {
                var_path("USERPROFILE").or_else(|| var_path("HOME"))
            } else {
                var_path("HOME")
            },
            xdg_config_home: var_path("XDG_CONFIG_HOME"),
            espanso_config_dir: var_path("ESPANSO_CONFIG_DIR"),
        }
    }

    /// The OS's per-user config location: `%APPDATA%`, `~/Library/Application Support`
    /// or `$XDG_CONFIG_HOME` (default `~/.config`).
    pub fn config_base(&self) -> Option<PathBuf> {
        match self.os {
            Os::Windows => self.appdata.clone(),
            Os::MacOs => self
                .home
                .as_ref()
                .map(|h| h.join("Library").join("Application Support")),
            Os::Linux => self
                .xdg_config_home
                .clone()
                .filter(|p| p.is_absolute())
                .or_else(|| self.home.as_ref().map(|h| h.join(".config"))),
        }
    }
}

/// The default data folder: `<config base>/xpando`.
pub fn default_data_dir(env: &Env) -> Result<PathBuf, String> {
    env.config_base()
        .map(|b| b.join(APP_DIR_NAME))
        .ok_or_else(|| {
            "Could not find your user folder, so xpando cannot pick a default folder for your data."
                .to_string()
        })
}

/// What the user chose, stored apart from the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The chosen data folder; `None` means the default.
    #[serde(default)]
    pub data_dir: Option<String>,
    /// The first-run import offer was answered (imported, or "Not now" was chosen to stop asking).
    #[serde(default)]
    pub import_handled: bool,
    /// Which releases the updater offers: `stable` (default) or `beta`.
    #[serde(default)]
    pub update_channel: crate::update::Channel,
    /// Look for a new version once a day.
    #[serde(default = "default_true")]
    pub auto_check: bool,
    /// When the last successful update check finished, in seconds since 1970.
    #[serde(default)]
    pub last_update_check: Option<u64>,
    /// A version the user chose "Skip this version" for (human form, e.g. `2026.11.23`).
    #[serde(default)]
    pub skipped_version: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            data_dir: None,
            import_handled: false,
            update_channel: crate::update::Channel::default(),
            auto_check: true,
            last_update_check: None,
            skipped_version: None,
        }
    }
}

/// Reads `<settings_dir>/settings.yml`. A missing file means the defaults. A file that exists
/// but cannot be read or understood is an error, so a broken file never silently switches the
/// app to the default folder.
pub fn load_settings(settings_dir: &Path) -> Result<Settings, String> {
    let path = settings_dir.join(SETTINGS_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => {
            return Err(format!(
                "Could not read the settings file {}: {e}",
                path.display()
            ))
        }
    };
    if text.trim().is_empty() {
        return Ok(Settings::default());
    }
    serde_norway::from_str(&text).map_err(|e| {
        format!(
            "The settings file {} could not be understood ({e}). Fix or delete it to continue.",
            path.display()
        )
    })
}

/// Writes the settings atomically (temporary file, then rename).
pub fn save_settings(settings_dir: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(settings_dir)
        .map_err(|e| format!("Could not create the settings folder: {e}"))?;
    let text = serde_norway::to_string(settings).map_err(|e| e.to_string())?;
    let path = settings_dir.join(SETTINGS_FILE);
    let tmp = settings_dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, text).map_err(|e| format!("Could not save the settings: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not save the settings: {e}")
    })
}

/// The data folder in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataDir {
    pub path: PathBuf,
    /// Whether this is the default location (the user has not changed it).
    pub is_default: bool,
}

/// Works out the data folder from the settings. A chosen folder must be an absolute path.
pub fn resolve(env: &Env, settings: &Settings) -> Result<DataDir, String> {
    match settings.data_dir.as_deref().map(str::trim) {
        Some(chosen) if !chosen.is_empty() => {
            let path = PathBuf::from(chosen);
            if !path.is_absolute() {
                return Err(format!(
                    "The data folder in the settings ({chosen}) is not a full path."
                ));
            }
            Ok(DataDir {
                path,
                is_default: false,
            })
        }
        _ => Ok(DataDir {
            path: default_data_dir(env)?,
            is_default: true,
        }),
    }
}

/// Whether a folder can be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum DirStatus {
    Ok,
    /// Nothing at this path (a disk not connected, a cloud folder not synced, a typo).
    Missing,
    /// The path is a file.
    NotADirectory,
    /// It exists but cannot be listed (permissions, a cloud drive that is signed out).
    Unreadable(String),
}

/// Checks a folder without creating anything.
pub fn check_dir(path: &Path) -> DirStatus {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => match fs::read_dir(path) {
            Ok(_) => DirStatus::Ok,
            Err(e) => DirStatus::Unreadable(e.to_string()),
        },
        Ok(_) => DirStatus::NotADirectory,
        Err(e) if e.kind() == io::ErrorKind::NotFound => DirStatus::Missing,
        Err(e) => DirStatus::Unreadable(e.to_string()),
    }
}

/// Whether the folder exists and can be listed.
pub fn is_reachable(path: &Path) -> bool {
    check_dir(path) == DirStatus::Ok
}

/// Whether files can be created in the folder (checked by creating and removing a tiny file).
pub fn is_writable(path: &Path) -> bool {
    let probe = path.join(format!(".xpando-write-test-{}", std::process::id()));
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// A sentence for the user explaining a status.
pub fn describe_status(path: &Path, status: &DirStatus) -> String {
    let shown = path.display();
    match status {
        DirStatus::Ok => format!("The folder {shown} is available."),
        DirStatus::Missing => format!(
            "The folder {shown} cannot be found. If it is on a cloud drive or another disk, check that it is connected and signed in. xpando will not use another folder instead."
        ),
        DirStatus::NotADirectory => format!("{shown} is a file, not a folder."),
        DirStatus::Unreadable(why) => format!(
            "The folder {shown} cannot be opened ({why}). If it is on a cloud drive, check that it is signed in and available offline."
        ),
    }
}

/// Makes sure `<dir>/match` and `<dir>/config` exist. Creating the folder itself is allowed
/// only when `create_root` is set (the default folder); a chosen folder that has gone missing
/// is reported, not recreated, because that is usually an unplugged disk or a signed-out
/// cloud drive.
pub fn ensure_layout(dir: &Path, create_root: bool) -> Result<(), String> {
    if !create_root {
        let status = check_dir(dir);
        if status != DirStatus::Ok {
            return Err(describe_status(dir, &status));
        }
    }
    for sub in ["match", "config"] {
        fs::create_dir_all(dir.join(sub))
            .map_err(|e| format!("Could not create {}: {e}", dir.join(sub).display()))?;
    }
    Ok(())
}

/// Resolves the data folder and makes it ready for use (see [`ensure_layout`]).
pub fn prepare(env: &Env, settings: &Settings) -> Result<DataDir, String> {
    let data = resolve(env, settings)?;
    ensure_layout(&data.path, data.is_default)?;
    Ok(data)
}

/// Whether `dir` already holds xpando or espanso data (any `*.yml` under `match` or `config`).
pub fn has_data(dir: &Path) -> bool {
    let (mut skipped, mut failed) = (Vec::new(), Vec::new());
    !copytree::walk(dir, &["match", "config"], true, &mut skipped, &mut failed).is_empty()
}

/// Copies everything in the data folder `from` into `to`: `match/`, `config/` and any other
/// files (such as backups and folder metadata). Copy only: `from` is not touched, files that
/// already exist in `to` are kept as they are (and listed), and sync conflict copies are left
/// out (and listed). Refuses the same folder or folders inside each other.
pub fn copy_data(from: &Path, to: &Path) -> Result<CopyReport, String> {
    let status = check_dir(from);
    if status != DirStatus::Ok {
        return Err(describe_status(from, &status));
    }
    copytree::check_not_nested(from, to)?;
    let to_status = check_dir(to);
    if to_status != DirStatus::Ok {
        return Err(describe_status(to, &to_status));
    }
    if !is_writable(to) {
        return Err(format!("xpando cannot save files in {}.", to.display()));
    }
    let mut report = CopyReport::default();
    let found = copytree::walk(from, &[], false, &mut report.skipped, &mut report.failed);
    copytree::copy_found(&found, from, to, &mut report);
    Ok(report)
}

/// Where the data is and how it is doing, for the Settings view and the warning banners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataInfo {
    pub path: String,
    pub is_default: bool,
    pub default_path: Option<String>,
    pub status: DirStatus,
    /// A sentence for the user when the folder is not usable.
    pub problem: Option<String>,
    /// Files that look like sync conflict copies.
    pub conflicts: Vec<ConflictedCopy>,
}

/// Reports on the data folder. The default folder is created if missing; a chosen folder that
/// is missing is reported as a problem and left alone.
pub fn data_info(env: &Env, settings: &Settings) -> Result<DataInfo, String> {
    let data = resolve(env, settings)?;
    if data.is_default {
        ensure_layout(&data.path, true)?;
    }
    let status = check_dir(&data.path);
    let reachable = status == DirStatus::Ok;
    if reachable {
        ensure_layout(&data.path, false)?;
    }
    Ok(DataInfo {
        problem: (!reachable).then(|| describe_status(&data.path, &status)),
        conflicts: if reachable {
            find_conflicted_copies(&data.path).unwrap_or_default()
        } else {
            Vec::new()
        },
        default_path: default_data_dir(env)
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        path: data.path.to_string_lossy().into_owned(),
        is_default: data.is_default,
        status,
    })
}

/// The answer to "can I use this folder?" for a folder the user typed or picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathCheck {
    pub path: String,
    pub ok: bool,
    /// A sentence for the user when `ok` is false.
    pub problem: Option<String>,
    /// The folder does not exist yet and would be created (its parent exists).
    pub will_create: bool,
    /// The folder already holds phrases (for example from another computer).
    pub has_data: bool,
}

fn refuse(path: &Path, problem: impl Into<String>) -> PathCheck {
    PathCheck {
        path: path.to_string_lossy().into_owned(),
        ok: false,
        problem: Some(problem.into()),
        will_create: false,
        has_data: false,
    }
}

/// Checks a folder chosen as the new data folder, without creating or changing anything.
/// `current` is the folder in use now.
pub fn check_choice(current: &Path, chosen: &str) -> PathCheck {
    let path = PathBuf::from(chosen.trim());
    if chosen.trim().is_empty() {
        return refuse(&path, "Type or choose a folder.");
    }
    if !path.is_absolute() {
        return refuse(
            &path,
            "Use the full path of the folder, for example C:\\Users\\you\\OneDrive\\xpando.",
        );
    }
    if path == current {
        return refuse(&path, "That is the folder xpando already uses.");
    }
    let status = check_dir(&path);
    let mut will_create = false;
    match &status {
        DirStatus::Ok => {}
        DirStatus::Missing => {
            // Only the last folder may be created, so a mistyped path or a missing disk is
            // reported instead of building a long path somewhere unexpected.
            match path.parent().map(check_dir) {
                Some(DirStatus::Ok) => will_create = true,
                _ => return refuse(&path, describe_status(&path, &status)),
            }
        }
        other => return refuse(&path, describe_status(&path, other)),
    }
    if !will_create && !is_writable(&path) {
        return refuse(&path, "xpando cannot save files in that folder.");
    }
    if copytree::check_not_nested(current, &path).is_err() {
        return refuse(
            &path,
            "Choose a folder that is separate from the one xpando uses now: not inside it, and not one that contains it.",
        );
    }
    PathCheck {
        path: path.to_string_lossy().into_owned(),
        ok: true,
        problem: None,
        will_create,
        has_data: !will_create && has_data(&path),
    }
}

/// The outcome of [`change_data_dir`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeResult {
    pub path: String,
    /// False when the copy had problems; the folder in use did not change.
    pub switched: bool,
    pub report: Option<CopyReport>,
}

/// Starts using another data folder (`None` goes back to the default) and saves that choice in
/// `settings_dir`. When `copy` is set the current data is copied there first (copy only; the
/// old folder stays as it is and existing files in the new one are kept). If the copy has
/// failures the folder is NOT switched, so nothing is half-moved.
pub fn change_data_dir(
    env: &Env,
    settings_dir: &Path,
    chosen: Option<&str>,
    copy: bool,
) -> Result<ChangeResult, String> {
    let mut settings = load_settings(settings_dir)?;
    let current = resolve(env, &settings)?.path;
    let (target, store) = match chosen.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => {
            let check = check_choice(&current, p);
            if !check.ok {
                return Err(check.problem.unwrap_or_default());
            }
            (PathBuf::from(&check.path), Some(check.path))
        }
        None => (default_data_dir(env)?, None),
    };
    if target == current {
        return Err("That is the folder xpando already uses.".into());
    }
    if check_dir(&target) == DirStatus::Missing {
        // `check_choice` already required the parent to exist; the default folder may need
        // its parents too.
        let made = if store.is_some() {
            fs::create_dir(&target)
        } else {
            fs::create_dir_all(&target)
        };
        made.map_err(|e| format!("Could not create the folder: {e}"))?;
    }
    let mut report = None;
    if copy {
        let r = copy_data(&current, &target)?;
        let clean = r.failed.is_empty();
        report = Some(r);
        if !clean {
            return Ok(ChangeResult {
                path: target.to_string_lossy().into_owned(),
                switched: false,
                report,
            });
        }
    }
    ensure_layout(&target, false)?;
    settings.data_dir = store;
    save_settings(settings_dir, &settings)?;
    Ok(ChangeResult {
        path: target.to_string_lossy().into_owned(),
        switched: true,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xpando-datadir-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn env(os: Os) -> Env {
        let root = Path::new(if cfg!(windows) { "C:\\fake" } else { "/fake" });
        Env {
            os,
            appdata: Some(root.join("AppData")),
            home: Some(root.join("home")),
            xdg_config_home: None,
            espanso_config_dir: None,
        }
    }

    #[test]
    fn default_folder_per_os() {
        let w = env(Os::Windows);
        assert_eq!(
            default_data_dir(&w).unwrap(),
            w.appdata.clone().unwrap().join("xpando")
        );
        let m = env(Os::MacOs);
        assert_eq!(
            default_data_dir(&m).unwrap(),
            m.home
                .clone()
                .unwrap()
                .join("Library")
                .join("Application Support")
                .join("xpando")
        );
        let l = env(Os::Linux);
        assert_eq!(
            default_data_dir(&l).unwrap(),
            l.home.clone().unwrap().join(".config").join("xpando")
        );
    }

    #[test]
    fn linux_prefers_an_absolute_xdg_config_home() {
        let mut l = env(Os::Linux);
        let xdg = l.home.clone().unwrap().join("cfg");
        l.xdg_config_home = Some(xdg.clone());
        assert_eq!(default_data_dir(&l).unwrap(), xdg.join("xpando"));
        l.xdg_config_home = Some(PathBuf::from("relative/cfg"));
        assert_eq!(
            default_data_dir(&l).unwrap(),
            l.home.clone().unwrap().join(".config").join("xpando")
        );
    }

    #[test]
    fn missing_user_folders_are_an_error_not_a_guess() {
        let mut w = env(Os::Windows);
        w.appdata = None;
        assert!(default_data_dir(&w).is_err());
        let mut l = env(Os::Linux);
        l.home = None;
        assert!(default_data_dir(&l).is_err());
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let dir = temp_dir("settings");
        assert_eq!(load_settings(&dir).unwrap(), Settings::default());
        let s = Settings {
            data_dir: Some("D:\\Cloud\\xpando".into()),
            import_handled: true,
            ..Default::default()
        };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir).unwrap(), s);
        assert!(!dir.join("settings.yml.tmp").exists());

        // Saving to a folder that does not exist yet creates it.
        let nested = dir.join("a").join("b");
        save_settings(&nested, &Settings::default()).unwrap();
        assert_eq!(load_settings(&nested).unwrap(), Settings::default());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn update_settings_default_round_trip_and_survive_a_folder_change() {
        use crate::update::Channel;
        let dir = temp_dir("settings-update");
        let d = load_settings(&dir).unwrap();
        assert_eq!(d.update_channel, Channel::Stable);
        assert!(d.auto_check);
        assert_eq!(
            (d.last_update_check, d.skipped_version.clone()),
            (None, None)
        );

        // An old file with only the data folder keeps the update defaults.
        fs::write(
            dir.join("settings.yml"),
            "data_dir: /x
",
        )
        .unwrap();
        let old = load_settings(&dir).unwrap();
        assert!(old.auto_check);
        assert_eq!(old.update_channel, Channel::Stable);

        let s = Settings {
            update_channel: Channel::Beta,
            auto_check: false,
            last_update_check: Some(1_700_000_000),
            skipped_version: Some("2026.11.23".into()),
            ..Default::default()
        };
        save_settings(&dir, &s).unwrap();
        let text = fs::read_to_string(dir.join("settings.yml")).unwrap();
        assert!(text.contains("update_channel: beta"), "{text}");
        assert!(text.contains("auto_check: false"), "{text}");
        assert_eq!(load_settings(&dir).unwrap(), s);

        // A channel name from a newer or hand-edited file falls back to stable, not an error.
        fs::write(
            dir.join("settings.yml"),
            "update_channel: nightly
",
        )
        .unwrap();
        assert_eq!(load_settings(&dir).unwrap().update_channel, Channel::Stable);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_settings_file_means_defaults_and_unknown_keys_are_ignored() {
        let dir = temp_dir("settings-lenient");
        fs::write(dir.join("settings.yml"), "  \n").unwrap();
        assert_eq!(load_settings(&dir).unwrap(), Settings::default());
        fs::write(
            dir.join("settings.yml"),
            "future_option: 3\ndata_dir: /x/y\n",
        )
        .unwrap();
        assert_eq!(
            load_settings(&dir).unwrap().data_dir.as_deref(),
            Some("/x/y")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_settings_file_is_an_error_not_a_silent_default() {
        let dir = temp_dir("settings-bad");
        fs::write(dir.join("settings.yml"), "data_dir: [unclosed").unwrap();
        let err = load_settings(&dir).unwrap_err();
        assert!(err.contains("settings.yml"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_uses_default_or_the_chosen_absolute_folder() {
        let e = env(Os::Windows);
        let d = resolve(&e, &Settings::default()).unwrap();
        assert!(d.is_default);
        assert_eq!(d.path, default_data_dir(&e).unwrap());

        let chosen = std::env::temp_dir().join("chosen-xpando");
        let d = resolve(
            &e,
            &Settings {
                data_dir: Some(chosen.to_string_lossy().into_owned()),
                import_handled: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!d.is_default);
        assert_eq!(d.path, chosen);

        // A blank choice is the default; a relative one is refused.
        let blank = Settings {
            data_dir: Some("  ".into()),
            import_handled: false,
            ..Default::default()
        };
        assert!(resolve(&e, &blank).unwrap().is_default);
        let rel = Settings {
            data_dir: Some("relative/dir".into()),
            import_handled: false,
            ..Default::default()
        };
        assert!(resolve(&e, &rel).is_err());
    }

    #[test]
    fn dir_status() {
        let dir = temp_dir("status");
        assert_eq!(check_dir(&dir), DirStatus::Ok);
        assert!(is_reachable(&dir));
        assert!(is_writable(&dir));
        assert_eq!(check_dir(&dir.join("nope")), DirStatus::Missing);
        assert!(!is_reachable(&dir.join("nope")));
        let file = dir.join("f.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(check_dir(&file), DirStatus::NotADirectory);
        assert!(!is_reachable(&file));
        assert!(describe_status(&dir.join("nope"), &DirStatus::Missing).contains("another folder"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ensure_layout_creates_subfolders_but_not_a_missing_chosen_root() {
        let dir = temp_dir("layout");
        let chosen = dir.join("cloud");
        // A chosen folder that is gone is an error and is not created.
        assert!(ensure_layout(&chosen, false).is_err());
        assert!(!chosen.exists());
        // The default folder is created with its subfolders.
        ensure_layout(&chosen, true).unwrap();
        assert!(chosen.join("match").is_dir());
        assert!(chosen.join("config").is_dir());
        // An existing chosen folder gets its subfolders; existing files stay.
        fs::write(chosen.join("match").join("a.yml"), "keep").unwrap();
        ensure_layout(&chosen, false).unwrap();
        assert_eq!(
            fs::read_to_string(chosen.join("match/a.yml")).unwrap(),
            "keep"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prepare_default_creates_it_and_chosen_missing_fails() {
        let dir = temp_dir("prepare");
        let mut e = env(Os::Windows);
        e.appdata = Some(dir.join("AppData"));
        let d = prepare(&e, &Settings::default()).unwrap();
        assert!(d.path.join("match").is_dir());

        let gone = Settings {
            data_dir: Some(dir.join("unplugged").to_string_lossy().into_owned()),
            import_handled: false,
            ..Default::default()
        };
        let err = prepare(&e, &gone).unwrap_err();
        assert!(err.contains("cannot be found"), "{err}");
        assert!(!dir.join("unplugged").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn has_data_looks_for_yaml_only() {
        let dir = temp_dir("hasdata");
        ensure_layout(&dir, false).unwrap();
        assert!(!has_data(&dir));
        fs::write(dir.join("match/readme.txt"), "x").unwrap();
        assert!(!has_data(&dir));
        fs::write(dir.join("match/base.yml"), "matches: []\n").unwrap();
        assert!(has_data(&dir));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn copy_data_copies_everything_and_leaves_the_source_alone() {
        let root = temp_dir("copy");
        let (from, to) = (root.join("from"), root.join("to"));
        fs::create_dir_all(from.join("match/sub")).unwrap();
        fs::create_dir_all(from.join("config")).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("match/base.yml"), "A").unwrap();
        fs::write(from.join("match/_hidden.yml"), "B").unwrap();
        fs::write(from.join("match/sub/deep.yaml"), "C").unwrap();
        fs::write(from.join("match/base.yml.bak"), "D").unwrap();
        fs::write(from.join("config/default.yml"), "E").unwrap();
        fs::write(from.join("match/base (conflicted copy).yml"), "F").unwrap();

        let report = copy_data(&from, &to).unwrap();
        assert_eq!(
            report.copied,
            [
                "config/default.yml",
                "match/_hidden.yml",
                "match/base.yml",
                "match/base.yml.bak",
                "match/sub/deep.yaml"
            ]
        );
        assert!(report.already_there.is_empty());
        assert!(report.failed.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].file, "match/base (conflicted copy).yml");
        assert_eq!(
            fs::read_to_string(to.join("match/sub/deep.yaml")).unwrap(),
            "C"
        );
        assert!(!to.join("match/base (conflicted copy).yml").exists());
        // Source untouched.
        assert_eq!(
            fs::read_to_string(from.join("match/base.yml")).unwrap(),
            "A"
        );
        assert!(from.join("match/base (conflicted copy).yml").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_data_never_overwrites() {
        let root = temp_dir("copy-keep");
        let (from, to) = (root.join("from"), root.join("to"));
        fs::create_dir_all(from.join("match")).unwrap();
        fs::create_dir_all(to.join("match")).unwrap();
        fs::write(from.join("match/a.yml"), "old").unwrap();
        fs::write(from.join("match/b.yml"), "b").unwrap();
        fs::write(to.join("match/a.yml"), "mine").unwrap();

        let report = copy_data(&from, &to).unwrap();
        assert_eq!(report.already_there, ["match/a.yml"]);
        assert_eq!(report.copied, ["match/b.yml"]);
        assert_eq!(fs::read_to_string(to.join("match/a.yml")).unwrap(), "mine");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_data_refuses_same_nested_and_unreachable_folders() {
        let root = temp_dir("copy-bad");
        let from = root.join("from");
        fs::create_dir_all(from.join("inside")).unwrap();
        fs::write(from.join("a.yml"), "x").unwrap();

        assert!(copy_data(&from, &from).is_err());
        assert!(copy_data(&from, &from.join("inside")).is_err());
        assert!(copy_data(&from.join("inside"), &from).is_err());
        // Missing source or destination: reported, nothing created.
        assert!(copy_data(&root.join("nope"), &from).is_err());
        let err = copy_data(&from, &root.join("unplugged")).unwrap_err();
        assert!(err.contains("cannot be found"), "{err}");
        assert!(!root.join("unplugged").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    fn env_in(root: &Path) -> Env {
        let mut e = env(Os::Windows);
        e.appdata = Some(root.join("AppData"));
        e
    }

    #[test]
    fn choice_checks() {
        let root = temp_dir("choice");
        let current = root.join("current");
        fs::create_dir_all(&current).unwrap();
        let other = root.join("other");
        fs::create_dir_all(&other).unwrap();

        assert!(!check_choice(&current, "").ok);
        assert!(!check_choice(&current, "relative/dir").ok);
        assert!(!check_choice(&current, &current.to_string_lossy()).ok);
        assert!(!check_choice(&current, &current.join("inside").to_string_lossy()).ok);
        assert!(!check_choice(&current, &root.to_string_lossy()).ok);
        // A missing folder whose parent is missing too is refused (typo or unplugged disk).
        assert!(!check_choice(&current, &root.join("a/b/c").to_string_lossy()).ok);
        // A file is not a folder.
        fs::write(root.join("f.txt"), "x").unwrap();
        assert!(!check_choice(&current, &root.join("f.txt").to_string_lossy()).ok);

        let ok = check_choice(&current, &other.to_string_lossy());
        assert!(ok.ok && !ok.will_create && !ok.has_data);
        let fresh = check_choice(&current, &root.join("fresh").to_string_lossy());
        assert!(fresh.ok && fresh.will_create);
        assert!(!root.join("fresh").exists());

        fs::create_dir_all(other.join("match")).unwrap();
        fs::write(other.join("match/a.yml"), "matches: []\n").unwrap();
        assert!(check_choice(&current, &other.to_string_lossy()).has_data);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn data_info_default_chosen_and_unreachable() {
        let root = temp_dir("info");
        let e = env_in(&root);
        let info = data_info(&e, &Settings::default()).unwrap();
        assert!(info.is_default && info.problem.is_none());
        assert_eq!(info.status, DirStatus::Ok);
        assert!(Path::new(&info.path).join("match").is_dir());

        fs::write(Path::new(&info.path).join("match/a (1).yml"), "").unwrap();
        let info = data_info(&e, &Settings::default()).unwrap();
        assert_eq!(info.conflicts.len(), 1);

        let gone = Settings {
            data_dir: Some(root.join("unplugged").to_string_lossy().into_owned()),
            import_handled: false,
            ..Default::default()
        };
        let info = data_info(&e, &gone).unwrap();
        assert_eq!(info.status, DirStatus::Missing);
        assert!(info.problem.unwrap().contains("another folder"));
        assert!(info.conflicts.is_empty());
        assert!(!root.join("unplugged").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn change_with_copy_switches_and_keeps_the_old_folder() {
        let root = temp_dir("change");
        let e = env_in(&root);
        let settings_dir = root.join("settings");
        let current = PathBuf::from(data_info(&e, &Settings::default()).unwrap().path);
        fs::write(current.join("match/base.yml"), "matches: []\n").unwrap();
        let new = root.join("cloud");
        fs::create_dir_all(&new).unwrap();

        let r = change_data_dir(&e, &settings_dir, Some(&new.to_string_lossy()), true).unwrap();
        assert!(r.switched);
        assert_eq!(r.report.unwrap().copied, ["match/base.yml"]);
        assert!(new.join("match/base.yml").is_file());
        // Old folder left in place.
        assert!(current.join("match/base.yml").is_file());
        let s = load_settings(&settings_dir).unwrap();
        assert_eq!(resolve(&e, &s).unwrap().path, new);

        // Back to the default, without copying: the new folder is left in place as well.
        let r = change_data_dir(&e, &settings_dir, None, false).unwrap();
        assert!(r.switched && r.report.is_none());
        assert!(
            resolve(&e, &load_settings(&settings_dir).unwrap())
                .unwrap()
                .is_default
        );
        assert!(new.join("match/base.yml").is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn change_without_copy_and_to_a_new_folder() {
        let root = temp_dir("change2");
        let e = env_in(&root);
        let settings_dir = root.join("settings");
        let new = root.join("fresh");
        let r = change_data_dir(&e, &settings_dir, Some(&new.to_string_lossy()), false).unwrap();
        assert!(r.switched && r.report.is_none());
        assert!(new.join("match").is_dir() && new.join("config").is_dir());
        // Choosing the same folder again, or a bad one, is an error and changes nothing.
        assert!(change_data_dir(&e, &settings_dir, Some(&new.to_string_lossy()), false).is_err());
        assert!(change_data_dir(&e, &settings_dir, Some("relative"), false).is_err());
        let s = load_settings(&settings_dir).unwrap();
        assert_eq!(resolve(&e, &s).unwrap().path, new);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn failed_copy_does_not_switch() {
        let root = temp_dir("change3");
        let e = env_in(&root);
        let settings_dir = root.join("settings");
        let current = PathBuf::from(data_info(&e, &Settings::default()).unwrap().path);
        fs::write(current.join("match/a.yml"), "x").unwrap();
        let new = root.join("cloud");
        fs::create_dir_all(&new).unwrap();
        // A file where the `match` folder should be makes copying into it fail.
        fs::write(new.join("match"), "in the way").unwrap();

        let r = change_data_dir(&e, &settings_dir, Some(&new.to_string_lossy()), true).unwrap();
        assert!(!r.switched);
        assert_eq!(r.report.unwrap().failed.len(), 1);
        assert_eq!(load_settings(&settings_dir).unwrap(), Settings::default());
        fs::remove_dir_all(&root).unwrap();
    }
}
