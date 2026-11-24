//! "Works in" for one phrase: moving a phrase to the folder whose apps match what the user
//! chose.
//!
//! espanso has no per-phrase app filter, so xpando maps the choice onto folders: a phrase that
//! should work only in Word lives in a folder that only Word loads. This module finds that
//! folder (a folder whose apps are exactly the chosen programs, written as a plain list of
//! program names), or plans one (create the folder, give it the apps with the same machinery as
//! [`set_scope`]) and then moves the entry from the source file to the destination file. The
//! entry travels as raw text with its comment block ([`crate::edit::take_entry`] and
//! [`crate::edit::put_entry`]), is re-parsed and compared with the original, and both files are
//! written in one plan that is checked, applied step by step and undone if any step fails.
//!
//! Folders with other filters (a window title, a class, a hand-written program pattern) are
//! never matched and never changed; a new folder is created instead and the preview says so.

use super::*;

use crate::conflicts::{find_conflicts_in_texts, Certainty, ConflictKind};
use crate::edit::{put_entry, take_entry, EntryBlock};

/// Where a phrase should work.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhraseWhere {
    /// In every app: the phrase goes to an all-apps folder, `folder` if given (the one it came
    /// from), else the default one (`base.yml`, or the first all-apps folder).
    All {
        #[serde(default)]
        folder: Option<String>,
    },
    /// Only in these programs. `names` are friendly names (parallel to `exes`, may be empty)
    /// used to name a new folder.
    Apps {
        exes: Vec<String>,
        #[serde(default)]
        names: Vec<String>,
    },
    /// Into this existing folder, whatever its apps are (used to move a phrase back).
    Folder { file: String },
}

/// A trigger that clashes with the moved phrase in its new folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveConflict {
    pub trigger: String,
    /// The longer trigger of a prefix clash.
    pub other_trigger: Option<String>,
    /// Name of the other phrase.
    pub title: String,
    /// The folder the other phrase is in.
    pub folder: String,
    /// Plain-language sentence for the user.
    pub message: String,
    /// True when the move is refused because of it.
    pub blocking: bool,
}

/// What a move would do. Nothing has been changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MovePreview {
    pub from_file: String,
    pub from_name: String,
    /// The destination file; `None` when the folder does not exist yet.
    pub to_file: Option<String>,
    pub to_name: String,
    pub creates_folder: bool,
    /// The phrase is already where it should be.
    pub unchanged: bool,
    /// One plain-language sentence, for example: This moves the phrase to the folder “Word”.
    pub notice: String,
    /// Things worth knowing that do not stop the move.
    pub warnings: Vec<String>,
    pub conflicts: Vec<MoveConflict>,
    /// Why the move cannot be done, if it cannot.
    pub blocked: Option<String>,
    /// The files that would be touched (Advanced mode shows them).
    pub changes: Vec<Change>,
}

/// What a move did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveOutcome {
    pub from_file: String,
    pub from_name: String,
    /// The destination file (the name can differ from the preview: app folders start with `_`).
    pub to_file: String,
    pub to_name: String,
    /// Position of the phrase in the destination file.
    pub to_index: usize,
    pub created_folder: bool,
    pub changes: Vec<Change>,
}

struct Planned {
    preview: MovePreview,
    plan: Option<Plan>,
    to_file: String,
    to_index: usize,
}

// ---------------------------------------------------------------------------------------
// Program names
// ---------------------------------------------------------------------------------------

/// The comparison key of a program name: trimmed, lower case, without `.exe`.
pub fn exe_key(raw: &str) -> String {
    let n = raw.trim();
    let n = if n.to_lowercase().ends_with(".exe") {
        &n[..n.len() - 4]
    } else {
        n
    };
    n.trim().to_lowercase()
}

/// Distinct programs of a request as `(key, friendly name)`, in the order given.
fn requested(exes: &[String], names: &[String]) -> Result<Vec<(String, String)>, String> {
    // Validates every name the same way the folder's filter would.
    exec_pattern(exes)?;
    let mut out: Vec<(String, String)> = Vec::new();
    for (i, e) in exes.iter().enumerate() {
        let key = exe_key(e);
        if out.iter().any(|(k, _)| *k == key) {
            continue;
        }
        let friendly = names
            .get(i)
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
            .map_or_else(|| capitalize(&key), String::from);
        out.push((key, friendly));
    }
    Ok(out)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// A folder name for these programs, for example `Word` or `Word, Outlook`. Only characters
/// [`validate_name`] accepts are kept; the result is at most the allowed length.
pub fn auto_folder_name(friendly: &[String]) -> String {
    let clean = |s: &str| -> String {
        let kept: String = s
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.') {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        kept.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_matches(['_', '-', '.'])
            .to_string()
    };
    let names: Vec<String> = friendly
        .iter()
        .map(|n| clean(n))
        .filter(|n| !n.is_empty())
        .collect();
    let text = match names.len() {
        0 => "Apps".to_string(),
        1..=3 => names.join(", "),
        n => format!("{}, {} and {} more", names[0], names[1], n - 2),
    };
    let text: String = text.chars().take(MAX_NAME_CHARS - 4).collect();
    text.trim_end_matches([' ', ',', '.', '-']).to_string()
}

/// A folder name that collides with nothing, starting from `base`.
fn free_folder_name(l: &Layout, base: &str) -> Option<String> {
    for n in 1..100 {
        let cand = if n == 1 {
            base.to_string()
        } else {
            format!("{base} {n}")
        };
        if validate_name(&cand).is_err() {
            return None;
        }
        let busy = name_taken(l, &cand, None).is_some()
            || l.other_files.contains(&format!("match/{cand}.yml"))
            || l.other_files.contains(&format!("match/_{cand}.yml"))
            || config_file_taken(l, &format!("{cand}.yml"))
            || config_file_taken(l, &format!("{cand}.yaml"));
        if !busy {
            return Some(cand);
        }
    }
    None
}

/// The set of program keys a folder is limited to, when it is limited to nothing else: only a
/// plain list of program names, no window title, class or system filter. `None` for every
/// other folder.
fn simple_app_set(scope: &Scope) -> Option<BTreeSet<String>> {
    if scope.kind != ScopeKind::Only || scope.apps.is_empty() {
        return None;
    }
    let mut set = BTreeSet::new();
    for a in &scope.apps {
        if !a.exec_simple
            || a.exe_names.is_empty()
            || a.title_pattern.is_some()
            || a.class_pattern.is_some()
            || a.os.is_some()
        {
            return None;
        }
        set.extend(a.exe_names.iter().map(|n| exe_key(n)));
    }
    Some(set)
}

// ---------------------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------------------

fn quote(name: &str) -> String {
    format!("\u{201c}{name}\u{201d}")
}

fn read_text(dir: &Path, file: &str) -> Result<String, String> {
    let path = dir.join("match").join(file);
    let bytes = fs::read(&path).map_err(|e| format!("Could not read match/{file}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("match/{file} is not valid UTF-8."))
}

fn read_only(dir: &Path, file: &str) -> bool {
    fs::metadata(dir.join("match").join(file)).is_ok_and(|m| m.permissions().readonly())
}

fn blocked_preview(from_file: &str, to_name: &str, why: String) -> Planned {
    Planned {
        preview: MovePreview {
            from_file: from_file.to_string(),
            from_name: display_name(from_file),
            to_file: None,
            to_name: to_name.to_string(),
            creates_folder: false,
            unchanged: false,
            notice: String::new(),
            warnings: Vec::new(),
            conflicts: Vec::new(),
            blocked: Some(why),
            changes: Vec::new(),
        },
        plan: None,
        to_file: String::new(),
        to_index: 0,
    }
}

enum Dest {
    /// Already in the right folder.
    Same,
    Existing(String),
    New {
        name: String,
        exes: Vec<String>,
    },
}

/// Chooses the destination, or explains why there is none.
fn choose_dest(
    l: &Layout,
    src: &str,
    wh: &PhraseWhere,
    warnings: &mut Vec<String>,
) -> Result<Dest, String> {
    let infos = folder_infos(l);
    let src_info = infos
        .iter()
        .find(|i| i.file == src)
        .ok_or("The phrase's folder is not known.")?;
    match wh {
        PhraseWhere::Folder { file } => {
            ensure_known(l, file)?;
            if file == src {
                return Ok(Dest::Same);
            }
            if manageable(file).is_err() {
                return Err("That folder belongs to espanso and is not changed here.".into());
            }
            Ok(Dest::Existing(file.clone()))
        }
        PhraseWhere::All { folder } => {
            if src_info.scope.kind == ScopeKind::All && folder.as_ref().is_none_or(|f| f == src) {
                return Ok(Dest::Same);
            }
            let target =
                match folder {
                    Some(f) => {
                        ensure_known(l, f)?;
                        f.clone()
                    }
                    None => {
                        let mut all = infos.iter().filter(|i| {
                            i.scope.kind == ScopeKind::All && manageable(&i.file).is_ok()
                        });
                        let base = infos.iter().find(|i| {
                            i.file.to_lowercase() == "base.yml" && i.scope.kind == ScopeKind::All
                        });
                        match base.or_else(|| all.next()) {
                            Some(i) => i.file.clone(),
                            None => return Err(
                                "There is no folder that works in all apps yet. Create one first."
                                    .into(),
                            ),
                        }
                    }
                };
            if target == src {
                return Ok(Dest::Same);
            }
            let ti = infos
                .iter()
                .find(|i| i.file == target)
                .ok_or("That folder is not known.")?;
            if ti.scope.kind != ScopeKind::All {
                return Err(format!(
                    "The folder {} is limited to certain apps, so it cannot take a phrase that \
                     should work everywhere.",
                    quote(&ti.name)
                ));
            }
            if manageable(&target).is_err() {
                return Err("That folder belongs to espanso and is not changed here.".into());
            }
            Ok(Dest::Existing(target))
        }
        PhraseWhere::Apps { exes, names } => {
            let want = requested(exes, names)?;
            if want.is_empty() {
                return Err("Choose at least one program.".into());
            }
            let want_set: BTreeSet<String> = want.iter().map(|(k, _)| k.clone()).collect();
            let mut same_apps_but_custom: Vec<String> = Vec::new();
            let mut found: Option<String> = None;
            for i in &infos {
                if manageable(&i.file).is_err() {
                    continue;
                }
                match simple_app_set(&i.scope) {
                    Some(set) if set == want_set => {
                        if i.file == src {
                            return Ok(Dest::Same);
                        }
                        if found.is_none() {
                            found = Some(i.file.clone());
                        }
                    }
                    Some(_) => {}
                    None => {
                        // A folder for the same programs that also has another filter is
                        // left alone; say so.
                        if i.scope.kind == ScopeKind::Only
                            && i.scope.apps.iter().any(|a| {
                                a.exe_names
                                    .iter()
                                    .map(|n| exe_key(n))
                                    .collect::<BTreeSet<_>>()
                                    == want_set
                            })
                        {
                            same_apps_but_custom.push(i.name.clone());
                        }
                    }
                }
            }
            if let Some(f) = found {
                return Ok(Dest::Existing(f));
            }
            for n in same_apps_but_custom {
                warnings.push(format!(
                    "The folder {} is for the same programs but has a filter that xpando does \
                     not manage (such as a window title), so it is left as it is and a new \
                     folder is used.",
                    quote(&n)
                ));
            }
            let friendly: Vec<String> = want.iter().map(|(_, f)| f.clone()).collect();
            let base = auto_folder_name(&friendly);
            let name = free_folder_name(l, &base)
                .ok_or("xpando could not find a free name for a new folder.")?;
            Ok(Dest::New {
                name,
                exes: want.into_iter().map(|(k, _)| k).collect(),
            })
        }
    }
}

fn describe(c: &crate::conflicts::Conflict, moved: (&str, usize), blocking: bool) -> MoveConflict {
    let other = c
        .phrases
        .iter()
        .find(|p| (p.file.as_str(), p.index) != moved)
        .cloned();
    let (title, folder) = other
        .as_ref()
        .map(|o| (o.title.clone(), display_name(&o.file)))
        .unwrap_or_default();
    let message = match c.kind {
        ConflictKind::Duplicate => format!(
            "The trigger {} is already used by {} in the folder {}.",
            quote(&c.trigger),
            quote(&title),
            quote(&folder)
        ),
        ConflictKind::Prefix => format!(
            "The trigger {} starts the same as {} ({} in the folder {}), so one can fire \
             before the other is finished.",
            quote(&c.trigger),
            quote(c.other_trigger.as_deref().unwrap_or("")),
            quote(&title),
            quote(&folder)
        ),
    };
    MoveConflict {
        trigger: c.trigger.clone(),
        other_trigger: c.other_trigger.clone(),
        title,
        folder,
        message,
        blocking,
    }
}

/// Conflicts the moved phrase has in its new place.
fn new_conflicts(
    l: &Layout,
    dest: (&str, usize),
    after_texts: &[(String, String)],
    dest_is_app_folder: bool,
) -> Vec<MoveConflict> {
    let infos = folder_infos(l);
    let mut out = Vec::new();
    for c in find_conflicts_in_texts(after_texts) {
        if !c.phrases.iter().any(|p| (p.file.as_str(), p.index) == dest) {
            continue;
        }
        if c.kind == ConflictKind::Prefix && c.low_risk {
            continue;
        }
        let others: Vec<&crate::conflicts::ConflictPhrase> = c
            .phrases
            .iter()
            .filter(|p| (p.file.as_str(), p.index) != dest)
            .collect();
        // The same folder is always loaded together. An app folder also loads every folder
        // that works in all apps, so a repeated trigger there fires for both.
        let together = c.certainty == Certainty::Definite
            || (dest_is_app_folder
                && others.iter().any(|o| {
                    infos
                        .iter()
                        .find(|i| i.file == o.file)
                        .is_some_and(|i| i.scope.kind == ScopeKind::All)
                }));
        let blocking = c.kind == ConflictKind::Duplicate && together;
        out.push(describe(&c, dest, blocking));
    }
    out
}

fn plan_move(
    dir: &Path,
    l: &Layout,
    src: &str,
    index: usize,
    wh: &PhraseWhere,
) -> Result<Planned, String> {
    ensure_known(l, src)?;
    let src_name = display_name(src);
    if let Err(why) = manageable(src) {
        if src.starts_with("packages/") {
            let _ = why;
            return Ok(blocked_preview(
                src,
                "",
                "This phrase belongs to an installed espanso package. Change packages with                  espanso, not here."
                    .into(),
            ));
        }
    }
    if read_only(dir, src) {
        return Ok(blocked_preview(
            src,
            "",
            format!(
                "The folder {} is read-only, so the phrase cannot be moved out of it.",
                quote(&src_name)
            ),
        ));
    }
    let src_text = read_text(dir, src)?;
    let infos = folder_infos(l);
    let src_info = infos.iter().find(|i| i.file == src);
    let mut warnings = Vec::new();
    if let Some(i) = src_info {
        if i.scope.kind == ScopeKind::Only && simple_app_set(&i.scope).is_none() {
            warnings.push(format!(
                "The folder {} is limited by a filter that xpando does not manage (for example a \
                 window title). Moving the phrase out of it drops that limit.",
                quote(&i.name)
            ));
        }
        if i.scope.kind == ScopeKind::Nowhere {
            warnings.push(format!(
                "The folder {} is not used by any config, so this phrase does not expand today.",
                quote(&i.name)
            ));
        }
    }
    let dest = match choose_dest(l, src, wh, &mut warnings) {
        Ok(d) => d,
        Err(why) => return Ok(blocked_preview(src, "", why)),
    };

    let (dest_name, dest_file_now, creates) = match &dest {
        Dest::Same => {
            let mut p = blocked_preview(src, &src_name, String::new());
            p.preview.blocked = None;
            p.preview.unchanged = true;
            p.preview.to_file = Some(src.to_string());
            p.preview.notice = format!("The phrase is already in the folder {}.", quote(&src_name));
            p.preview.warnings = warnings;
            p.to_file = src.to_string();
            return Ok(p);
        }
        Dest::Existing(f) => (display_name(f), Some(f.clone()), false),
        Dest::New { name, .. } => (name.clone(), None, true),
    };
    let blocked = |why: String| Ok(blocked_preview(src, &dest_name, why));

    if let Some(f) = &dest_file_now {
        if read_only(dir, f) {
            return blocked(format!(
                "The folder {} is read-only, so the phrase cannot be added to it.",
                quote(&dest_name)
            ));
        }
    }

    let (src_new, block): (String, EntryBlock) = match take_entry(&src_text, index) {
        Ok(x) => x,
        Err(e) => return blocked(e),
    };
    let dest_text = match &dest_file_now {
        Some(f) => read_text(dir, f)?,
        None => "matches:\n".to_string(),
    };
    let dest_new = match put_entry(&dest_text, &block) {
        Ok(t) => t,
        Err(e) => return blocked(e),
    };
    let to_index = crate::scan_entries(&dest_new)
        .map(|s| s.len().saturating_sub(1))
        .unwrap_or(0);

    // Build the plan.
    let mut plan;
    let to_file;
    match &dest {
        Dest::New { name, exes } => {
            let (mut p1, file1) = plan_create(l, name)?;
            let (p2, file2) = plan_scope(
                &p1.after,
                &file1,
                &ScopeSpec::Apps {
                    exes: Some(exes.clone()),
                    title: TitleChange::Clear,
                },
            )?;
            // The new file is created with the phrase already in it.
            for op in &mut p1.ops {
                if let Op::CreateMatch { name, text } = op {
                    if *name == file1 {
                        *text = dest_new.clone();
                    }
                }
            }
            if let Some(c) = p1.changes.first_mut() {
                c.detail = format!(
                    "A new folder {} for these programs. The phrase is put into it.",
                    quote(name)
                );
            }
            plan = Plan {
                ops: p1.ops.into_iter().chain(p2.ops).collect(),
                changes: p1.changes.into_iter().chain(p2.changes).collect(),
                after: p2.after,
            };
            to_file = file2;
        }
        Dest::Existing(f) => {
            plan = Plan::new(l);
            plan.write_match(
                f,
                dest_new.clone(),
                &dest_text,
                "The phrase is added at the end; nothing else in the file changes.".into(),
            );
            to_file = f.clone();
        }
        Dest::Same => unreachable!("handled above"),
    }
    plan.write_match(
        src,
        src_new,
        &src_text,
        "The phrase and the comment above it are taken out; nothing else in the file changes."
            .into(),
    );

    // Conflicts in the new place.
    let mut after_texts = BTreeMap::new();
    for f in &l.match_files {
        if let Ok(t) = read_text(dir, f) {
            after_texts.insert(f.clone(), t);
        }
    }
    after_texts.insert(src.to_string(), plan_text(&plan, src));
    after_texts.insert(to_file.clone(), dest_new);
    let after_vec: Vec<(String, String)> = after_texts.into_iter().collect();
    let dest_is_app_folder = matches!(dest, Dest::New { .. })
        || infos
            .iter()
            .any(|i| Some(&i.file) == dest_file_now.as_ref() && i.scope.kind == ScopeKind::Only);
    let conflicts = new_conflicts(l, (&to_file, to_index), &after_vec, dest_is_app_folder);
    let blocked_by = conflicts.iter().find(|c| c.blocking).map(|c| {
        format!(
            "{} Change one of the triggers first, then move the phrase.",
            c.message
        )
    });

    let notice = match (&dest, wh) {
        (Dest::New { .. }, _) => format!(
            "This creates the folder {} for these programs and moves the phrase to it.",
            quote(&dest_name)
        ),
        (_, PhraseWhere::All { .. }) => format!(
            "This moves the phrase to the folder {}, which works in all apps.",
            quote(&dest_name)
        ),
        _ => format!("This moves the phrase to the folder {}.", quote(&dest_name)),
    };
    let changes = plan.changes.clone();
    Ok(Planned {
        preview: MovePreview {
            from_file: src.to_string(),
            from_name: src_name,
            to_file: dest_file_now.clone().map(|_| to_file.clone()),
            to_name: dest_name,
            creates_folder: creates,
            unchanged: false,
            notice,
            warnings,
            conflicts,
            blocked: blocked_by,
            changes,
        },
        plan: Some(plan),
        to_file,
        to_index,
    })
}

/// The text a plan writes to match file `name`.
fn plan_text(plan: &Plan, name: &str) -> String {
    plan.ops
        .iter()
        .rev()
        .find_map(|op| match op {
            Op::WriteMatch { name: n, text, .. } if n == name => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------------------

/// What moving phrase `index` of `file` to `wh` would do, without changing anything.
pub fn preview_phrase_where(
    config_dir: &Path,
    file: &str,
    index: usize,
    wh: &PhraseWhere,
) -> Result<MovePreview, String> {
    let l = Layout::from_disk(config_dir)?;
    Ok(plan_move(config_dir, &l, file, index, wh)?.preview)
}

/// Moves phrase `index` of `file` to the folder that fits `wh`, creating it if needed.
/// `expected_hash` is the hash the source folder had when the UI loaded it.
pub fn set_phrase_where(
    config_dir: &Path,
    file: &str,
    index: usize,
    wh: &PhraseWhere,
    expected_hash: &str,
) -> Result<MoveOutcome, String> {
    let l = Layout::from_disk(config_dir)?;
    let planned = plan_move(config_dir, &l, file, index, wh)?;
    if let Some(why) = &planned.preview.blocked {
        return Err(why.clone());
    }
    let Some(plan) = planned.plan else {
        return Err("The phrase is already where it should be.".into());
    };
    check_hash(config_dir, file, expected_hash)?;
    execute(config_dir, &l, &plan)?;
    Ok(MoveOutcome {
        from_file: file.to_string(),
        from_name: planned.preview.from_name,
        to_name: display_name(&planned.to_file),
        to_file: planned.to_file,
        to_index: planned.to_index,
        created_folder: planned.preview.creates_folder,
        changes: plan.changes,
    })
}

#[cfg(test)]
#[path = "phrases_tests.rs"]
mod tests;
