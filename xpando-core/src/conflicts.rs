//! Trigger-conflict detection: the same trigger defined twice, and triggers that are a
//! strict prefix of another one.
//!
//! espanso semantics this relies on: a trigger fires as soon as its last character is typed,
//! unless the phrase uses `word` (word boundary on both sides), `left_word` or `right_word`.
//! So with `:ad` and `:addr`, typing `:addr` fires `:ad` first and `dr` is left over. That is
//! a high risk unless `:ad` is right-word restricted, in which case it only fires when a
//! separator follows, and the longer trigger's next character is not a separator.
//! Regex entries are ignored. Match files loaded only for certain apps (`config/*.yml`
//! `includes`) may reuse a trigger on purpose, so conflicts between different files are
//! reported as `Possible` and conflicts inside one file as `Definite`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use serde_norway::Value;

use crate::{load_folders, Folder, Phrase};

/// Word-boundary option of a phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WordMode {
    /// Fires as soon as the trigger is typed.
    #[default]
    None,
    /// `word: true` (or both `left_word` and `right_word`).
    Word,
    LeftWord,
    RightWord,
}

impl WordMode {
    fn from_flags(word: bool, left: bool, right: bool) -> Self {
        match (word || (left && right), left, right) {
            (true, _, _) => WordMode::Word,
            (_, true, _) => WordMode::LeftWord,
            (_, _, true) => WordMode::RightWord,
            _ => WordMode::None,
        }
    }

    /// True when the trigger only fires once a word separator follows it.
    fn right_restricted(self) -> bool {
        matches!(self, WordMode::Word | WordMode::RightWord)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// The same trigger on more than one phrase.
    Duplicate,
    /// `trigger` is a strict prefix of `other_trigger`.
    Prefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// Two of the phrases are in the same file, so both are always loaded together.
    Definite,
    /// Only across files, which may be app-specific and never active together.
    Possible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Duplicate,
    /// Has the shorter trigger, which fires first.
    Shorter,
    Longer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictPhrase {
    pub file: String,
    pub index: usize,
    pub trigger: String,
    pub title: String,
    pub word: WordMode,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub certainty: Certainty,
    /// For a prefix collision: the shorter trigger fires on its own, and the longer one is
    /// shadowed. `true` when word options make a real collision unlikely.
    pub low_risk: bool,
    /// The duplicated trigger, or the shorter trigger of a prefix collision.
    pub trigger: String,
    /// The longer trigger of a prefix collision.
    pub other_trigger: Option<String>,
    pub phrases: Vec<ConflictPhrase>,
}

/// Word options per phrase, keyed by `(file, entry index)`.
type Flags = HashMap<(String, usize), WordMode>;

fn truthy(map: &serde_norway::Mapping, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Reads `word` / `left_word` / `right_word` of every entry of `matches:` in `text`.
fn word_modes(text: &str) -> Vec<WordMode> {
    let Ok(doc) = serde_norway::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(entries) = doc
        .as_mapping()
        .and_then(|m| m.get("matches"))
        .and_then(Value::as_sequence)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|e| match e.as_mapping() {
            Some(m) => WordMode::from_flags(
                truthy(m, "word"),
                truthy(m, "left_word"),
                truthy(m, "right_word"),
            ),
            None => WordMode::None,
        })
        .collect()
}

fn phrase_title(p: &Phrase) -> String {
    if let Some(l) = p.label.as_deref().filter(|l| !l.is_empty()) {
        return l.to_string();
    }
    match p.comment.as_deref() {
        Some(c) if !c.contains('\n') && c.chars().count() <= 40 => c.to_string(),
        _ => p.triggers.first().cloned().unwrap_or_default(),
    }
}

/// Finds trigger conflicts, treating every phrase as having no word options.
/// Use [`find_conflicts_in_dir`] to take `word` options into account.
pub fn find_conflicts(folders: &[Folder]) -> Vec<Conflict> {
    detect(folders, &Flags::new())
}

/// Loads the match files under `config_dir` and finds conflicts, reading each phrase's
/// `word` / `left_word` / `right_word` options from the files.
pub fn find_conflicts_in_dir(config_dir: &Path) -> io::Result<Vec<Conflict>> {
    let folders = load_folders(config_dir)?;
    let mut flags = Flags::new();
    for f in &folders {
        if let Ok(text) = fs::read_to_string(config_dir.join("match").join(&f.file)) {
            for (i, m) in word_modes(&text).into_iter().enumerate() {
                flags.insert((f.file.clone(), i), m);
            }
        }
    }
    Ok(detect(&folders, &flags))
}

fn detect(folders: &[Folder], flags: &Flags) -> Vec<Conflict> {
    // trigger -> distinct phrases using it, in file order.
    let mut by_trigger: BTreeMap<&str, Vec<ConflictPhrase>> = BTreeMap::new();
    for f in folders {
        for p in f.phrases.iter().filter(|p| !p.is_regex) {
            let word = flags
                .get(&(f.file.clone(), p.index))
                .copied()
                .unwrap_or_default();
            let mut seen = BTreeSet::new();
            for t in p.triggers.iter().filter(|t| !t.is_empty()) {
                if seen.insert(t.as_str()) {
                    by_trigger.entry(t).or_default().push(ConflictPhrase {
                        file: f.file.clone(),
                        index: p.index,
                        trigger: t.clone(),
                        title: phrase_title(p),
                        word,
                        role: Role::Duplicate,
                    });
                }
            }
        }
    }

    let mut out = Vec::new();
    for (trigger, phrases) in &by_trigger {
        if phrases.len() > 1 {
            out.push(Conflict {
                kind: ConflictKind::Duplicate,
                certainty: if shares_file(phrases, phrases) {
                    Certainty::Definite
                } else {
                    Certainty::Possible
                },
                low_risk: false,
                trigger: trigger.to_string(),
                other_trigger: None,
                phrases: phrases.clone(),
            });
        }
    }

    let keys: Vec<&str> = by_trigger.keys().copied().collect();
    for short in &keys {
        for long in &keys {
            if long.len() <= short.len() || !long.starts_with(short) {
                continue;
            }
            let shorter = &by_trigger[short];
            let longer = &by_trigger[long];
            let next_is_word_char = long[short.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric());
            let low_risk = next_is_word_char && shorter.iter().all(|p| p.word.right_restricted());
            let mut phrases: Vec<ConflictPhrase> = Vec::new();
            phrases.extend(shorter.iter().cloned().map(|p| ConflictPhrase {
                role: Role::Shorter,
                ..p
            }));
            phrases.extend(longer.iter().cloned().map(|p| ConflictPhrase {
                role: Role::Longer,
                ..p
            }));
            out.push(Conflict {
                kind: ConflictKind::Prefix,
                certainty: if shares_file(shorter, longer) {
                    Certainty::Definite
                } else {
                    Certainty::Possible
                },
                low_risk,
                trigger: short.to_string(),
                other_trigger: Some(long.to_string()),
                phrases,
            });
        }
    }
    out
}

/// True when a phrase of `a` and a different phrase (or trigger) of `b` live in one file.
fn shares_file(a: &[ConflictPhrase], b: &[ConflictPhrase]) -> bool {
    a.iter().any(|x| {
        b.iter()
            .any(|y| x.file == y.file && (x.index != y.index || x.trigger != y.trigger))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(index: usize, triggers: &[&str], label: Option<&str>) -> Phrase {
        Phrase {
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            is_regex: false,
            label: label.map(String::from),
            kind: "replace".into(),
            content: "x".into(),
            preview: "x".into(),
            vars: vec![],
            comment: None,
            index,
            line: None,
        }
    }

    fn folder(file: &str, phrases: Vec<Phrase>) -> Folder {
        Folder {
            name: file.trim_end_matches(".yml").into(),
            file: file.into(),
            phrases,
            warnings: vec![],
            hash: String::new(),
        }
    }

    fn of_kind(c: &[Conflict], kind: ConflictKind) -> Vec<&Conflict> {
        c.iter().filter(|c| c.kind == kind).collect()
    }

    #[test]
    fn no_conflicts_for_distinct_triggers() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":a"], None), phrase(1, &[":b"], None)],
        );
        assert!(find_conflicts(&[f]).is_empty());
    }

    #[test]
    fn duplicate_in_one_file_is_definite() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":a"], None), phrase(1, &[":a"], None)],
        );
        let c = find_conflicts(&[f]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, ConflictKind::Duplicate);
        assert_eq!(c[0].certainty, Certainty::Definite);
        assert_eq!(c[0].trigger, ":a");
        assert_eq!(c[0].phrases.len(), 2);
        assert!(!c[0].low_risk);
    }

    #[test]
    fn duplicate_across_files_is_possible() {
        let a = folder("a.yml", vec![phrase(0, &[":a"], None)]);
        let b = folder("_b.yml", vec![phrase(0, &[":a"], None)]);
        let c = find_conflicts(&[a, b]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].certainty, Certainty::Possible);
    }

    #[test]
    fn three_way_duplicate_with_two_in_one_file_is_definite() {
        let a = folder(
            "a.yml",
            vec![phrase(0, &[":a"], None), phrase(1, &[":a"], None)],
        );
        let b = folder("b.yml", vec![phrase(0, &[":a"], None)]);
        let c = find_conflicts(&[a, b]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].certainty, Certainty::Definite);
        assert_eq!(c[0].phrases.len(), 3);
    }

    #[test]
    fn same_trigger_twice_in_one_phrase_is_not_a_conflict() {
        let f = folder("a.yml", vec![phrase(0, &[":a", ":a"], None)]);
        assert!(find_conflicts(&[f]).is_empty());
    }

    #[test]
    fn multi_trigger_phrase_conflicts_on_each_trigger() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":a", ":b"], None), phrase(1, &[":b"], None)],
        );
        let c = find_conflicts(&[f]);
        let d = of_kind(&c, ConflictKind::Duplicate);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].trigger, ":b");
    }

    #[test]
    fn prefix_collision_is_reported_with_roles() {
        let f = folder(
            "a.yml",
            vec![
                phrase(0, &[":ad"], None),
                phrase(1, &[":addr"], Some("Address")),
            ],
        );
        let c = find_conflicts(&[f]);
        assert_eq!(c.len(), 1);
        let c = &c[0];
        assert_eq!(c.kind, ConflictKind::Prefix);
        assert_eq!(c.trigger, ":ad");
        assert_eq!(c.other_trigger.as_deref(), Some(":addr"));
        assert_eq!(c.certainty, Certainty::Definite);
        assert!(!c.low_risk);
        assert_eq!(c.phrases[0].role, Role::Shorter);
        assert_eq!(c.phrases[1].role, Role::Longer);
        assert_eq!(c.phrases[1].title, "Address");
    }

    #[test]
    fn prefix_across_files_is_possible() {
        let a = folder("a.yml", vec![phrase(0, &[":ad"], None)]);
        let b = folder("b.yml", vec![phrase(0, &[":addr"], None)]);
        let c = find_conflicts(&[a, b]);
        assert_eq!(c[0].certainty, Certainty::Possible);
    }

    #[test]
    fn equal_triggers_are_not_a_prefix_collision() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":a"], None), phrase(1, &[":a"], None)],
        );
        assert!(of_kind(&find_conflicts(&[f]), ConflictKind::Prefix).is_empty());
    }

    #[test]
    fn unrelated_triggers_with_shared_start_are_not_prefixes() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":ab"], None), phrase(1, &[":ac"], None)],
        );
        assert!(find_conflicts(&[f]).is_empty());
    }

    #[test]
    fn prefix_is_case_sensitive() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":AD"], None), phrase(1, &[":addr"], None)],
        );
        assert!(find_conflicts(&[f]).is_empty());
    }

    #[test]
    fn regex_entries_are_ignored() {
        let mut r = phrase(1, &[":a"], None);
        r.is_regex = true;
        let f = folder("a.yml", vec![phrase(0, &[":a"], None), r]);
        assert!(find_conflicts(&[f]).is_empty());
    }

    #[test]
    fn three_nested_prefixes_give_three_pairs() {
        let f = folder(
            "a.yml",
            vec![
                phrase(0, &[":a"], None),
                phrase(1, &[":ab"], None),
                phrase(2, &[":abc"], None),
            ],
        );
        let c = find_conflicts(&[f]);
        assert_eq!(of_kind(&c, ConflictKind::Prefix).len(), 3);
    }

    #[test]
    fn right_word_on_the_shorter_trigger_is_low_risk() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":ad"], None), phrase(1, &[":addr"], None)],
        );
        let mut flags = Flags::new();
        flags.insert(("a.yml".into(), 0), WordMode::RightWord);
        let c = detect(&[f], &flags);
        assert!(c[0].low_risk);
        assert_eq!(c[0].phrases[0].word, WordMode::RightWord);
    }

    #[test]
    fn left_word_alone_does_not_lower_risk() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":ad"], None), phrase(1, &[":addr"], None)],
        );
        let mut flags = Flags::new();
        flags.insert(("a.yml".into(), 0), WordMode::LeftWord);
        assert!(!detect(&[f], &flags)[0].low_risk);
    }

    #[test]
    fn word_on_the_shorter_trigger_is_low_risk_but_not_hidden() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &["ad"], None), phrase(1, &["addr"], None)],
        );
        let mut flags = Flags::new();
        flags.insert(("a.yml".into(), 0), WordMode::Word);
        let c = detect(&[f], &flags);
        assert_eq!(c.len(), 1);
        assert!(c[0].low_risk);
    }

    #[test]
    fn separator_after_the_prefix_is_never_low_risk() {
        // "ad" then "ad r": a space after the short trigger satisfies right_word.
        let f = folder(
            "a.yml",
            vec![phrase(0, &["ad"], None), phrase(1, &["ad r"], None)],
        );
        let mut flags = Flags::new();
        flags.insert(("a.yml".into(), 0), WordMode::Word);
        assert!(!detect(&[f], &flags)[0].low_risk);
    }

    #[test]
    fn word_flags_are_read_from_text() {
        let text = "matches:\n  - trigger: a\n    replace: x\n    word: true\n  - trigger: b\n    replace: x\n    left_word: true\n  - trigger: c\n    replace: x\n    right_word: true\n  - trigger: d\n    replace: x\n    left_word: true\n    right_word: true\n  - trigger: e\n    replace: x\n";
        assert_eq!(
            word_modes(text),
            [
                WordMode::Word,
                WordMode::LeftWord,
                WordMode::RightWord,
                WordMode::Word,
                WordMode::None
            ]
        );
    }

    #[test]
    fn word_flags_of_invalid_text_are_empty() {
        assert!(word_modes("matches:\n  - [x\n").is_empty());
        assert!(word_modes("").is_empty());
    }

    #[test]
    fn find_conflicts_in_dir_reads_word_options() {
        let dir = std::env::temp_dir().join(format!("xpando-conflicts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("match")).unwrap();
        fs::write(
            dir.join("match/a.yml"),
            "matches:\n  - trigger: \":ad\"\n    replace: x\n    right_word: true\n  - trigger: \":addr\"\n    replace: y\n",
        )
        .unwrap();
        let c = find_conflicts_in_dir(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c[0].low_risk);
        assert_eq!(c[0].phrases[0].word, WordMode::RightWord);
    }

    #[test]
    fn title_falls_back_to_trigger() {
        let f = folder(
            "a.yml",
            vec![phrase(0, &[":a"], None), phrase(1, &[":a"], None)],
        );
        assert_eq!(find_conflicts(&[f])[0].phrases[0].title, ":a");
    }

    #[test]
    fn index_is_the_entry_index_not_list_position() {
        // Entry 0 was unparseable and skipped, so the first phrase has index 1.
        let f = folder(
            "a.yml",
            vec![phrase(1, &[":a"], None), phrase(2, &[":a"], None)],
        );
        let c = find_conflicts(&[f]);
        assert_eq!(c[0].phrases[0].index, 1);
        assert_eq!(c[0].phrases[1].index, 2);
    }
}
