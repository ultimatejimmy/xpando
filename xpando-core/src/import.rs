//! Importing an existing espanso setup into xpando's own data folder.
//!
//! Copy only: the espanso folder is opened for reading and nothing in it is ever changed,
//! moved or deleted. What is imported: `match/**/*.yml|yaml` (including `_`-prefixed files and
//! installed packages) and `config/**/*.yml|yaml` (including app filters). Files that already
//! exist in the xpando folder are never overwritten; they are reported instead.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::copytree::{self, CopyReport, Problem};
use crate::datadir::{check_dir, describe_status, ensure_layout, is_writable, DirStatus, Env, Os};

/// Folders where an existing espanso configuration may be, most likely first.
pub fn espanso_candidates(env: &Env) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |p: Option<PathBuf>| {
        if let Some(p) = p {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    };
    add(env.espanso_config_dir.clone());
    match env.os {
        Os::Windows => add(env.appdata.as_ref().map(|a| a.join("espanso"))),
        Os::MacOs => {
            // espanso looks in ~/.config/espanso first on macOS, then Application Support.
            add(env.home.as_ref().map(|h| h.join(".config").join("espanso")));
            add(env.config_base().map(|b| b.join("espanso")));
        }
        Os::Linux => {
            add(env.config_base().map(|b| b.join("espanso")));
            add(env.home.as_ref().map(|h| h.join(".config").join("espanso")));
        }
    }
    out
}

/// The espanso folders that exist and look like a configuration (they have `match` or
/// `config`). Excludes `own_data_dir`, so xpando never offers to import itself.
pub fn find_espanso_dirs(env: &Env, own_data_dir: &Path) -> Vec<PathBuf> {
    espanso_candidates(env)
        .into_iter()
        .filter(|p| p.join("match").is_dir() || p.join("config").is_dir())
        .filter(|p| copytree::check_not_nested(p, own_data_dir).is_ok())
        .collect()
}

/// One file the import would copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFile {
    /// Relative to the espanso folder, with `/` separators, e.g. `match/base.yml`.
    pub file: String,
    pub size: u64,
    /// The xpando folder already has a file with this name; it would be kept as it is.
    pub exists_in_destination: bool,
}

/// What an import would do, computed without changing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportPreview {
    pub source: String,
    pub files: Vec<PreviewFile>,
    /// Files that will not be imported, with the reason.
    pub skipped: Vec<Problem>,
    /// Files or folders that could not be read.
    pub unreadable: Vec<Problem>,
}

impl ImportPreview {
    /// How many files would actually be copied.
    pub fn to_copy(&self) -> usize {
        self.files
            .iter()
            .filter(|f| !f.exists_in_destination)
            .count()
    }
}

const SUBS: [&str; 2] = ["match", "config"];

/// Lists what importing `source` into `dest` would do. Reads only; creates nothing.
pub fn preview_import(source: &Path, dest: &Path) -> Result<ImportPreview, String> {
    let status = check_dir(source);
    if status != DirStatus::Ok {
        return Err(describe_status(source, &status));
    }
    copytree::check_not_nested(source, dest)?;
    let (mut skipped, mut unreadable) = (Vec::new(), Vec::new());
    let found = copytree::walk(source, &SUBS, true, &mut skipped, &mut unreadable);
    let files = found
        .into_iter()
        .map(|f| PreviewFile {
            exists_in_destination: dest.join(f.rel.split('/').collect::<PathBuf>()).exists(),
            file: f.rel,
            size: f.size,
        })
        .collect();
    Ok(ImportPreview {
        source: source.to_string_lossy().into_owned(),
        files,
        skipped,
        unreadable,
    })
}

/// Copies the importable files of `source` into `dest` (creating `match/` and `config/`
/// there). Existing files in `dest` are kept and listed as `already_there`. The source is only
/// read.
pub fn run_import(source: &Path, dest: &Path) -> Result<CopyReport, String> {
    let status = check_dir(source);
    if status != DirStatus::Ok {
        return Err(describe_status(source, &status));
    }
    copytree::check_not_nested(source, dest)?;
    let dest_status = check_dir(dest);
    if dest_status != DirStatus::Ok {
        return Err(describe_status(dest, &dest_status));
    }
    ensure_layout(dest, false)?;
    if !is_writable(dest) {
        return Err(format!("xpando cannot save files in {}.", dest.display()));
    }
    let mut report = CopyReport::default();
    let found = copytree::walk(source, &SUBS, true, &mut report.skipped, &mut report.failed);
    copytree::copy_found(&found, source, dest, &mut report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xpando-import-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_env(os: Os, root: &Path) -> Env {
        Env {
            os,
            appdata: Some(root.join("AppData")),
            home: Some(root.join("home")),
            xdg_config_home: None,
            espanso_config_dir: None,
        }
    }

    /// An espanso folder with a typical mix of files.
    fn sample_source(root: &Path) -> PathBuf {
        let src = root.join("espanso");
        fs::create_dir_all(src.join("match/packages/pkg")).unwrap();
        fs::create_dir_all(src.join("config")).unwrap();
        fs::create_dir_all(src.join("user")).unwrap();
        fs::write(src.join("match/base.yml"), "matches: []\n").unwrap();
        fs::write(src.join("match/_private.yml"), "matches: []\n").unwrap();
        fs::write(src.join("match/packages/pkg/package.yml"), "matches: []\n").unwrap();
        fs::write(src.join("match/img.png"), "png").unwrap();
        fs::write(src.join("match/base (1).yml"), "matches: []\n").unwrap();
        fs::write(src.join("config/default.yml"), "toggle_key: ALT\n").unwrap();
        fs::write(src.join("config/word.yaml"), "filter_exec: winword\n").unwrap();
        fs::write(src.join("user/other.yml"), "x: 1\n").unwrap();
        src
    }

    #[test]
    fn candidates_per_os() {
        let root = Path::new(if cfg!(windows) { "C:\\fake" } else { "/fake" });
        let w = espanso_candidates(&fake_env(Os::Windows, root));
        assert_eq!(w, [root.join("AppData").join("espanso")]);

        let m = espanso_candidates(&fake_env(Os::MacOs, root));
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], root.join("home").join(".config").join("espanso"));
        assert!(
            m[1].ends_with("Application Support/espanso")
                || m[1].ends_with("Application Support\\espanso")
        );

        // Linux: the XDG folder and ~/.config are the same by default and are listed once.
        let l = espanso_candidates(&fake_env(Os::Linux, root));
        assert_eq!(l, [root.join("home").join(".config").join("espanso")]);

        let mut custom = fake_env(Os::Linux, root);
        custom.espanso_config_dir = Some(root.join("mine"));
        let l = espanso_candidates(&custom);
        assert_eq!(l[0], root.join("mine"));
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn finds_only_folders_that_look_like_espanso_and_not_itself() {
        let root = temp_dir("find");
        let env = fake_env(Os::Windows, &root);
        let own = root.join("AppData").join("xpando");
        // Nothing there yet.
        assert!(find_espanso_dirs(&env, &own).is_empty());
        // An empty espanso folder does not count.
        fs::create_dir_all(root.join("AppData/espanso")).unwrap();
        assert!(find_espanso_dirs(&env, &own).is_empty());
        fs::create_dir_all(root.join("AppData/espanso/match")).unwrap();
        assert_eq!(
            find_espanso_dirs(&env, &own),
            [root.join("AppData").join("espanso")]
        );
        // Never offers its own data folder.
        let own_is_espanso = root.join("AppData").join("espanso");
        assert!(find_espanso_dirs(&env, &own_is_espanso).is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn preview_lists_yaml_in_match_and_config_and_explains_the_rest() {
        let root = temp_dir("preview");
        let src = sample_source(&root);
        let dest = root.join("xpando");
        fs::create_dir_all(&dest).unwrap();

        let p = preview_import(&src, &dest).unwrap();
        let files: Vec<_> = p.files.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "match/_private.yml",
                "match/base.yml",
                "match/packages/pkg/package.yml",
                "config/default.yml",
                "config/word.yaml"
            ]
        );
        assert_eq!(p.to_copy(), 5);
        assert!(p.files.iter().all(|f| !f.exists_in_destination));
        let skipped: Vec<_> = p.skipped.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(skipped, ["match/base (1).yml", "match/img.png"]);
        assert!(p.unreadable.is_empty());
        // A preview changes nothing.
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn preview_marks_files_that_would_be_kept() {
        let root = temp_dir("preview-exists");
        let src = sample_source(&root);
        let dest = root.join("xpando");
        fs::create_dir_all(dest.join("match")).unwrap();
        fs::write(dest.join("match/base.yml"), "mine").unwrap();
        let p = preview_import(&src, &dest).unwrap();
        assert_eq!(p.to_copy(), 4);
        let base = p.files.iter().find(|f| f.file == "match/base.yml").unwrap();
        assert!(base.exists_in_destination);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn import_copies_and_never_touches_the_source() {
        let root = temp_dir("run");
        let src = sample_source(&root);
        let before: Vec<_> = {
            let mut v = Vec::new();
            let (mut s, mut f) = (Vec::new(), Vec::new());
            for x in copytree::walk(&src, &[], false, &mut s, &mut f) {
                let bytes = fs::read(src.join(x.rel.split('/').collect::<PathBuf>())).unwrap();
                v.push((x.rel, bytes));
            }
            v
        };
        let dest = root.join("xpando");
        fs::create_dir_all(&dest).unwrap();

        let r = run_import(&src, &dest).unwrap();
        assert_eq!(r.copied.len(), 5);
        assert!(r.already_there.is_empty());
        assert!(r.failed.is_empty());
        assert_eq!(r.skipped.len(), 2);
        assert_eq!(
            fs::read_to_string(dest.join("config/default.yml")).unwrap(),
            "toggle_key: ALT\n"
        );
        assert!(dest.join("match/_private.yml").is_file());
        assert!(dest.join("match/packages/pkg/package.yml").is_file());
        assert!(!dest.join("match/img.png").exists());
        assert!(!dest.join("user").exists());

        // Source identical afterwards: same files, same bytes.
        let (mut s, mut f) = (Vec::new(), Vec::new());
        let after: Vec<_> = copytree::walk(&src, &[], false, &mut s, &mut f)
            .into_iter()
            .map(|x| {
                let bytes = fs::read(src.join(x.rel.split('/').collect::<PathBuf>())).unwrap();
                (x.rel, bytes)
            })
            .collect();
        assert_eq!(before, after);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn import_never_overwrites_and_reports_what_it_kept() {
        let root = temp_dir("keep");
        let src = sample_source(&root);
        let dest = root.join("xpando");
        fs::create_dir_all(dest.join("match")).unwrap();
        fs::write(dest.join("match/base.yml"), "my own phrases").unwrap();

        let r = run_import(&src, &dest).unwrap();
        assert_eq!(r.already_there, ["match/base.yml"]);
        assert_eq!(r.copied.len(), 4);
        assert_eq!(
            fs::read_to_string(dest.join("match/base.yml")).unwrap(),
            "my own phrases"
        );

        // Running it again copies nothing new and keeps everything.
        let again = run_import(&src, &dest).unwrap();
        assert!(again.copied.is_empty());
        assert_eq!(again.already_there.len(), 5);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn import_creates_the_layout_in_an_empty_destination() {
        let root = temp_dir("layout");
        let src = sample_source(&root);
        let dest = root.join("xpando");
        fs::create_dir_all(&dest).unwrap();
        run_import(&src, &dest).unwrap();
        assert!(dest.join("match").is_dir() && dest.join("config").is_dir());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn import_refuses_bad_folders_without_creating_anything() {
        let root = temp_dir("bad");
        let src = sample_source(&root);
        // Missing source.
        assert!(preview_import(&root.join("nope"), &root.join("x")).is_err());
        assert!(run_import(&root.join("nope"), &root.join("x")).is_err());
        // Same and nested folders.
        assert!(run_import(&src, &src).is_err());
        assert!(run_import(&src, &src.join("match")).is_err());
        assert!(preview_import(&src.join("match"), &src).is_err());
        // Missing destination is reported, not created.
        assert!(run_import(&src, &root.join("unplugged")).is_err());
        assert!(!root.join("unplugged").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_espanso_folder_imports_nothing() {
        let root = temp_dir("empty");
        let src = root.join("espanso");
        fs::create_dir_all(src.join("match")).unwrap();
        let dest = root.join("xpando");
        fs::create_dir_all(&dest).unwrap();
        let p = preview_import(&src, &dest).unwrap();
        assert!(p.files.is_empty());
        let r = run_import(&src, &dest).unwrap();
        assert!(r.copied.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}
