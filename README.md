<p align="center">
  <img src="xpando-app/src-tauri/icons/128x128.png" alt="xpando logo" width="96" />
</p>

<h1 align="center">xpando</h1>

<p align="center"><em>A text expander with a real app. Built on <a href="https://github.com/espanso/espanso">espanso</a>.</em></p>

<p align="center">
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-blue" />
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Windows%20(primary)%2C%20macOS%20and%20Linux-blue" />
  <img alt="Status" src="https://img.shields.io/badge/status-early%20development-orange" />
</p>

xpando is a fork of espanso with a modern desktop app for managing your phrases,
in the spirit of tools like PhraseExpress. Your phrases stay in plain YAML files,
so everything espanso can do still works, but you no longer have to hand-edit them.

> **Status:** early development. Windows is the primary target. macOS and Linux
> are kept building in CI but get less testing. Expect rough edges, and keep
> backups of your `match` folder (xpando also writes a one-time `.yml.bak` the
> first time it edits a file).

## What you get

* **Phrase manager.** Browse, search and edit every phrase in a three-pane window.
  Search understands operators, and conflicting triggers are flagged.
* **Rich text.** A full WYSIWYG editor for formatted phrases (bold, lists, links,
  tables), stored as espanso `html:` matches.
* **Macros.** Insert the date, the clipboard, a random pick, a choice list and
  more, with a simple editor. Power options (shell commands, forms) are in
  Advanced mode.
* **Folders and per-app scope.** Each folder is a match file. Limit a folder to
  chosen apps, or leave it for everything.
* **Advanced mode.** One switch reveals the power tools: every match option,
  the macro settings, and an editable YAML view with a safety net.
* **Safe edits.** Saves are atomic, preserve your comments and formatting, and
  refuse if the file changed underneath you.
* **Set clipboard.** A native `set_clipboard` macro for xpando's engine, instead
  of PowerShell workarounds. Stock espanso does not understand it. See
  [docs/xpando/SET-CLIPBOARD.md](docs/xpando/SET-CLIPBOARD.md).
* **Everything espanso does.** Triggers, regex, app-specific configs, packages
  and the espanso hub keep working. See the [espanso docs](https://espanso.org/docs/).

## Install

Installers will be published on the [Releases](https://github.com/ultimatejimmy/xpando/releases)
page once the release pipeline is live. Versions are date-based
(for example `2026.11.23`, with betas like `2026.11.23-beta`, which are marked
pre-release). See [docs/xpando/RELEASING.md](docs/xpando/RELEASING.md).

## Build from source

You need Rust (stable) and Node.js. On Windows, install the Visual Studio 2022
Build Tools with the C++ workload.

```powershell
cd xpando-app
npm ci --ignore-scripts
npx tauri dev
```

The app reads and writes your existing espanso `match` folder
(`%APPDATA%\espanso\match` on Windows). Run the checks with:

```powershell
cd xpando-core; cargo test
cd ..\xpando-app; npm run build
```

The espanso engine itself builds as before, see [Compilation.md](Compilation.md).

## Project layout

| Path | What it is |
| :--- | :--------- |
| `xpando-core/` | Rust library: parsing, format-preserving YAML edits, conflicts, folders |
| `xpando-app/` | The Tauri 2 desktop app (React and TypeScript front end) |
| `docs/xpando/` | Plan, glossary, design notes and release process |
| everything else | The upstream espanso engine, kept close to upstream so changes can be merged |

The roadmap is in [docs/xpando/PLAN.md](docs/xpando/PLAN.md) and the shared
vocabulary (for example, "macro") is in [docs/xpando/TERMS.md](docs/xpando/TERMS.md).

## Contributing

Issues and pull requests are welcome. Please keep user-facing wording consistent
with [TERMS.md](docs/xpando/TERMS.md), and run the checks above before opening a PR.
Dependencies are pinned to exact versions and installed with `--ignore-scripts`.

## Credits and license

xpando is built on [espanso](https://github.com/espanso/espanso), created by
[Federico Terzi](https://github.com/federico-terzi) and maintained by the espanso
team and contributors. Thank you all. The espanso engine, its docs and its
community live at [espanso.org](https://espanso.org).

xpando is free software, licensed under the [GPL-3.0 license](LICENSE), the same
license as espanso. The panda icon is from [Lucide](https://lucide.dev), and the
wordmark font is Fredoka (SIL Open Font License).
