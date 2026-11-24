# xpando — Implementation Plan

xpando is a fork of [espanso](https://github.com/espanso/espanso) that keeps espanso's
cross-platform text-expansion engine and adds a modern desktop app with PhraseExpress-style
phrase management.

- **Repo:** https://github.com/ultimatejimmy/xpando (fork of `espanso/espanso`, default branch `dev`)
- **License:** GPL-3.0 (inherited; all xpando code is GPL-3.0)
- **Audience:** public open-source app — polish, onboarding, installers and auto-update matter
- **Platforms:** Windows first-class (primary dev/test); macOS and Linux built in CI and kept working
- **UI stack:** Tauri 2 (Rust backend + web frontend in a native window, WebView2 on Windows)

## Decisions so far

| Topic | Decision |
|---|---|
| Base | Fork espanso, minimize edits to engine crates so upstream merges stay cheap |
| Frontend shell | Tauri — native window, tray icon, installers; not a browser tab |
| First-release scope | Phrase manager GUI |
| Rich text | Full WYSIWYG editor, stored as espanso's existing `html:` match field |
| Relationship to espanso | xpando is its own app with its own engine; it does not support running on stock espanso. People coming from espanso get their YAML imported automatically (see M6) |
| Data location | xpando keeps its data in its own folder, not espanso's. The user can choose that folder (including OneDrive, Google Drive, etc.) to back up or share between devices (see M6) |
| Visual direction | **A + D hybrid**: Windows 11 Fluent look (A) with an **Advanced** switch that reveals Workbench (D) power tools — see below |

## Iconography and branding

- **Accent color:** green (default `#107C10`, white text on it passes 4.5:1). The panda logo stays
  black (`#1B1B1B`, white in dark mode), not tinted with the accent.
- **Icons:** [Lucide](https://lucide.dev) everywhere in the app UI (no emoji, no mixed icon sets).
  Use the `lucide` family package with an exact pinned version, installed per org npm policy
  (internal registry, `--ignore-scripts`, ≥3 days old, committed lockfile); import only the
  icons used so unused ones tree-shake out. As of 2026-11-23 `lucide-static` latest is 1.78.0
  (published today, so too new) — pick the newest release ≥3 days old and verify on socket.dev first.
- **Logo:** Lucide [`panda`](https://lucide.dev/icons/panda) (ISC license). Used for the app/window
  icon, tray icon and installers. Needs rasterizing to `.ico`/`.icns`/PNG sets for the Tauri bundle
  (tray variant should be a simplified monochrome version for dark/light taskbars).

- **Wordmark font:** Fredoka SemiBold (OFL, bundled with the app) for the "xpando" wordmark only;
  UI text stays in Segoe UI Variable (system font) on Windows. Logo lockup: panda slightly smaller than the text height, both black.

## Design principles (decided while building)

- **Normal mode is simple; Advanced mode adds more.** Each control is essential (always visible),
  occasional (behind one "More options" disclosure) or technical (Advanced only). Nothing is removed,
  only moved. New features must be classified the same way when they are added.
- **One word for one thing.** The user-facing term is **macro** (a dynamic piece such as a date or the
  clipboard, stored as an espanso `vars` entry); its fields are **settings**. See
  [TERMS.md](TERMS.md) for the glossary, which the wiki reuses. Technical names appear only in Advanced mode.
- **Edits never lose the user's text.** Unsaved changes are guarded, removals offer Undo, and every file
  write is atomic with a backup.

## UI direction: Fluent + Advanced mode

Default (Advanced off) — clean three-pane Fluent layout for everyone:
folders → phrase list → editor with tabs **Content · Options · Apps**. YAML is never shown.

Advanced on (persisted setting, also toggleable from the header) adds:

- **YAML tab** — the phrase's exact YAML with line numbers, validation status, "Show whole file",
  and "Open in editor" (external editor). **Editable** (M2c below): the YAML tab has an Edit mode for
  the selected entry, with live validation, and saves through the same safety net as the form editor.
- **Variables tab** — table view of `vars` (name, type, params) for direct editing.
- **File awareness** — backing file names next to folders, `file · line` under the phrase title.
- **Hotkey badges** in the phrase list next to trigger chips.
- **Search operators** — `trigger:`, `label:`, `folder:`, `file:`, `comment:`, `body:`.
- **Trigger-conflict panel** in the sidebar (duplicates and prefix collisions).
- **Status bar** — engine state, config path, phrase/file counts, Reload.

Mockups: "A+D" row on the design canvas (Content tab and YAML tab views; the Advanced switch and tabs work in Play).

## Architecture

```
┌──────────────────────────── xpando app (new) ───────────────────────────┐
│  Web UI (TypeScript)  ◄── Tauri commands/events ──►  Rust backend        │
│  - phrase tree, editor, search, settings            - uses espanso-config│
│                                                     - format-preserving  │
│                                                       YAML writer        │
│                                                     - daemon control/IPC │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │ writes match/*.yml, config/*.yml
                                ▼
                 espanso config dir (unchanged format)
                                │ file watcher → hot reload (existing)
                                ▼
┌──────────────────── espanso daemon (existing engine) ───────────────────┐
│ detect → match → render → inject, clipboard, forms (espanso-modulo)     │
└─────────────────────────────────────────────────────────────────────────┘
```

Key principles:

1. **YAML stays the source of truth.** Power users can keep hand-editing; the app and the
   files never disagree. No separate database for phrases.
2. **Format-preserving writes.** `serde_yaml` drops comments and reorders keys. The backend
   must edit YAML via a CST/format-preserving approach (evaluate `yaml-edit`/`saphyr`-based
   editing, or a narrow custom writer) so a user's comments and layout survive app edits.
   This is the single biggest technical risk — prototype it first.
3. **New code lives in new crates.** `xpando-app/` (Tauri) and `xpando-core/` (phrase model,
   YAML read/write, validation). Engine crates only get small, upstreamable patches.
4. **xpando-specific metadata** (folder icons, colors, sort order, usage counts) lives in a
   sidecar file (e.g. `xpando.meta.json`), never in match YAML, so files stay espanso-compatible.

## Phrase model mapping

| UI concept | espanso representation |
|---|---|
| Folder | A match file (`match/<folder>.yml`) or subdirectory; nested folders → subdirectories |
| Phrase | One entry in `matches:` |
| Trigger(s) | `trigger` / `triggers` / `regex` |
| Description | `label` |
| Plain-text body | `replace` |
| Rich-text body (WYSIWYG) | `html` (with plain-text fallback generated for apps without rich paste) |
| Markdown body | `markdown` |
| Image | `image_path` |
| Fill-in fields | `form` + `form_fields` |
| Macros (date, clipboard, shell, script…) | `vars` |
| Options | `word`, `propagate_case`, `force_mode`, `search_terms`, etc. |
| App-specific folders | `config/*.yml` with `filter_exec`/`filter_title` + `includes` |

## Milestones

### M0 — Foundation (≈1 week)
- [ ] Install Rust stable + MSVC build tools; build and run stock espanso on Windows.
- [ ] Rebrand plan: binary/app name, config dir strategy (default: **share espanso's config
      dir** so existing users can switch with zero migration; revisit before 1.0).
- [ ] Scaffold `xpando-app` (Tauri 2) and `xpando-core` as workspace members.
- [ ] Frontend toolchain: Vite + TypeScript + chosen UI framework. Per org policy: internal
      registry, exact pinned versions ≥3 days old, `--ignore-scripts`, committed lockfile,
      `npm ci` in CI. Each package confirmed before install.
- [ ] CI (GitHub Actions): build + test on windows-latest, macos-latest, ubuntu-latest.

### M1 — Read-only phrase browser (≈2 weeks)
- [ ] `xpando-core`: load all match files via `espanso-config`, build folder/phrase tree.
- [ ] UI: sidebar folder tree, phrase list, detail pane, global search (trigger/label/body).
- [ ] Tray icon: open manager, pause/resume expansion, quit.
- [ ] Daemon status (running/paused) via existing CLI/IPC.

### M2 — Editing (≈3 weeks)
- [ ] Format-preserving YAML writer with round-trip tests against espanso's sample configs.
- [ ] Create/rename/move/delete phrases and folders; drag-and-drop.
- [ ] Plain-text editor with live preview; trigger validation and **conflict detection**
      (duplicate triggers, prefix collisions such as `:ad` vs `:addr`).
- [ ] Undo/redo; external-change detection (file edited outside the app → reload/merge prompt).
- [x] First editor: triggers, label, plain-text body; create and delete; hash check, atomic save, `.bak`.
- [x] M2b: match options (`word`, `propagate_case`, `force_mode`, ...), editable description comment,
      unsaved-changes guard, draft kept across a reload conflict, per-folder app scope.
- [x] **M2c: edit the YAML directly (Advanced mode).** In the YAML tab an Edit button turns the
      entry's source into an editable monospace text area (line numbers, tab/indent handling).
      Rules: the text must parse as exactly ONE entry; the result is re-parsed and every other phrase
      must be unchanged; invalid YAML is shown inline with line and column and cannot be saved; the file
      hash check, atomic write and `.bak` backup apply as for form edits. The comment block above the
      entry is part of the editable text. A second step offers editing the whole file (all entries,
      top-level keys such as `imports`/`global_vars`) with the same validation, for power users.
      Without a code-editor library at first (plain text area); CodeMirror can be evaluated later,
      subject to the package approval rules.

### M3 — WYSIWYG rich text (≈3–4 weeks)
- [ ] Editor component (evaluate TipTap/ProseMirror vs Lexical) limited to formatting espanso
      can paste reliably: bold/italic/underline, lists, links, headings, inline images, tables (stretch).
- [ ] Serialize to `html:`; sanitize; auto-generate plain-text fallback.
- [ ] Per-app paste testing matrix (Outlook, Word, Gmail, Slack, Teams, plain-text fields).
- [ ] Upstream fix candidates if espanso's HTML clipboard injection has gaps.

### M4 — Macros & forms builder (≈2–3 weeks)
- [ ] Insert-macro menu in editor: date/time, clipboard, cursor position, shell, script,
      choice, random, nested phrase → generates `vars` + `{{placeholders}}`.
- [ ] Visual form-field designer (text, multiline, choice, list) → `form`/`form_fields`.

### M4b — Native "set clipboard" action (≈1–2 weeks, engine patch)
Today the only way to put text on the clipboard after an expansion is a `shell` variable running
PowerShell (`Set-Clipboard -Value $env:ESPANSO_OUT`), as in the PhraseExpress-converted `TDC` phrase
(`{#datetime}_{#insertclipboard}{#setclipboard}`). That is slow, Windows-only, spawns a process, and
is invisible as a feature. Replace it with a built-in action that works the same on every OS.

- **Engine:** new variable type (working name `set_clipboard`) in `espanso-render/src/extension/`,
  modelled on the existing `clipboard.rs` (reader). It takes `text:` (placeholders allowed, e.g.
  `"{{d}}_{{clip}}"`) and writes it with the existing `espanso-clipboard` `set_text` (already
  implemented for Win32, X11, Wayland and macOS). A small `ClipboardWriter` provider trait mirrors
  `ClipboardProvider` so the extension stays unit-testable. Wire it up where the reader is wired
  (`espanso/src`). Output is empty, so it composes with other variables.
- **Ordering risk:** espanso saves and restores the user's clipboard around a paste-based
  injection. The new action must write *after* that restore, or its result is lost. Needs an
  explicit "after injection" step in the engine; design and test this first, per OS.
- **Compatibility:** keep `shell`/`script` variables working unchanged. The app can offer a
  one-click "Convert to Set clipboard" for phrases that match the PowerShell pattern. Stock espanso
  would not understand the new type, so xpando-written files using it are xpando-only; say so in
  the UI and docs, and offer to upstream the patch.
- **UI:** "Set clipboard" in the Insert-macro menu (M4), shown in the Variables list with its text.
- **Tests:** unit tests for the extension, plus a manual per-OS matrix (paste vs type injection,
  clipboard restore on/off, empty and multi-line text, images on the clipboard beforehand).

### Status of M4b (native set clipboard)
Implemented in the engine as variable type `set_clipboard` (see [SET-CLIPBOARD.md](SET-CLIPBOARD.md)) and offered in the
Insert macro menu. Engine detection (`engine_status`, status line, warning for stock) and the one-click, previewed "convert the
PowerShell hack" action are done; see [ENGINE.md](ENGINE.md). Still open: the per-OS manual test matrix (macOS,
Linux, real clipboards), and actually building and bundling the engine (`externalBin` plan and CI outline are in
ENGINE.md, not enabled).

### M5 — Release polish (≈2 weeks)
- [ ] First-run onboarding (permissions on macOS, start-at-login, import existing espanso config).
- [ ] Settings UI for `config/default.yml` (backend, toggle key, search shortcut, etc.).
- [ ] Installers: **NSIS** (Windows; MSI can't hold date-based versions, see RELEASING.md), DMG (macOS,
      signing + notarization needs Apple Developer account), AppImage/deb (Linux). Tauri updater for
      auto-update, with a stable channel and an opt-in beta channel.
- [ ] **Date-based versioning and releases** as described in [RELEASING.md](RELEASING.md): tags like
      `2026.11.23`, patches `2026.11.23.1`, betas `2026.11.23-beta`, `-beta2`; betas are GitHub
      **pre-releases**. Build a `scripts/version` tool that maps the human version to a valid semver for
      the manifests, and a release workflow that validates tag, manifests and changelog agree.
- [ ] **GitHub Wiki** (M5b below).
- [ ] First public release after the wiki and docs are in place.

### M5b — GitHub Wiki
The user-facing documentation lives in the repository's GitHub Wiki.
- **Setup:** enable the Wiki on `ultimatejimmy/xpando` (Settings > Features) and create the first
  page in the browser once; GitHub only creates `xpando.wiki.git` after that.
- **Source of truth:** write the pages as Markdown in `docs/wiki/` so changes are reviewed in pull
  requests, and publish them to the wiki with a small GitHub Actions workflow (needs
  `contents: write`) on every merge to `dev`. Nobody edits the wiki by hand, so it can't drift.
- **Pages:** Home; Getting started; Install (Windows, macOS, Linux); A tour of the app; Phrases and
  triggers; Match options; Variables and macros (dates, clipboard, shell, set clipboard, choices);
  Rich text; Folders and per-app scope; Advanced mode and editing YAML; Search operators; Trigger
  conflicts; Using an existing espanso setup (shared config folder, backups, what the app changes);
  Migrating from PhraseExpress; Troubleshooting; FAQ; Differences from espanso; Building from
  source; Contributing; Versioning and releases; Release notes.
- **Screenshots:** captured from the app on each OS, stored in the repo (`docs/wiki/images/`), light
  and dark where it matters. Re-shot when the UI changes.
- **Quality bar:** every feature merged from now on gets a wiki page or section in the same PR,
  and the wiki links to RELEASING.md for the version scheme.

### M6 — Own data folder, import and sync
- xpando-owned data folder (default under the user's app-data directory) holding `match/` and `config/`; the bundled engine is started with that folder as its config directory (espanso's `ESPANSO_CONFIG_DIR`), so it never reads or writes the stock espanso folder.
- **Import from espanso:** on first run, detect an existing espanso config (`%APPDATA%\espanso` and the per-OS equivalents) and offer a one-click copy into the xpando folder. Copy only, never move or alter the originals; preview what will be imported; keep `_`-prefixed files and `config/` app filters; report anything the app cannot read.
- **Choose the data folder (optional):** a sensible default (`%APPDATA%\xpando` and per-OS equivalents) means no setup is needed. A setting lets the user point xpando at any folder, including a OneDrive/Google Drive/Dropbox folder. When it changes, xpando asks whether to copy the existing data to the new location (copy, never move; the old folder is left in place). Settings are stored separately from the data.
- **Sync safety:** cloud folders can change underneath us. Keep the content-hash check and atomic saves, watch the folder for outside changes and reload, detect sync-conflict copies (for example `file (conflicted copy).yml`) and surface them, warn when the folder is unreachable or offline instead of silently falling back.
- **Single engine:** detect and warn if a stock espanso daemon is running (it would also expand the same triggers).
- Remove the temporary stock-espanso handling once the engine is bundled.

### Status of M6
Implemented (wiki: [Data folder](../wiki/Data-Folder.md), [Import from espanso](../wiki/Import-from-espanso.md), [Sync tips](../wiki/Sync-Tips.md)):

- **`xpando_core::datadir`**: default folder per OS (`%APPDATA%\xpando`, `~/Library/Application Support/xpando`, `$XDG_CONFIG_HOME/xpando` or `~/.config/xpando`); `Settings` (`data_dir`, `import_handled`) in `settings.yml` inside the app's own config folder (Tauri `app_config_dir`), separate from the data and written atomically (YAML rather than JSON so no new crate is needed; a corrupt file is an error, never a silent fallback to the default); `prepare`/`ensure_layout` (create `match/` and `config/`, but never recreate a missing chosen folder); `check_dir`/`is_reachable`/`is_writable`/`describe_status`; `check_choice` (validates a typed/picked folder: full path, exists or its parent exists, writable, not the same, not nested); `copy_data` and `change_data_dir` (copy only, no overwrite, a copy with failures does not switch).
- **`xpando_core::import`**: per-OS espanso folder candidates (`ESPANSO_CONFIG_DIR`, `%APPDATA%\espanso`, `~/.config/espanso`, macOS Application Support), `preview_import` (files, which would be kept, what is skipped and why) and `run_import` (`match/**` and `config/**` YAML including `_` files and packages; non-YAML, links and conflict copies are reported as skipped; create-new-only writes so nothing is overwritten; the source is only read).
- **`xpando_core::syncsafe`**: `is_conflicted_copy` / `original_name` (Dropbox/Nextcloud "conflicted copy", "case conflict", "Selective Sync Conflict"; Syncthing `.sync-conflict-`; OneDrive `-DESKTOP-`/`-LAPTOP-`; Google Drive/Windows `name (1)`), `find_conflicted_copies`, and re-exports of `is_reachable`/`check_dir`.
- **App**: every command (load, save, folders, advanced, engine checks) resolves its folder through `data::data_dir`, so the espanso folder is never used; `engine_status` starts the engine with `ESPANSO_CONFIG_DIR` set to that folder. UI: first-run import dialog (preview list, Import / Not now / Don't ask again), Settings (gear icon) with data folder, **Change...** (Browse plus a text box, validation, then "Copy my phrases?"), **Use the default folder** and **Find espanso phrases**; banners for an unreachable folder (no fallback; Try again / Settings) and for conflicted copies; the window re-reads the folder when it regains focus.
- **Folder picker without a new package**: the `pick_folder` command runs what the OS already has (PowerShell folder dialog on Windows, `osascript` on macOS, `zenity` or `kdialog` on Linux). The text box always works, so a missing picker is not a dead end. Switching to `tauri-plugin-dialog` later needs a Cargo and npm dependency.
- No new Rust crates and no new npm packages.

Still open: a live file watcher (today the folder is re-read on window focus, and saves keep the hash check); running the daemon from the app with the data folder (the engine is only queried for `--version` so far, with `ESPANSO_CONFIG_DIR` set); detecting a running stock espanso daemon; per-OS manual tests (macOS and Linux paths and pickers are untested here; the Windows picker and the full app flow were not exercised in a running window); removing the stock-espanso handling once the engine is bundled.

### Later (post-0.1)
- Modern hotkey popup / search bar replacing espanso-modulo's wxWidgets search UI.
- Modern forms window replacing modulo forms.
- Clipboard history, autotext suggestions, usage statistics.
- Shared/team phrase libraries (Git- or folder-backed, read-only subscriptions).
- Import from PhraseExpress (`.pxp` export) and other expanders.

## Risks

| Risk | Mitigation |
|---|---|
| YAML round-trip loses comments/formatting | Prototype in M0/M1; extensive round-trip tests; fall back to rewriting only touched nodes |
| Rich text pastes inconsistently across apps | Constrain editor feature set; per-app test matrix; plain-text fallback |
| Upstream drift | Keep engine patches small; merge `upstream/dev` monthly; upstream generic fixes |
| Native set-clipboard needs an engine change and files using it break in stock espanso | Isolate it in one extension file; flag xpando-only phrases in the UI; offer the patch upstream |
| macOS/Linux regressions unnoticed | CI on all three; recruit community testers; label platform issues |
| Name/branding confusion with espanso | Clear README attribution; distinct name/icon; GPL compliance |

## Open questions

1. ~~Share espanso's config directory, or use a separate `xpando` dir with an importer?~~ Decided: separate xpando folder plus importer (M6).
2. Ship our own build of the espanso daemon (rebranded) or depend on users installing espanso?
   (Recommendation: ship it bundled — single installer is far better UX.)
3. Frontend framework: React (largest ecosystem, TipTap/Lexical support) vs Svelte (lighter).
4. Pick a design direction from the options.
