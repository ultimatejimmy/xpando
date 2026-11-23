# The xpando engine (M4b)

Stock espanso rejects the `set_clipboard` macro type ([SET-CLIPBOARD.md](SET-CLIPBOARD.md)), so
phrases that use it need xpando's patched build of espanso. This page says how the app tells the
two apart, how the engine is meant to ship with the app, and how to build it.

## Telling stock from xpando (implemented)

The patched engine reports its version with an `-xpando` suffix: `espanso --version` prints
`espanso 2.4.1-xpando` (`VERSION` in `espanso/src/main.rs`); stock prints `espanso 2.4.1`.

* `xpando_core::engine::classify_version_output` turns that output into `stock`, `xpando` or `missing`.
* The Tauri command `engine_status` returns `{ kind, path, version, bundled }`. It runs
  `--version` (5 second limit, no console window on Windows) on, in order:
  1. `xpando-engine` next to the app executable (what Tauri's `externalBin` installs) and
     `engine/espanso` next to it,
  2. `espanso` on the `PATH`,
  3. the usual stock install location of the OS.

  The first xpando build wins; otherwise the first stock espanso is reported.
* UI: in Advanced mode the status bar shows `Engine: ...`. In any mode, a phrase that has a Set
  clipboard macro shows a warning when the engine is stock or missing.

Limit: this finds the binary xpando would use, not the process that is running. If a stock
daemon is already running and the xpando engine sits elsewhere, restart the daemon from the app
(M5 covers the service/start-up flow).

## Shipping it with the app (plan, not enabled yet)

Tauri's `bundle.externalBin` copies a binary next to the app executable and the build script
fails when the file is missing, so it is deliberately **not** enabled in `tauri.conf.json` until
CI produces the binaries. When it does:

1. Build the engine per target (below) and copy it to
   `xpando-app/src-tauri/binaries/xpando-engine-<target-triple>[.exe]`
   (for example `xpando-engine-x86_64-pc-windows-msvc.exe`).
2. Add to `tauri.conf.json` under `bundle`: `"externalBin": ["binaries/xpando-engine"]`.
3. `.gitignore` the `binaries/` folder; the release workflow builds it, it is not committed.
4. The app starts `xpando-engine` (not `espanso`) for `service`/`daemon` commands, and the
   installer must not register a second copy as a competing service. On Windows the engine
   needs `espanso.exe`'s companion files (`espansod`-style worker is the same binary).
5. macOS: sign and notarize the engine with the app. Linux: Wayland needs the
   `wayland` feature build.

## Building the engine

From the repository root (Rust toolchain per `rust-toolchain.toml`):

```
# Windows and macOS: default features
cargo build --release
# Linux X11 (add ",wayland" to the features for the Wayland build)
cargo build --release --no-default-features --features modulo,vendored-tls
```

The flags are the ones upstream's `.github/workflows/ci.yml` uses for its builds; the output is
`target/release/espanso[.exe]`. Check it with `target/release/espanso --version`, which must end
in `-xpando`.

## CI job outline (not added to `.github/workflows` yet)

```yaml
engine:
  name: engine (${{ matrix.os }})
  strategy:
    matrix:
      include:
        - { os: windows-latest, triple: x86_64-pc-windows-msvc }
        - { os: macos-latest,   triple: aarch64-apple-darwin }
        - { os: ubuntu-latest,  triple: x86_64-unknown-linux-gnu }
  steps:
    - checkout (pinned by SHA like the other jobs)
    - install the OS libraries listed in upstream's ci.yml
    - cargo build --release --locked   # Linux: add the --no-default-features flags above
    - run: target/release/espanso --version | grep -- '-xpando'
    - copy to xpando-engine-${{ matrix.triple }} and upload as an artifact
# The release workflow downloads these into xpando-app/src-tauri/binaries/ before `tauri build`.
```

Not verified: no engine build was run for this change (heavy, needs per-OS libraries), so the
job must be confirmed by its first CI run. The `-xpando` version suffix compiles in theory
(`concat!` of two constants) but was not built here either.

## Converting the PowerShell hack

In Advanced mode a phrase whose macro is exactly `Set-Clipboard -Value $env:ESPANSO_<NAME>`
(also `$env:ESPANSO_<NAME> | Set-Clipboard`, `-v`, `scb`, or inside a `powershell -Command "..."`
wrapper) shows an offer with a Preview. Only after **Convert** is clicked is that one macro
rewritten, to `type: set_clipboard` with `text: "{{name}}"`. Anything that does not match
exactly (extra commands, other settings such as `ignore_error`, a name that is not an earlier
macro) is never offered, and nothing is converted automatically. The file hash is checked on
write, so a file edited meanwhile is refused. The result only works with the xpando engine.
