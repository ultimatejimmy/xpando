# Contributing

xpando is open source under GPL-3.0. Fixes, ideas and wiki improvements are welcome.

## Where the code is

| Folder | What |
|---|---|
| `xpando-app/` | The Tauri 2 app: a TypeScript interface (`src/`) and a Rust backend (`src-tauri/`). |
| `xpando-core/` | Rust library that reads and edits phrase files safely. |
| `espanso-*`, `espanso/` | The engine, from espanso. Changes here stay small so upstream merges stay easy. |
| `docs/xpando/` | Design notes: plan, terms, folders, rich text, set clipboard, releasing. |
| `docs/wiki/` | The source of this wiki. |

## Build and run

You need Rust (stable) with the platform build tools, and Node.js. See `Compilation.md` in the repository for the engine. For the app, install the frontend dependencies in `xpando-app` and use the Tauri development command.

Dependency rules: use the project's configured registry, install with scripts disabled (`--ignore-scripts`), pin exact versions, commit the lockfile and use `npm ci` in CI.

## Guidelines

* **Normal mode is simple, Advanced mode adds more.** Each control is always visible, behind **More options**, or Advanced only. Nothing is removed.
* **One word for one thing.** Use the words on the [[Home]] page (phrase, trigger, text, macro, setting, folder). Never "variable", "placeholder" or "parameter" for normal users.
* **Never lose the user's text.** Guard unsaved changes, offer Undo, write atomically with backups.
* Every new feature gets a wiki page or section in the same pull request.

## Editing the wiki

Do not edit the wiki on GitHub: it is overwritten on every publish. Edit the Markdown in `docs/wiki/` and open a pull request. When it is merged to `dev`, a workflow publishes it. Screenshots are listed in `docs/wiki/SCREENSHOTS.md`; put images in `docs/wiki/images/`.

## Pull requests

Branch from `dev`, keep changes focused, run the tests of the part you changed, and describe what and why.
