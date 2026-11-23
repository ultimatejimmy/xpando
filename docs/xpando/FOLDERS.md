# Folders and app scope

A xpando **folder** is one espanso match file, `match/<name>.yml`. Which apps a folder applies to
is not stored in the folder: espanso decides it from the files in `config/`. This note records the
espanso rules xpando relies on (read from `espanso-config/src/config/{resolve,store,path}.rs`) and
what xpando does and refuses.

## espanso semantics used

1. **Configs.** `config/default.yml` (or `.yaml`) is the default config. Every other top-level
   `config/*.yml|yaml` is an app-specific config (not recursive). A config that cannot be parsed is
   skipped by espanso.
2. **Inheritance.** Each custom config is loaded with `default.yml` as its parent: any setting the
   custom config leaves out (including `includes`, `extra_includes`, `excludes`, `extra_excludes`,
   `use_standard_includes` and the `filter_*` keys) takes the default's value. A key set in the custom
   config replaces, not extends, the default's list.
3. **Which match files a config loads.** The union of
   `../match/**/[!_]*.yml`, `../match/**/[!_]*.yaml` (the *standard includes*, unless
   `use_standard_includes: false`), `includes` and `extra_includes`, minus `excludes` and
   `extra_excludes`. Entries are globs relative to `config/` (`*`, `**`, `?`, `[..]`, `[!..]`),
   absolute paths are allowed. `includes` and `extra_includes` behave identically.
4. **Leading underscore.** The standard includes skip files whose *name* starts with `_`. That is the
   whole meaning of the underscore: `match/_word.yml` is loaded only by configs that name it,
   `match/base.yml` is loaded by every config. Directory names are not checked. `.bak`/`.tmp` files are
   not `.yml`, so espanso ignores them.
5. **Filters.** `filter_exec`, `filter_title`, `filter_class` are regular expressions searched (not
   anchored) in the active window's executable *path* (full path on Windows), title and class;
   `filter_os` is `windows|macos|linux`. All filters present must match. A config with **no** filter
   never matches an app (its `match_paths` are loaded but never selected).
6. **One active config.** When a window is focused espanso picks the **first** custom config whose
   filters match, otherwise the default. Only that config's match files are active; the default's are
   *not* added. So a folder applies everywhere only if the default and every filtered custom config
   load it. If two custom configs match the same window, which one wins depends on directory read
   order (unconfirmed, not specified by espanso); xpando does not try to resolve this.
7. **`imports:`** inside a match file loads other match files wherever the importer is loaded,
   regardless of underscore or excludes.
8. `exclude_default_matches` (espanso 0.x) **does not exist** in this espanso; unknown config keys are
   ignored, so xpando neither reads nor writes it.

Not confirmed: the exact order in which `read_dir` yields configs on each OS (point 6), and whether
espanso's loader accepts a UTF-8 BOM (xpando preserves one if present).

## Scope as xpando reports it

For each folder: **All apps** (default and every filtered config load it), **Only** a list of
configs, **All except** a list of configs, or **Not used** (nothing loads it, so its phrases never
expand; shown in a warning colour). The label for a config is its `label:` or its file name
capitalised (`word.yml` gives "Word only"). Executable names are shown when `filter_exec` is
`(?i)(a|b)` or xpando's own pattern; anything else is shown as "custom pattern" and is kept unless the
user chooses to replace it.

## Operations

All operations are planned on an in-memory snapshot (`Layout`), checked there (the scope recomputed
from the planned files must be what was intended), then applied step by step with atomic writes. Config
edits go through `save_file` (hash check, one-time `.bak`); renames refuse to overwrite; deleted files
are first copied to `<file>.deleted.bak` (or `.deleted-2.bak`, ...). After applying, the disk is re-read
and must equal the plan; otherwise every step is undone. Every operation has a preview that returns
the exact list of files touched and backups.

* **Create** `match/<name>.yml` with `matches:`. Names: letters/digits/space/`-`/`_`/`.` inside, at most
  64 characters, no separators or `..`, no leading `_`, `-` or `.`, not a Windows reserved name; must not
  collide (case-insensitively, with or without the underscore, `.yml` or `.yaml`) with any folder.
  New folders apply to all apps.
* **Rename.** Renames the file (keeping its underscore and extension) and rewrites every literal
  include/exclude entry that names it, in any config including `default.yml`. If the folder is the only
  one a config loads and that config is named after it (`config/word.yml` for `_word.yml`), the config is
  renamed too. After the plan, the folder's scope must be unchanged (this catches wildcard entries that
  stop matching).
* **Delete** (needs `confirm`). Removes literal entries naming it; a config left holding only
  `filter_*`/`label` keys is deleted (backed up). Other configs keep all comments and keys.
* **Apps.**
  * *All apps*: remove the folder from the configs naming it, drop the leading `_`.
  * *Only these apps*: ensure the file starts with `_`; if exactly one config names the folder and
    names nothing else, edit its `filter_exec`/`filter_title` in place (the rest of the file is
    untouched); otherwise remove the folder from the configs naming it and create `config/<name>.yml`
    with a header comment, the filters and `extra_includes`. Written patterns: `filter_exec` =
    `(?i)(^|[\\/])(a|b)(\.exe)?$` (matches the program file name, not any path containing it);
    `filter_title` = `(?i)` plus the escaped text ("contains"). At least one of the two is required.
    Window titles are plain text, not regular expressions, because xpando cannot validate a regular
    expression and espanso silently skips a config whose regex is invalid.

Config text edits are line based: a block list item or a one-line `key: scalar` can be removed,
replaced or added; trailing `# comments`, quote style, CRLF/LF, BOM and final newline are preserved.
After every edit the new text is re-parsed and must equal the old settings plus exactly the intended
change.

## Refused (with the reason shown to the user)

* Files in sub-folders of `match/` and anything in `match/packages/` (managed by espanso).
* Folders imported by another match file (rename, delete, scope).
* Absolute include paths into the config directory (rename, delete, scope); configs that are not
  UTF-8 (rename, delete); configs that fail to parse, and `default.yml` with filters (scope).
* A reference through a wildcard (`includes: ["../match/_w*.yml"]`), a reference from `default.yml`,
  or an `excludes` entry matching the folder (scope changes only; rename still works and is checked).
* Config layouts the line editor does not understand: inline lists (`[a, b]`), multi-line or block
  scalars, anchors/tags, quoted or duplicate keys, tabs, documents separators, mixed line endings, a
  stray `\r`, or a file whose scanned keys differ from the parsed keys.
* Name collisions (folder names, the target `_name.yml`/`name.yml`, an existing `config/<name>.yml`
  that does not belong to the folder, existing backup names are never overwritten).
* A plan whose re-analysis would not give the requested scope (for example `use_standard_includes:
  false` in another config makes "All apps" unreachable).

## UI rules

Config file names and raw filter patterns are shown only in Advanced mode, except in the delete
confirmation, which always names every file it will touch and its backup.
