use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const DEFAULT: &str = "# espanso configuration file\n\n# toggle_key: ALT\n";

fn lay(configs: &[(&str, &str)], files: &[&str]) -> Layout {
    let mut l = Layout {
        root: "C:/Users/me/AppData/Roaming/espanso".into(),
        ..Layout::default()
    };
    for (n, t) in configs {
        l.configs.insert((*n).into(), (*t).into());
    }
    for f in files {
        l.match_files.insert((*f).into());
    }
    l
}

fn info(l: &Layout, file: &str) -> FolderInfo {
    folder_infos(l)
        .into_iter()
        .find(|f| f.file == file)
        .unwrap_or_else(|| panic!("no folder {file}"))
}

fn word_layout() -> Layout {
    lay(
        &[
            ("default.yml", DEFAULT),
            (
                "word.yml",
                "# Word only\nfilter_exec: \"(?i)winword\"\nextra_includes:\n  - \"../match/_word.yml\"\n",
            ),
            (
                "firefox.yml",
                "filter_exec: \"(?i)firefox\"\n# keep tabs\nword_separators: [\" \", \"\\t\"]\nextra_includes:\n  - \"../match/_firefox.yml\"\n",
            ),
        ],
        &["base.yml", "_word.yml", "_firefox.yml", "_orphan.yml"],
    )
}

fn rm_entry(key: &str, entry: &str) -> ConfigEdit {
    ConfigEdit::RemoveEntry {
        key: key.into(),
        entry: entry.into(),
    }
}

fn set_text(key: &str, value: Option<&str>) -> ConfigEdit {
    ConfigEdit::SetText {
        key: key.into(),
        value: value.map(String::from),
    }
}

// ---- names ------------------------------------------------------------------------------

#[test]
fn names_are_validated() {
    assert_eq!(validate_name("  Work notes ").unwrap(), "Work notes");
    for bad in [
        "", "  ", "a/b", "a\\b", "..", "a..b", "_x", "-x", ".x", "x.", "a:b", "a*b", "CON",
        "nul.txt", "a  b",
    ] {
        assert!(validate_name(bad).is_err(), "{bad:?} should be refused");
    }
    assert!(validate_name(&"x".repeat(65)).is_err());
    assert!(validate_name("Übung 2").is_ok());
}

// ---- globs ------------------------------------------------------------------------------

#[test]
fn standard_includes_skip_underscore_files_only() {
    assert!(standard_matches("base.yml"));
    assert!(standard_matches("sub/deep/base.yaml"));
    assert!(standard_matches("packages/x/package.yml"));
    assert!(!standard_matches("_word.yml"));
    assert!(!standard_matches("sub/_word.yml"));
    assert!(!standard_matches("base.yml.bak"));
}

#[test]
fn entries_resolve_like_espanso() {
    let root = "C:/cfg";
    let m = |e: &str, f: &str| entry_matches(e, f, root);
    assert_eq!(m("../match/_word.yml", "_word.yml"), Some(true));
    assert_eq!(m("../match/./_word.yml", "_word.yml"), Some(true));
    assert_eq!(m("..\\match\\_word.yml", "_word.yml"), Some(true));
    assert_eq!(m("../match/_word.yml", "_other.yml"), Some(false));
    assert_eq!(m("../match/_w*.yml", "_word.yml"), Some(true));
    assert_eq!(m("../match/**/*.yml", "a/b/c.yml"), Some(true));
    assert_eq!(m("../match/*.yml", "a/c.yml"), Some(false));
    assert_eq!(m("../match/_?ord.yml", "_word.yml"), Some(true));
    assert_eq!(m("../match/[a-c]*.yml", "base.yml"), Some(true));
    assert_eq!(m("C:/cfg/match/x.yml", "x.yml"), Some(true));
    assert_eq!(m("c:\\CFG\\match\\x.yml", "x.yml"), Some(true));
    assert_eq!(m("D:/elsewhere/x.yml", "x.yml"), None);
    assert_eq!(m("../../x.yml", "x.yml"), None);
}

// ---- scope ------------------------------------------------------------------------------

#[test]
fn scope_of_the_users_convention() {
    let l = word_layout();
    let base = info(&l, "base.yml");
    assert_eq!(base.scope.kind, ScopeKind::All);
    assert!(base.scope.apps.is_empty());
    let w = info(&l, "_word.yml");
    assert_eq!(w.name, "word");
    assert_eq!(w.scope.kind, ScopeKind::Only);
    assert_eq!(w.scope.apps.len(), 1);
    assert_eq!(w.scope.apps[0].config, "word.yml");
    assert_eq!(w.scope.apps[0].exe_names, ["winword"]);
    assert!(w.scope.apps[0].exec_simple);
    assert_eq!(w.configs, ["word.yml"]);
    assert!(w.scope_locked.is_none() && w.locked.is_none());
}

#[test]
fn folder_referenced_by_no_config_applies_nowhere() {
    let l = word_layout();
    let o = info(&l, "_orphan.yml");
    assert_eq!(o.scope.kind, ScopeKind::Nowhere);
    assert!(o.configs.is_empty());
}

#[test]
fn config_without_filters_never_applies() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            ("x.yml", "extra_includes:\n  - \"../match/_x.yml\"\n"),
        ],
        &["_x.yml"],
    );
    assert_eq!(info(&l, "_x.yml").scope.kind, ScopeKind::Nowhere);
}

#[test]
fn defaults_flow_into_custom_configs() {
    // default.yml loads _g.yml for everyone; word.yml replaces extra_includes so it does not.
    let l = lay(
        &[
            ("default.yml", "extra_includes:\n  - \"../match/_g.yml\"\n"),
            (
                "word.yml",
                "filter_exec: winword\nextra_includes:\n  - \"../match/_w.yml\"\n",
            ),
            ("ff.yml", "filter_exec: firefox\n"),
        ],
        &["_g.yml", "_w.yml", "base.yml"],
    );
    let g = info(&l, "_g.yml");
    assert_eq!(g.scope.kind, ScopeKind::Except);
    assert_eq!(g.scope.apps.len(), 1);
    assert_eq!(g.scope.apps[0].config, "word.yml");
    assert_eq!(info(&l, "base.yml").scope.kind, ScopeKind::All);
    assert_eq!(info(&l, "_w.yml").scope.kind, ScopeKind::Only);
}

#[test]
fn excludes_and_standard_includes_switch_are_honoured() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "a.yml",
                "filter_exec: a\nexcludes:\n  - \"../match/base.yml\"\nuse_standard_includes: true\n",
            ),
            ("b.yml", "filter_exec: b\nuse_standard_includes: false\n"),
        ],
        &["base.yml", "other.yml"],
    );
    let b = info(&l, "base.yml");
    assert_eq!(b.scope.kind, ScopeKind::Except);
    let names: Vec<_> = b.scope.apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
    assert!(b.scope_locked.unwrap().contains("excludes"));
}

#[test]
fn imports_pull_files_into_a_scope() {
    let mut l = word_layout();
    l.match_files.insert("_helper.yml".into());
    l.imports
        .insert("_word.yml".into(), vec!["_helper.yml".into()]);
    let h = info(&l, "_helper.yml");
    assert_eq!(h.scope.kind, ScopeKind::Only);
    assert_eq!(h.imported_by, ["_word.yml"]);
    assert!(h.locked.unwrap().contains("imported by _word.yml"));
}

#[test]
fn exec_patterns_round_trip() {
    for names in [
        vec!["winword"],
        vec!["idea64", "java"],
        vec!["Microsoft Word", "a+b"],
    ] {
        let names: Vec<String> = names.into_iter().map(String::from).collect();
        let p = exec_pattern(&names).unwrap();
        assert_eq!(parse_exec_pattern(&p).unwrap(), names, "{p}");
    }
    assert_eq!(
        parse_exec_pattern("(?i)(idea64|java)").unwrap(),
        ["idea64", "java"]
    );
    assert_eq!(parse_exec_pattern("(?i)firefox").unwrap(), ["firefox"]);
    assert!(parse_exec_pattern("(?i)^C:\\\\Program.*").is_none());
    assert!(parse_exec_pattern("firefox").is_none());
    assert_eq!(
        exec_pattern(&["WINWORD.EXE".into(), "winword".into()]).unwrap(),
        r"(?i)(^|[\\/])(WINWORD)(\.exe)?$"
    );
    assert!(exec_pattern(&[]).is_err());
    assert!(exec_pattern(&["a/b".into()]).is_err());
    assert!(exec_pattern(&["a|b".into()]).is_err());
    assert_eq!(title_pattern("Inbox - (1)").unwrap(), r"(?i)Inbox \- \(1\)");
    assert_eq!(
        parse_title_pattern(r"(?i)Inbox \- \(1\)").unwrap(),
        "Inbox - (1)"
    );
    assert!(title_pattern("  ").is_err());
}

// ---- config text editing ----------------------------------------------------------------

const WORD_CFG: &str = "# Assumed: Word\nfilter_exec: \"(?i)winword\" # exe\n\nextra_includes:\n  - \"../match/_word.yml\"  # main\n  # a note\n  - \"../match/_other.yml\"\nword_separators: [\" \"]\n";

#[test]
fn edit_removes_one_entry_and_keeps_everything_else() {
    let out = edit_config(
        WORD_CFG,
        &[rm_entry("extra_includes", "../match/_word.yml")],
    )
    .unwrap();
    assert_eq!(
        out,
        "# Assumed: Word\nfilter_exec: \"(?i)winword\" # exe\n\nextra_includes:\n  # a note\n  - \"../match/_other.yml\"\nword_separators: [\" \"]\n"
    );
}

#[test]
fn edit_removing_the_last_entry_removes_the_key() {
    let text = "a: 1\nextra_includes:\n  - x.yml\n# tail\n";
    let out = edit_config(text, &[rm_entry("extra_includes", "x.yml")]).unwrap();
    assert_eq!(out, "a: 1\n# tail\n");
}

#[test]
fn edit_replaces_entry_keeping_quote_style_and_comment() {
    let out = edit_config(
        WORD_CFG,
        &[ConfigEdit::ReplaceEntry {
            key: "extra_includes".into(),
            old: "../match/_word.yml".into(),
            new: "../match/_docs.yml".into(),
        }],
    )
    .unwrap();
    assert!(out.contains("  - \"../match/_docs.yml\"  # main\n"));
    assert_eq!(out.lines().count(), WORD_CFG.lines().count());
    let plain = edit_config(
        "extra_includes:\n  - ../match/_a.yml\n",
        &[ConfigEdit::ReplaceEntry {
            key: "extra_includes".into(),
            old: "../match/_a.yml".into(),
            new: "../match/_b.yml".into(),
        }],
    )
    .unwrap();
    assert_eq!(plain, "extra_includes:\n  - ../match/_b.yml\n");
}

#[test]
fn edit_adds_entries() {
    let add = |text: &str, v: &str| {
        edit_config(
            text,
            &[ConfigEdit::AddEntry {
                key: "extra_includes".into(),
                value: v.into(),
            }],
        )
    };
    let out = add(WORD_CFG, "../match/_new.yml").unwrap();
    assert!(out.contains("  - \"../match/_other.yml\"\n  - \"../match/_new.yml\"\nword_separators"));
    let fresh = add("filter_exec: x\n", "../match/_n.yml").unwrap();
    assert_eq!(
        fresh,
        "filter_exec: x\nextra_includes:\n  - \"../match/_n.yml\"\n"
    );
}

#[test]
fn edit_sets_and_removes_text_settings() {
    let set = |t: &str, k: &str, v: Option<&str>| edit_config(t, &[set_text(k, v)]);
    let out = set(WORD_CFG, "filter_exec", Some(r"(?i)excel")).unwrap();
    assert!(out.contains("filter_exec: '(?i)excel' # exe\n"));
    let out = set(WORD_CFG, "filter_title", Some("Inbox")).unwrap();
    assert!(out.contains("# exe\nfilter_title: 'Inbox'\n\nextra_includes"));
    let out = set("extra_includes:\n  - a\n", "filter_exec", Some("x")).unwrap();
    assert_eq!(out, "extra_includes:\n  - a\nfilter_exec: 'x'\n");
    let out = set(WORD_CFG, "filter_exec", None).unwrap();
    assert!(!out.contains("filter_exec") && out.starts_with("# Assumed: Word\n\nextra"));
    assert_eq!(set("a: 1\n", "filter_exec", None).unwrap(), "a: 1\n");
    // A quote inside the value is doubled.
    assert!(set("a: 1\n", "filter_title", Some("it's"))
        .unwrap()
        .contains("'it''s'"));
}

#[test]
fn edit_keeps_crlf_bom_and_missing_final_newline() {
    let crlf = "filter_exec: x\r\nextra_includes:\r\n  - a.yml\r\n  - b.yml\r\n";
    let out = edit_config(crlf, &[rm_entry("extra_includes", "a.yml")]).unwrap();
    assert_eq!(out, "filter_exec: x\r\nextra_includes:\r\n  - b.yml\r\n");
    let out = edit_config(
        crlf,
        &[ConfigEdit::AddEntry {
            key: "extra_includes".into(),
            value: "c.yml".into(),
        }],
    )
    .unwrap();
    assert!(out.ends_with("  - b.yml\r\n  - c.yml\r\n"));
    assert!(!out.replace("\r\n", "").contains('\n'));
    let bom = "\u{feff}filter_exec: x\nextra_includes:\n  - a.yml\n  - b.yml";
    let out = edit_config(bom, &[rm_entry("extra_includes", "b.yml")]).unwrap();
    assert_eq!(out, "\u{feff}filter_exec: x\nextra_includes:\n  - a.yml");
}

#[test]
fn edit_refuses_layouts_it_cannot_handle() {
    let rm = |t: &str| edit_config(t, &[rm_entry("extra_includes", "a.yml")]);
    // One-line list
    assert!(rm("extra_includes: [\"a.yml\", \"b.yml\"]\n")
        .unwrap_err()
        .contains("one line"));
    // Mixed line endings
    assert!(rm("filter_exec: x\r\nextra_includes:\n  - a.yml\n")
        .unwrap_err()
        .contains("line endings"));
    // Tabs, multiple documents, anchors, quoted keys, duplicates, flow mapping
    assert!(rm("extra_includes:\n\t- a.yml\n").is_err());
    assert!(rm("extra_includes:\n  - a.yml\n---\nx: 1\n").is_err());
    assert!(rm("extra_includes:\n  - &x a.yml\n").is_err());
    assert!(rm("\"extra_includes\":\n  - a.yml\n").is_err());
    assert!(rm("extra_includes:\n  - a.yml\nextra_includes:\n  - b.yml\n").is_err());
    assert!(rm("{extra_includes: [a.yml]}\n").is_err());
    // Multi-line item
    assert!(rm("extra_includes:\n  - a.yml\n    more\n").is_err());
    // Not YAML at all
    assert!(rm("extra_includes: [\n").is_err());
    // Entry not present
    assert!(rm("extra_includes:\n  - z.yml\n").is_err());
    // Multi-line scalar cannot be replaced
    assert!(edit_config(
        "filter_exec: >\n  folded\n",
        &[set_text("filter_exec", Some("x"))]
    )
    .is_err());
}

// ---- rename -----------------------------------------------------------------------------

fn paths(plan: &Plan) -> Vec<(String, String)> {
    plan.changes
        .iter()
        .map(|c| (c.file.clone(), c.action.clone()))
        .collect()
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_string(), b.to_string())
}

#[test]
fn rename_updates_the_file_the_config_and_its_name() {
    let l = word_layout();
    let (plan, new_file) = plan_rename(&l, "_word.yml", "Docs").unwrap();
    assert_eq!(new_file, "_Docs.yml");
    assert_eq!(
        paths(&plan),
        [
            pair("config/word.yml", "edit"),
            pair("match/_word.yml", "rename"),
            pair("config/word.yml", "rename"),
        ]
    );
    assert!(plan.after.configs["Docs.yml"].contains("\"../match/_Docs.yml\""));
    assert!(plan.after.configs["Docs.yml"].contains("# Word only"));
    assert_eq!(info(&plan.after, "_Docs.yml").scope.kind, ScopeKind::Only);
}

#[test]
fn rename_without_references_only_renames_the_file() {
    let l = word_layout();
    let (plan, f) = plan_rename(&l, "base.yml", "main").unwrap();
    assert_eq!(f, "main.yml");
    assert_eq!(paths(&plan), [pair("match/base.yml", "rename")]);
    let (_, f) = plan_rename(&l, "_orphan.yml", "lost").unwrap();
    assert_eq!(f, "_lost.yml");
}

#[test]
fn rename_keeps_a_config_name_that_is_not_the_convention() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "msword.yml",
                "filter_exec: w\nextra_includes:\n  - ../match/_word.yml\n",
            ),
        ],
        &["_word.yml"],
    );
    let (plan, _) = plan_rename(&l, "_word.yml", "docs").unwrap();
    assert!(!plan
        .changes
        .iter()
        .any(|c| c.file == "config/msword.yml" && c.action == "rename"));
    assert!(plan.after.configs["msword.yml"].contains("../match/_docs.yml"));
}

#[test]
fn rename_with_a_shared_config_does_not_rename_the_config() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "idea.yml",
                "filter_exec: idea\nextra_includes:\n  - \"../match/_idea.yml\"\n  - \"../match/_idea_java.yml\"\n",
            ),
        ],
        &["_idea.yml", "_idea_java.yml"],
    );
    let (plan, _) = plan_rename(&l, "_idea.yml", "ide").unwrap();
    assert!(plan.after.configs.contains_key("idea.yml"));
    assert!(plan.after.configs["idea.yml"].contains("_ide.yml"));
    assert!(plan.after.configs["idea.yml"].contains("_idea_java.yml"));
}

#[test]
fn rename_collisions_are_refused() {
    let l = word_layout();
    // Same display name in either spelling, any case.
    assert!(plan_rename(&l, "_word.yml", "base")
        .unwrap_err()
        .contains("already exists"));
    assert!(plan_rename(&l, "base.yml", "Firefox")
        .unwrap_err()
        .contains("already exists"));
    assert!(plan_rename(&l, "base.yml", "base")
        .unwrap_err()
        .contains("already"));
    assert!(plan_rename(&l, "base.yml", "a/b").is_err());
    assert!(plan_rename(&l, "base.yml", "_x").is_err());
    assert!(plan_rename(&l, "missing.yml", "x").is_err());
    assert!(plan_rename(&l, "../base.yml", "x").is_err());
    let mut l2 = word_layout();
    l2.other_files.insert("match/_zed.yml".into());
    assert!(plan_rename(&l2, "_word.yml", "zed")
        .unwrap_err()
        .contains("already exists"));
}

#[test]
fn rename_may_only_change_case() {
    let l = word_layout();
    let (_, f) = plan_rename(&l, "base.yml", "Base").unwrap();
    assert_eq!(f, "Base.yml");
}

#[test]
fn rename_refuses_unusual_references() {
    // Imported
    let mut l = word_layout();
    l.imports
        .insert("base.yml".into(), vec!["_word.yml".into()]);
    assert!(plan_rename(&l, "_word.yml", "x")
        .unwrap_err()
        .contains("imported"));
    // Packages and sub-folders
    let mut l = word_layout();
    l.match_files.insert("packages/p/package.yml".into());
    l.match_files.insert("sub/x.yml".into());
    assert!(plan_rename(&l, "packages/p/package.yml", "x")
        .unwrap_err()
        .contains("package"));
    assert!(plan_rename(&l, "sub/x.yml", "x")
        .unwrap_err()
        .contains("sub-folder"));
    // Absolute path inside the config directory
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nextra_includes:\n  - \"C:/Users/me/AppData/Roaming/espanso/match/_word.yml\"\n",
            ),
        ],
        &["_word.yml"],
    );
    assert!(plan_rename(&l, "_word.yml", "x")
        .unwrap_err()
        .contains("absolute"));
    // Reference with a layout we cannot rewrite
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nextra_includes: [\"../match/_word.yml\"]\n",
            ),
        ],
        &["_word.yml"],
    );
    assert!(plan_rename(&l, "_word.yml", "x")
        .unwrap_err()
        .contains("w.yml"));
    // A config that cannot be read as UTF-8
    let mut l = word_layout();
    l.unreadable.insert("bad.yml".into());
    assert!(plan_rename(&l, "_word.yml", "x")
        .unwrap_err()
        .contains("bad.yml"));
}

#[test]
fn rename_that_would_change_a_wildcard_scope_is_refused() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nextra_includes:\n  - \"../match/_w*.yml\"\n",
            ),
        ],
        &["_word.yml"],
    );
    assert_eq!(info(&l, "_word.yml").scope.kind, ScopeKind::Only);
    assert!(plan_rename(&l, "_word.yml", "docs")
        .unwrap_err()
        .contains("different apps"));
    assert!(plan_rename(&l, "_word.yml", "wide").is_ok());
}

// ---- delete -----------------------------------------------------------------------------

#[test]
fn delete_removes_references_and_dead_configs() {
    let l = word_layout();
    let p = plan_delete(&l, "_word.yml").unwrap();
    assert_eq!(
        paths(&p),
        [
            pair("config/word.yml", "delete"),
            pair("match/_word.yml", "delete"),
        ]
    );
    assert_eq!(
        p.changes[0].backup.as_deref(),
        Some("config/word.yml.deleted.bak")
    );
    assert_eq!(
        p.changes[1].backup.as_deref(),
        Some("match/_word.yml.deleted.bak")
    );
    assert!(!p.after.configs.contains_key("word.yml"));
    assert!(!p.after.match_files.contains("_word.yml"));
}

#[test]
fn delete_keeps_a_config_that_has_other_settings() {
    let l = word_layout();
    let p = plan_delete(&l, "_firefox.yml").unwrap();
    assert_eq!(p.changes[0].action, "edit");
    let t = &p.after.configs["firefox.yml"];
    assert_eq!(
        t,
        "filter_exec: \"(?i)firefox\"\n# keep tabs\nword_separators: [\" \", \"\\t\"]\n"
    );
}

#[test]
fn delete_keeps_shared_configs_and_unreferenced_folders_are_simple() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "idea.yml",
                "filter_exec: idea\nextra_includes:\n  - \"../match/_idea.yml\"\n  - \"../match/_idea_java.yml\"\n",
            ),
        ],
        &["_idea.yml", "_idea_java.yml", "plain.yml"],
    );
    let p = plan_delete(&l, "_idea.yml").unwrap();
    assert_eq!(p.changes.len(), 2);
    assert!(p.after.configs["idea.yml"].contains("_idea_java.yml"));
    assert!(!p.after.configs["idea.yml"].contains("\"../match/_idea.yml\""));
    let p = plan_delete(&l, "plain.yml").unwrap();
    assert_eq!(p.changes.len(), 1);
}

#[test]
fn delete_backup_names_do_not_collide() {
    let mut l = word_layout();
    l.other_files.insert("match/base.yml.deleted.bak".into());
    let p = plan_delete(&l, "base.yml").unwrap();
    assert_eq!(
        p.changes[0].backup.as_deref(),
        Some("match/base.yml.deleted-2.bak")
    );
}

// ---- scope changes ----------------------------------------------------------------------

fn apps(exes: &[&str], title: TitleChange) -> ScopeSpec {
    ScopeSpec::Apps {
        exes: Some(exes.iter().map(|s| s.to_string()).collect()),
        title,
    }
}

#[test]
fn apps_for_a_global_folder_adds_underscore_and_config() {
    let l = word_layout();
    let (p, f) = plan_scope(&l, "base.yml", &apps(&["excel"], TitleChange::Keep)).unwrap();
    assert_eq!(f, "_base.yml");
    assert_eq!(
        paths(&p),
        [
            pair("config/base.yml", "create"),
            pair("match/base.yml", "rename"),
        ]
    );
    let cfg = &p.after.configs["base.yml"];
    assert_eq!(
        cfg,
        "# Apps for the \"base\" folder (match/_base.yml). Written by xpando.\nfilter_exec: '(?i)(^|[\\\\/])(excel)(\\.exe)?$'\nextra_includes:\n  - \"../match/_base.yml\"\n"
    );
    let s = info(&p.after, "_base.yml").scope;
    assert_eq!(s.kind, ScopeKind::Only);
    assert_eq!(s.apps[0].exe_names, ["excel"]);
}

#[test]
fn apps_for_an_unreferenced_underscore_folder() {
    let l = word_layout();
    let (p, f) = plan_scope(
        &l,
        "_orphan.yml",
        &apps(&["a", "b"], TitleChange::Contains("Report (final)".into())),
    )
    .unwrap();
    assert_eq!(f, "_orphan.yml");
    assert_eq!(paths(&p), [pair("config/orphan.yml", "create")]);
    let s = info(&p.after, "_orphan.yml").scope;
    assert_eq!(s.apps[0].exe_names, ["a", "b"]);
    assert_eq!(s.apps[0].title_text.as_deref(), Some("Report (final)"));
}

#[test]
fn apps_edit_in_place_keeps_comments_and_other_keys() {
    let l = word_layout();
    let (p, f) = plan_scope(
        &l,
        "_firefox.yml",
        &apps(
            &["firefox", "librewolf"],
            TitleChange::Contains("GitHub".into()),
        ),
    )
    .unwrap();
    assert_eq!(f, "_firefox.yml");
    assert_eq!(paths(&p), [pair("config/firefox.yml", "edit")]);
    let t = &p.after.configs["firefox.yml"];
    assert!(t.contains("# keep tabs\nword_separators: [\" \", \"\\t\"]\n"));
    assert!(t.contains("filter_title: '(?i)GitHub'\n"));
    assert!(t.contains("(firefox|librewolf)"));
    assert!(t.contains("extra_includes:\n  - \"../match/_firefox.yml\"\n"));
    // Clearing the title and keeping the apps
    let l2 = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "t.yml",
                "filter_exec: x\nfilter_title: y\nextra_includes:\n  - ../match/_t.yml\n",
            ),
        ],
        &["_t.yml"],
    );
    let (p, _) = plan_scope(
        &l2,
        "_t.yml",
        &ScopeSpec::Apps {
            exes: None,
            title: TitleChange::Clear,
        },
    )
    .unwrap();
    assert_eq!(
        p.after.configs["t.yml"],
        "filter_exec: x\nextra_includes:\n  - ../match/_t.yml\n"
    );
}

#[test]
fn apps_unchanged_is_a_no_op_plan() {
    let l = word_layout();
    let (p, _) = plan_scope(
        &l,
        "_word.yml",
        &ScopeSpec::Apps {
            exes: None,
            title: TitleChange::Keep,
        },
    )
    .unwrap();
    assert!(p.ops.is_empty());
}

#[test]
fn apps_for_a_folder_in_a_shared_config_splits_it_out() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "ide.yml",
                "# both\nfilter_exec: idea\nextra_includes:\n  - \"../match/_a.yml\"\n  - \"../match/_b.yml\"\n",
            ),
        ],
        &["_a.yml", "_b.yml"],
    );
    let (p, _) = plan_scope(&l, "_a.yml", &apps(&["code"], TitleChange::Keep)).unwrap();
    assert!(
        p.after.configs["ide.yml"].contains("_b.yml")
            && !p.after.configs["ide.yml"].contains("_a.yml")
    );
    assert!(p.after.configs["a.yml"].contains("code"));
    assert_eq!(info(&p.after, "_b.yml").scope.apps[0].config, "ide.yml");
    assert_eq!(info(&p.after, "_a.yml").scope.apps[0].config, "a.yml");
}

#[test]
fn apps_refuses_a_config_name_clash() {
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "idea.yml",
                "filter_exec: idea\nextra_includes:\n  - \"../match/_idea.yml\"\n  - \"../match/_idea_java.yml\"\n",
            ),
        ],
        &["_idea.yml", "_idea_java.yml"],
    );
    // idea.yml is shared with another folder, so a new config named idea.yml would clash.
    let e = plan_scope(&l, "_idea.yml", &apps(&["code"], TitleChange::Keep)).unwrap_err();
    assert!(e.contains("config/idea.yml already exists"), "{e}");
}

#[test]
fn apps_requires_something_to_match() {
    let l = word_layout();
    assert!(plan_scope(&l, "base.yml", &apps(&[], TitleChange::Keep)).is_err());
    assert!(plan_scope(&l, "base.yml", &apps(&[], TitleChange::Clear)).is_err());
    assert!(plan_scope(&l, "base.yml", &apps(&["a/b"], TitleChange::Keep)).is_err());
    // Keep with nothing to keep
    let keep = ScopeSpec::Apps {
        exes: None,
        title: TitleChange::Keep,
    };
    assert!(plan_scope(&l, "base.yml", &keep).is_err());
}

#[test]
fn apps_refuses_a_target_file_that_exists() {
    let mut l = word_layout();
    l.other_files.insert("match/_base.yml".into());
    assert!(plan_scope(&l, "base.yml", &apps(&["a"], TitleChange::Keep))
        .unwrap_err()
        .contains("already exists"));
}

#[test]
fn all_apps_removes_the_reference_and_the_underscore() {
    let l = word_layout();
    let (p, f) = plan_scope(&l, "_word.yml", &ScopeSpec::All).unwrap();
    assert_eq!(f, "word.yml");
    assert_eq!(
        paths(&p),
        [
            pair("config/word.yml", "delete"),
            pair("match/_word.yml", "rename"),
        ]
    );
    assert_eq!(info(&p.after, "word.yml").scope.kind, ScopeKind::All);
    // Config with other settings stays
    let (p, _) = plan_scope(&l, "_firefox.yml", &ScopeSpec::All).unwrap();
    assert!(p.after.configs["firefox.yml"].contains("word_separators"));
    assert!(!p.after.configs["firefox.yml"].contains("_firefox"));
    // Already everywhere
    assert!(plan_scope(&l, "base.yml", &ScopeSpec::All)
        .unwrap_err()
        .contains("already"));
    // Unreferenced underscore file
    let (_, f) = plan_scope(&l, "_orphan.yml", &ScopeSpec::All).unwrap();
    assert_eq!(f, "orphan.yml");
}

#[test]
fn all_apps_refuses_when_the_plain_name_exists() {
    let mut l = word_layout();
    l.match_files.insert("word.yml".into());
    assert!(plan_scope(&l, "_word.yml", &ScopeSpec::All)
        .unwrap_err()
        .contains("already exists"));
}

#[test]
fn scope_changes_refuse_layouts_they_cannot_keep_safe() {
    // Listed directly by default.yml
    let l = lay(
        &[
            ("default.yml", "extra_includes:\n  - ../match/_g.yml\n"),
            ("w.yml", "filter_exec: w\n"),
        ],
        &["_g.yml"],
    );
    assert!(plan_scope(&l, "_g.yml", &ScopeSpec::All)
        .unwrap_err()
        .contains("default config"));
    // Wildcard include
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nextra_includes: [\"../match/_w*.yml\"]\n",
            ),
        ],
        &["_word.yml"],
    );
    assert!(plan_scope(&l, "_word.yml", &ScopeSpec::All)
        .unwrap_err()
        .contains("pattern"));
    // Absolute
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nextra_includes:\n  - \"C:\\\\Users\\\\me\\\\AppData\\\\Roaming\\\\espanso\\\\match\\\\_word.yml\"\n",
            ),
        ],
        &["_word.yml"],
    );
    assert!(plan_scope(&l, "_word.yml", &ScopeSpec::All)
        .unwrap_err()
        .contains("absolute"));
    // Filters in default.yml
    let l = lay(&[("default.yml", "filter_exec: x\n")], &["a.yml"]);
    assert!(plan_scope(&l, "a.yml", &apps(&["y"], TitleChange::Keep))
        .unwrap_err()
        .contains("default.yml"));
    // A broken config
    let l = lay(
        &[("default.yml", DEFAULT), ("bad.yml", "filter_exec: [\n")],
        &["a.yml"],
    );
    assert!(plan_scope(&l, "a.yml", &apps(&["y"], TitleChange::Keep))
        .unwrap_err()
        .contains("bad.yml"));
    // use_standard_includes: false in another config makes "all apps" unreachable
    let l = lay(
        &[
            ("default.yml", DEFAULT),
            (
                "w.yml",
                "filter_exec: w\nuse_standard_includes: false\nextra_includes:\n  - ../match/_x.yml\n",
            ),
        ],
        &["_x.yml"],
    );
    let e = plan_scope(&l, "_x.yml", &ScopeSpec::All).unwrap_err();
    assert!(e.contains("for every app"), "{e}");
}

// ---- create -----------------------------------------------------------------------------

#[test]
fn create_checks_names_and_collisions() {
    let l = word_layout();
    let (p, f) = plan_create(&l, " Notes ").unwrap();
    assert_eq!(f, "Notes.yml");
    assert_eq!(p.changes.len(), 1);
    assert!(plan_create(&l, "BASE")
        .unwrap_err()
        .contains("already exists"));
    assert!(plan_create(&l, "word")
        .unwrap_err()
        .contains("already exists"));
    assert!(plan_create(&l, "../x").is_err());
    assert!(plan_create(&l, "").is_err());
    let mut l = word_layout();
    l.other_files.insert("match/Notes.yml".into());
    assert!(plan_create(&l, "Notes").is_err());
}

// ---- on disk ----------------------------------------------------------------------------

static N: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new(configs: &[(&str, &str)], matches: &[(&str, &str)]) -> Dir {
        let d = std::env::temp_dir().join(format!(
            "xpando-folders-{}-{}",
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
    fn list(&self, dir: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.0.join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const WORD_MATCH: &str = "matches:\r\n  - trigger: \":a\"\r\n    replace: \"A\"\r\n";

fn word_dir() -> Dir {
    Dir::new(
        &[
            ("default.yml", DEFAULT),
            (
                "word.yml",
                "# Word only\r\nfilter_exec: \"(?i)winword\"\r\nextra_includes:\r\n  - \"../match/_word.yml\"\r\n",
            ),
        ],
        &[("_word.yml", WORD_MATCH), ("base.yml", "matches: []\n")],
    )
}

#[test]
fn disk_create_makes_a_valid_empty_folder_phrases_can_be_added_to() {
    let d = word_dir();
    let out = create_folder(&d.0, "Notes").unwrap();
    assert_eq!(out.file.as_deref(), Some("Notes.yml"));
    assert_eq!(d.read("match/Notes.yml"), "matches:\n");
    let folders = crate::load_folders(&d.0).unwrap();
    let f = folders.iter().find(|f| f.file == "Notes.yml").unwrap();
    assert!(f.phrases.is_empty() && f.warnings.is_empty());
    // A phrase can be added with the normal editor.
    let text = d.read("match/Notes.yml");
    let new = crate::edit::add_phrase(
        &text,
        &crate::edit::NewPhrase {
            triggers: vec![":x".into()],
            label: None,
            body: "hi".into(),
            comment: None,
        },
    )
    .unwrap();
    assert!(new.contains("trigger"));
    assert!(create_folder(&d.0, "notes")
        .unwrap_err()
        .contains("already exists"));
}

#[test]
fn disk_rename_with_crlf_references() {
    let d = word_dir();
    let h = d.hash("_word.yml");
    let out = rename_folder(&d.0, "_word.yml", "Docs", &h).unwrap();
    assert_eq!(out.file.as_deref(), Some("_Docs.yml"));
    assert_eq!(d.read("match/_Docs.yml"), WORD_MATCH);
    assert!(!d.exists("match/_word.yml") && !d.exists("config/word.yml"));
    assert_eq!(
        d.read("config/Docs.yml"),
        "# Word only\r\nfilter_exec: \"(?i)winword\"\r\nextra_includes:\r\n  - \"../match/_Docs.yml\"\r\n"
    );
    assert!(d.exists("config/word.yml.bak"));
    let info = list_folder_info(&d.0).unwrap();
    let f = info.iter().find(|f| f.file == "_Docs.yml").unwrap();
    assert_eq!(f.scope.kind, ScopeKind::Only);
}

#[test]
fn disk_rename_detects_outside_changes() {
    let d = word_dir();
    let e = rename_folder(&d.0, "_word.yml", "Docs", "0000000000000000").unwrap_err();
    assert!(e.starts_with(CONFLICT_PREFIX));
    assert!(d.exists("match/_word.yml"));
}

#[test]
fn disk_delete_needs_confirmation_and_keeps_backups() {
    let d = word_dir();
    let h = d.hash("_word.yml");
    assert!(delete_folder(&d.0, "_word.yml", false, &h).is_err());
    assert!(d.exists("match/_word.yml"));
    let preview = preview_delete(&d.0, "_word.yml").unwrap();
    assert_eq!(preview.len(), 2);
    let out = delete_folder(&d.0, "_word.yml", true, &h).unwrap();
    assert_eq!(out.changes, preview);
    assert_eq!(out.file, None);
    assert!(!d.exists("match/_word.yml") && !d.exists("config/word.yml"));
    assert_eq!(d.read("match/_word.yml.deleted.bak"), WORD_MATCH);
    assert!(d.read("config/word.yml.deleted.bak").contains("winword"));
}

#[test]
fn disk_scope_round_trip() {
    let d = word_dir();
    // Everywhere -> only in Excel
    let h = d.hash("base.yml");
    let out = set_scope(
        &d.0,
        "base.yml",
        &apps(&["excel.exe"], TitleChange::Keep),
        &h,
    )
    .unwrap();
    assert_eq!(out.file.as_deref(), Some("_base.yml"));
    assert!(d.exists("match/_base.yml") && !d.exists("match/base.yml"));
    assert!(d.read("config/base.yml").contains("excel"));
    let f = list_folder_info(&d.0)
        .unwrap()
        .into_iter()
        .find(|f| f.file == "_base.yml")
        .unwrap();
    assert_eq!(f.scope.kind, ScopeKind::Only);
    // ... and back to everywhere
    let h = d.hash("_base.yml");
    let out = set_scope(&d.0, "_base.yml", &ScopeSpec::All, &h).unwrap();
    assert_eq!(out.file.as_deref(), Some("base.yml"));
    assert!(!d.exists("config/base.yml") && d.exists("config/base.yml.deleted.bak"));
    let f = list_folder_info(&d.0)
        .unwrap()
        .into_iter()
        .find(|f| f.file == "base.yml")
        .unwrap();
    assert_eq!(f.scope.kind, ScopeKind::All);
}

#[test]
fn disk_scope_in_place_keeps_crlf() {
    let d = word_dir();
    let h = d.hash("_word.yml");
    set_scope(
        &d.0,
        "_word.yml",
        &apps(
            &["winword", "excel"],
            TitleChange::Contains("Report".into()),
        ),
        &h,
    )
    .unwrap();
    let t = d.read("config/word.yml");
    assert!(
        t.starts_with("# Word only\r\nfilter_exec: '(?i)(^|[\\\\/])(winword|excel)(\\.exe)?$'\r\n")
    );
    assert!(t.contains("filter_title: '(?i)Report'\r\n"));
    assert!(t.ends_with("  - \"../match/_word.yml\"\r\n"));
    assert!(!t.replace("\r\n", "").contains('\n'));
    assert!(d.exists("config/word.yml.bak"));
}

#[test]
fn disk_failure_rolls_everything_back() {
    let d = word_dir();
    // A directory in the way makes the final rename fail after the config was created.
    fs::create_dir(d.0.join("match").join("_base.yml")).unwrap();
    let before_cfg = d.list("config");
    let h = d.hash("base.yml");
    let e = set_scope(&d.0, "base.yml", &apps(&["excel"], TitleChange::Keep), &h).unwrap_err();
    assert!(e.contains("Nothing was changed"), "{e}");
    assert_eq!(d.list("config"), before_cfg);
    assert!(d.exists("match/base.yml"));
}

#[test]
fn disk_concurrent_config_change_is_detected() {
    let d = word_dir();
    let l = Layout::from_disk(&d.0).unwrap();
    let (plan, _) = plan_scope(&l, "_word.yml", &apps(&["excel"], TitleChange::Keep)).unwrap();
    fs::write(d.0.join("config/word.yml"), "filter_exec: other\n").unwrap();
    let e = execute(&d.0, &l, &plan).unwrap_err();
    assert!(e.contains("file-changed:"), "{e}");
    assert_eq!(d.read("config/word.yml"), "filter_exec: other\n");
}

/// Dry run against a real espanso config directory, read-only: set `XPANDO_REAL_CONFIG`.
#[test]
#[ignore]
fn dry_run_on_real_config() {
    let Ok(dir) = std::env::var("XPANDO_REAL_CONFIG") else {
        return;
    };
    let l = Layout::from_disk(Path::new(&dir)).unwrap();
    for f in folder_infos(&l) {
        println!(
            "{:<24} {:?} {:?} locked={:?} scope_locked={:?} configs={:?}",
            f.file,
            f.scope.kind,
            f.scope
                .apps
                .iter()
                .map(|a| (&a.name, &a.exe_names))
                .collect::<Vec<_>>(),
            f.locked,
            f.scope_locked,
            f.configs
        );
        if f.locked.is_none() {
            let p = plan_delete(&l, &f.file).unwrap();
            for c in &p.changes {
                println!("   delete: {} {} -> {:?}", c.action, c.file, c.backup);
            }
            let (p, nf) = plan_rename(&l, &f.file, "Renamed Test").unwrap();
            println!("   rename -> {nf}");
            for c in &p.changes {
                println!("     {} {}", c.action, c.file);
            }
        }
        if f.scope_locked.is_none() {
            for spec in [
                apps(&["notepad"], TitleChange::Contains("x".into())),
                ScopeSpec::All,
            ] {
                match plan_scope(&l, &f.file, &spec) {
                    Ok((p, nf)) => {
                        println!("   scope -> {nf}");
                        for c in &p.changes {
                            println!("     {} {}", c.action, c.file);
                        }
                    }
                    Err(e) => println!("   scope refused: {e}"),
                }
            }
        }
    }
}
