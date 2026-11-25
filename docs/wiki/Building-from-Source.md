# Building from Source

This page is for developers. If you only want to use xpando, see [Installation](Installation).

## What is in the repository

| Folder | What it is |
|---|---|
| `xpando-core/` | Rust library: reads phrase files, makes careful edits that keep comments and layout, finds conflicts |
| `xpando-app/` | The desktop app: Tauri 2, with a React and TypeScript front end (`src/`) and a Rust back end (`src-tauri/`) |
| `docs/xpando/` | Plan, glossary of terms, design notes and release process |
| everything else | espanso's engine with xpando's additions, kept close to upstream so changes can be merged |

## What you need

* Rust (stable). The repository has a `rust-toolchain.toml`.
* Node.js.
* **On Windows:** the Visual Studio 2022 Build Tools with the C++ workload.
* **On Linux:** the system libraries listed in `.github/workflows/release.yml` (the Tauri libraries, and for the engine `libx11-dev`, `libxtst-dev`, `libxkbcommon-dev`, `libdbus-1-dev` and the wxWidgets development package).

## Run the app

```powershell
cd xpando-app
npm ci --ignore-scripts
npx tauri dev
```

Install from the committed lockfile with `npm ci --ignore-scripts`, which is the project rule: dependencies are pinned to exact versions and install scripts are not run.

A development build uses your normal xpando data folder (see [Settings and Data Folder](Settings-and-Data-Folder)), so back it up first if you plan to experiment.

## Run the checks

```powershell
cd xpando-core
cargo test
cd ..\xpando-app
npm run build
```

`npm run build` type-checks the front end and builds it. The project's CI also runs `cargo fmt --check`.

## Build the engine

The app does not edit phrases through espanso. It starts **its own build of the engine** to watch your typing. The engine is built from the same repository:

```bash
# Windows and macOS: default features
cargo build --release
# Linux X11 (add ",wayland" to the features for a Wayland build)
cargo build --release --no-default-features --features modulo,vendored-tls
```

The result is `target/release/espanso` (`espanso.exe` on Windows). Check it with `target/release/espanso --version`. The output must end in `-xpando`, for example `espanso 2.4.1-xpando`. xpando uses that suffix to tell its build from stock espanso, and it only starts an engine that has it.

The app looks for the engine as `xpando-engine` next to the app program (what the installer provides), or as `espanso` on the `PATH`. To try a full installer-style build locally, run `scripts/build-engine.sh` and then `npx tauri build --config src-tauri/tauri.release.conf.json`.

More about the engine and how it is bundled: [docs/xpando/ENGINE.md](https://github.com/ultimatejimmy/xpando/blob/dev/docs/xpando/ENGINE.md). For the espanso engine's own build notes, see [Compilation.md](https://github.com/ultimatejimmy/xpando/blob/dev/Compilation.md) in the repository, which points to espanso's developer docs.

## Releases

Releases are built by GitHub Actions when a version tag is pushed. The scheme and the process are in [docs/xpando/RELEASING.md](https://github.com/ultimatejimmy/xpando/blob/dev/docs/xpando/RELEASING.md).

---

Related pages: [Contributing](Contributing) | [Installation](Installation) | [Updates](Updates)
