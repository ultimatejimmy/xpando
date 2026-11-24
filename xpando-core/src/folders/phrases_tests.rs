use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

const DEFAULT: &str = "# espanso configuration file\n\n# toggle_key: ALT\n";

const BASE: &str = "# my phrases\nmatches:\n  # Signature block\n  - trigger: \":sig\"\n    replace: \"Regards,\\nJimmy\"\n    label: Sig\n\n  # the year\n  - trigger: \":year\"\n    replace: \"{{d}}\"\n    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y\"\n\n  - trigger: \":zz\"\n    replace: \"last\"\n";

struct Dir(PathBuf);

impl Dir {
    fn new(configs: &[(&str, &str)], matches: &[(&str, &str)]) -> Dir {
        let d = std::env::temp_dir().join(format!(
            "xpando-phrases-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(d.join("config")).unwrap();
        fs::create_dir_all(d.join("match")).unwrap();
        for (n, t) in configs {
            fs::write(d.join("config").join(n), t).unwrap();
        }
        for (n, t) in matches {
            fs::write(d.join("match").join(n), t).unwrap();
        }
        Dir(d)
    }
    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap()
    }
    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).exists()
    }
    fn hash(&self, rel: &str) -> String {
        content_hash(&fs::read(self.0.join("match").join(rel)).unwrap())
    }
    fn info(&self, file: &str) -> FolderInfo {
        list_folder_info(&self.0)
            .unwrap()
            .into_iter()
            .find(|f| f.file == file)
            .unwrap_or_else(|| panic!("no folder {file}"))
    }
    /// Every file under match/ and config/ with its text, to prove nothing changed.
    fn snapshot(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        for sub in ["match", "config"] {
            for e in fs::read_dir(self.0.join(sub)).unwrap() {
                let p = e.unwrap().path();
                m.insert(
                    format!("{sub}/{}", p.file_name().unwrap().to_string_lossy()),
                    fs::read_to_string(&p).unwrap(),
                );
            }
        }
        m
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn word_config() -> String {
    format!(
        "# Word only\nfilter_exec: '{}'\nextra_includes:\n  - \"../match/_word.yml\"\n",
        exec_pattern(&["winword".to_string()]).unwrap()
    )
}

/// base.yml (all apps) and an empty Word-only folder.
fn with_word() -> Dir {
    Dir::new(
        &[("default.yml", DEFAULT), ("word.yml", &word_config())],
        &[("base.yml", BASE), ("_word.yml", "matches:\n")],
    )
}

fn apps(exes: &[&str]) -> PhraseWhere {
    PhraseWhere::Apps {
        exes: exes.iter().map(|s| s.to_string()).collect(),
        names: Vec::new(),
    }
}

fn index_of(d: &Dir, file: &str, trigger: &str) -> usize {
    let (p, _) = crate::parse_match_text(&d.read(&format!("match/{file}")));
    p.iter()
        .find(|p| p.triggers.iter().any(|t| t == trigger))
        .unwrap_or_else(|| panic!("no {trigger} in {file}"))
        .index
}

fn move_to(d: &Dir, file: &str, trigger: &str, wh: &PhraseWhere) -> Result<MoveOutcome, String> {
    let i = index_of(d, file, trigger);
    set_phrase_where(&d.0, file, i, wh, &d.hash(file))
}

#[test]
fn moves_into_the_folder_whose_apps_match_and_keeps_the_text() {
    let d = with_word();
    // The program name is matched without caring about case or ".exe".
    let out = move_to(&d, "base.yml", ":sig", &apps(&["WINWORD.EXE"])).unwrap();
    assert_eq!(out.to_file, "_word.yml");
    assert!(!out.created_folder);
    assert_eq!(out.to_index, 0);

    // Comment, quoting and the label travel with the phrase.
    assert_eq!(
        d.read("match/_word.yml"),
        "matches:\n  # Signature block\n  - trigger: \":sig\"\n    replace: \"Regards,\\nJimmy\"\n    label: Sig\n"
    );
    // The source file loses exactly that entry; everything else is byte for byte as before.
    let want = BASE.replace(
        "  # Signature block\n  - trigger: \":sig\"\n    replace: \"Regards,\\nJimmy\"\n    label: Sig\n\n",
        "",
    );
    assert_eq!(d.read("match/base.yml"), want);
    // The one-time backups exist.
    assert!(d.exists("match/base.yml.bak"));
    assert!(d.exists("match/_word.yml.bak"));
    // The scope of both folders is unchanged.
    assert_eq!(d.info("_word.yml").scope.kind, ScopeKind::Only);
    assert_eq!(d.info("base.yml").scope.kind, ScopeKind::All);
}

#[test]
fn a_phrase_with_macros_moves_whole() {
    let d = with_word();
    move_to(&d, "base.yml", ":year", &apps(&["winword"])).unwrap();
    let w = d.read("match/_word.yml");
    assert!(w.contains("  # the year\n  - trigger: \":year\"\n"));
    assert!(w.contains("    vars:\n      - name: d\n        type: date\n        params:\n          format: \"%Y\"\n"));
    let (p, warn) = crate::parse_match_text(&w);
    assert!(warn.is_empty());
    assert_eq!(p[0].vars.len(), 1);
}

#[test]
fn creates_a_folder_for_several_apps_and_reuses_it() {
    let d = with_word();
    let wh = PhraseWhere::Apps {
        exes: vec!["winword.exe".into(), "outlook.exe".into()],
        names: vec!["Word".into(), "Outlook".into()],
    };
    let pv = preview_phrase_where(&d.0, "base.yml", index_of(&d, "base.yml", ":sig"), &wh).unwrap();
    assert!(pv.creates_folder);
    assert_eq!(pv.to_name, "Word, Outlook");
    assert!(pv.notice.contains("creates the folder"));
    assert!(pv.blocked.is_none());
    // A preview changes nothing.
    assert!(!d.exists("match/_Word, Outlook.yml"));

    let out = move_to(&d, "base.yml", ":sig", &wh).unwrap();
    assert!(out.created_folder);
    assert_eq!(out.to_file, "_Word, Outlook.yml");
    let info = d.info("_Word, Outlook.yml");
    assert_eq!(info.scope.kind, ScopeKind::Only);
    assert_eq!(info.scope.apps.len(), 1);
    assert_eq!(info.scope.apps[0].exe_names, ["winword", "outlook"]);
    assert!(d.read("config/Word, Outlook.yml").contains("filter_exec"));
    assert!(d
        .read("match/_Word, Outlook.yml")
        .contains("# Signature block\n"));

    // The same programs in another order find that folder instead of making a second one.
    let again = apps(&["OUTLOOK", "winword"]);
    let pv =
        preview_phrase_where(&d.0, "base.yml", index_of(&d, "base.yml", ":year"), &again).unwrap();
    assert!(!pv.creates_folder);
    assert_eq!(pv.to_file.as_deref(), Some("_Word, Outlook.yml"));
    let out = move_to(&d, "base.yml", ":year", &again).unwrap();
    assert!(!out.created_folder);
    assert_eq!(out.to_index, 1);
    // Still exactly one config for it.
    assert_eq!(
        list_folder_info(&d.0)
            .unwrap()
            .iter()
            .filter(|i| i.scope.kind == ScopeKind::Only)
            .count(),
        2
    );
}

#[test]
fn a_taken_folder_name_gets_a_number() {
    // An all-apps folder is already called "Word".
    let d = Dir::new(
        &[("default.yml", DEFAULT)],
        &[("base.yml", BASE), ("Word.yml", "matches:\n")],
    );
    let wh = PhraseWhere::Apps {
        exes: vec!["winword".into()],
        names: vec!["Word".into()],
    };
    let out = move_to(&d, "base.yml", ":sig", &wh).unwrap();
    assert_eq!(out.to_file, "_Word 2.yml");
    assert_eq!(d.info("Word.yml").scope.kind, ScopeKind::All);
}

#[test]
fn a_clashing_trigger_in_the_destination_blocks_the_move() {
    let d = with_word();
    fs::write(
        d.0.join("match/_word.yml"),
        "matches:\n  - trigger: \":sig\"\n    replace: \"Word signature\"\n",
    )
    .unwrap();
    let before = d.snapshot();
    let i = index_of(&d, "base.yml", ":sig");
    let pv = preview_phrase_where(&d.0, "base.yml", i, &apps(&["winword"])).unwrap();
    let why = pv.blocked.expect("blocked");
    assert!(why.contains(":sig"), "{why}");
    assert!(pv.conflicts.iter().any(|c| c.blocking));
    let err = set_phrase_where(
        &d.0,
        "base.yml",
        i,
        &apps(&["winword"]),
        &d.hash("base.yml"),
    )
    .unwrap_err();
    assert!(err.contains(":sig"));
    assert_eq!(d.snapshot(), before, "nothing may change");
}

#[test]
fn an_app_folder_also_clashes_with_all_apps_folders() {
    // :zz also exists in another all-apps folder; in Word both would fire.
    let d = Dir::new(
        &[("default.yml", DEFAULT), ("word.yml", &word_config())],
        &[
            ("base.yml", BASE),
            (
                "other.yml",
                "matches:\n  - trigger: \":zz\"\n    replace: \"x\"\n",
            ),
            ("_word.yml", "matches:\n"),
        ],
    );
    let i = index_of(&d, "base.yml", ":zz");
    let pv = preview_phrase_where(&d.0, "base.yml", i, &apps(&["winword"])).unwrap();
    assert!(pv.blocked.is_some());
    // Between two all-apps folders it is only a heads-up.
    fs::write(d.0.join("match/third.yml"), "matches:\n").unwrap();
    let wh = PhraseWhere::All {
        folder: Some("third.yml".into()),
    };
    let pv = preview_phrase_where(&d.0, "base.yml", i, &wh).unwrap();
    assert!(pv.blocked.is_none());
    assert!(pv.conflicts.iter().any(|c| !c.blocking));
}

#[test]
fn folders_with_other_filters_are_never_matched_or_changed() {
    // Same program, but the folder also filters on a window title.
    let cfg = format!(
        "filter_exec: '{}'\nfilter_title: '(?i)inbox'\nextra_includes:\n  - \"../match/_inbox.yml\"\n",
        exec_pattern(&["outlook".to_string()]).unwrap()
    );
    // And a hand-written program pattern.
    let hand = "filter_exec: \"(?i)excel.*\"\nextra_includes:\n  - \"../match/_excel.yml\"\n";
    let d = Dir::new(
        &[
            ("default.yml", DEFAULT),
            ("inbox.yml", &cfg),
            ("excel.yml", hand),
        ],
        &[
            ("base.yml", BASE),
            ("_inbox.yml", "matches:\n"),
            ("_excel.yml", "matches:\n"),
        ],
    );
    assert!(simple_app_set(&d.info("_inbox.yml").scope).is_none());
    let before = d.snapshot();

    let i = index_of(&d, "base.yml", ":sig");
    let pv = preview_phrase_where(&d.0, "base.yml", i, &apps(&["outlook"])).unwrap();
    assert!(
        pv.creates_folder,
        "must not reuse the title-filtered folder"
    );
    assert!(pv.warnings.iter().any(|w| w.contains("left as it is")));
    let out = set_phrase_where(
        &d.0,
        "base.yml",
        i,
        &apps(&["outlook"]),
        &d.hash("base.yml"),
    )
    .unwrap();
    assert_ne!(out.to_file, "_inbox.yml");
    // The hand-written folder is not matched either.
    let pv = preview_phrase_where(
        &d.0,
        "base.yml",
        index_of(&d, "base.yml", ":year"),
        &apps(&["excel"]),
    )
    .unwrap();
    assert!(pv.creates_folder);

    // Neither folder nor its config was touched.
    for f in [
        "match/_inbox.yml",
        "match/_excel.yml",
        "config/inbox.yml",
        "config/excel.yml",
    ] {
        assert_eq!(d.read(f), before[f], "{f}");
    }
}

#[test]
fn moving_out_of_a_title_filtered_folder_warns() {
    let cfg = format!(
        "filter_exec: '{}'\nfilter_title: '(?i)inbox'\nextra_includes:\n  - \"../match/_inbox.yml\"\n",
        exec_pattern(&["outlook".to_string()]).unwrap()
    );
    let d = Dir::new(
        &[("default.yml", DEFAULT), ("inbox.yml", &cfg)],
        &[("base.yml", "matches:\n"), ("_inbox.yml", BASE)],
    );
    let pv = preview_phrase_where(
        &d.0,
        "_inbox.yml",
        index_of(&d, "_inbox.yml", ":sig"),
        &PhraseWhere::All { folder: None },
    )
    .unwrap();
    assert!(pv.blocked.is_none());
    assert!(pv.warnings.iter().any(|w| w.contains("drops that limit")));
}

#[test]
fn back_to_all_apps_uses_the_chosen_or_default_folder() {
    let d = with_word();
    move_to(&d, "base.yml", ":sig", &apps(&["winword"])).unwrap();
    let before_base = d.read("match/base.yml");

    // Default folder: base.yml.
    let pv =
        preview_phrase_where(&d.0, "_word.yml", 0, &PhraseWhere::All { folder: None }).unwrap();
    assert_eq!(pv.to_file.as_deref(), Some("base.yml"));
    assert!(pv.notice.contains("all apps"));

    // A remembered origin wins.
    fs::write(d.0.join("match/personal.yml"), "matches:\n").unwrap();
    let out = set_phrase_where(
        &d.0,
        "_word.yml",
        0,
        &PhraseWhere::All {
            folder: Some("personal.yml".into()),
        },
        &d.hash("_word.yml"),
    )
    .unwrap();
    assert_eq!(out.to_file, "personal.yml");
    assert!(d
        .read("match/personal.yml")
        .contains("# Signature block\n  - trigger: \":sig\""));
    assert_eq!(d.read("match/_word.yml").trim(), "matches:");
    assert_eq!(d.read("match/base.yml"), before_base);
}

#[test]
fn a_phrase_that_is_already_there_is_left_alone() {
    let d = with_word();
    let before = d.snapshot();
    let i = index_of(&d, "base.yml", ":sig");
    let pv = preview_phrase_where(&d.0, "base.yml", i, &PhraseWhere::All { folder: None }).unwrap();
    assert!(pv.unchanged && pv.blocked.is_none());
    assert!(pv.notice.contains("already"));

    move_to(&d, "base.yml", ":sig", &apps(&["winword"])).unwrap();
    let pv = preview_phrase_where(&d.0, "_word.yml", 0, &apps(&["winword"])).unwrap();
    assert!(pv.unchanged);
    assert!(set_phrase_where(
        &d.0,
        "_word.yml",
        0,
        &apps(&["winword"]),
        &d.hash("_word.yml")
    )
    .is_err());
    let _ = before;
}

#[test]
fn a_move_with_a_stale_hash_changes_nothing() {
    let d = with_word();
    let before = d.snapshot();
    let i = index_of(&d, "base.yml", ":sig");
    let err =
        set_phrase_where(&d.0, "base.yml", i, &apps(&["winword"]), "0000000000000000").unwrap_err();
    assert!(err.starts_with(CONFLICT_PREFIX), "{err}");
    assert_eq!(d.snapshot(), before);
}

#[test]
fn a_failed_second_write_puts_everything_back() {
    // The source file is replaced by a directory-free read-only state after planning is
    // impossible to simulate here, so check the rollback through a stale destination:
    // the plan is built, then the destination changes before the plan runs.
    let d = with_word();
    let l = Layout::from_disk(&d.0).unwrap();
    let i = index_of(&d, "base.yml", ":sig");
    let planned = plan_move(&d.0, &l, "base.yml", i, &apps(&["winword"])).unwrap();
    let plan = planned.plan.unwrap();
    fs::write(
        d.0.join("match/_word.yml"),
        "matches:\n  # edited meanwhile\n",
    )
    .unwrap();
    let before = d.snapshot();
    let err = execute(&d.0, &l, &plan).unwrap_err();
    assert!(err.contains("changed outside xpando"), "{err}");
    assert_eq!(d.snapshot(), before, "nothing may change");
}

#[test]
fn read_only_folders_are_refused() {
    let d = with_word();
    let word = d.0.join("match/_word.yml");
    let mut perms = fs::metadata(&word).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&word, perms.clone()).unwrap();
    let i = index_of(&d, "base.yml", ":sig");
    let pv = preview_phrase_where(&d.0, "base.yml", i, &apps(&["winword"])).unwrap();
    #[allow(clippy::permissions_set_readonly_false)]
    {
        perms.set_readonly(false);
        fs::set_permissions(&word, perms).unwrap();
    }
    assert!(pv.blocked.unwrap().contains("read-only"));
}

#[test]
fn the_phrase_adopts_the_destination_files_style() {
    // 4-space indentation and Windows line ends in the destination.
    let dest = "matches:\r\n    - trigger: \":a\"\r\n      replace: \"A\"\r\n";
    let d = Dir::new(
        &[("default.yml", DEFAULT), ("word.yml", &word_config())],
        &[("base.yml", BASE), ("_word.yml", dest)],
    );
    move_to(&d, "base.yml", ":year", &apps(&["winword"])).unwrap();
    let w = d.read("match/_word.yml");
    assert!(
        !w.replace("\r\n", "").contains('\n'),
        "mixed line ends: {w:?}"
    );
    assert!(w.contains(
        "    # the year\r\n    - trigger: \":year\"\r\n      replace: \"{{d}}\"\r\n      vars:\r\n"
    ));
    let (p, warn) = crate::parse_match_text(&w);
    assert!(warn.is_empty());
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].vars[0].params[0].1, "%Y");
}

#[test]
fn packages_and_missing_all_apps_folders_are_explained() {
    let d = Dir::new(
        &[("default.yml", DEFAULT), ("word.yml", &word_config())],
        &[("_word.yml", BASE)],
    );
    let pv =
        preview_phrase_where(&d.0, "_word.yml", 0, &PhraseWhere::All { folder: None }).unwrap();
    assert!(pv
        .blocked
        .unwrap()
        .contains("no folder that works in all apps"));
    let e = preview_phrase_where(&d.0, "_word.yml", 0, &apps(&[])).unwrap();
    assert!(e.blocked.is_some());
}

#[test]
fn names_and_keys() {
    assert_eq!(exe_key(" WinWord.EXE "), "winword");
    assert_eq!(exe_key("notepad++"), "notepad++");
    let n = |v: &[&str]| auto_folder_name(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(n(&["Word"]), "Word");
    assert_eq!(n(&["Word", "Outlook"]), "Word, Outlook");
    assert_eq!(n(&["A", "B", "C", "D", "E"]), "A, B and 3 more");
    assert_eq!(n(&["Notepad++"]), "Notepad");
    assert_eq!(n(&[]), "Apps");
    assert_eq!(n(&["Teams", "Teams", "Word"]), "Teams, Word");
    assert!(validate_name(&n(&["Word", "Excel", "PowerPoint"])).is_ok());
    assert!(validate_name(&n(&[&"x".repeat(200)])).is_ok());
}
