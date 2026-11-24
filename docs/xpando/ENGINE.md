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

Every time the app runs the engine it sets `ESPANSO_CONFIG_DIR` to xpando's data folder
(see `data::data_dir`; M6 in [PLAN.md](PLAN.md)), so the engine never reads or writes the stock
espanso folder. Any future `service`/`daemon` launch must set it the same way.

Limit: this finds the binary xpando would use, not the process that is running. If a stock
daemon is already running and the xpando engine sits elsewhere, restart the daemon from the app
(M5 covers the service/start-up flow).

## Shipping it with the app (wired in CI, first run pending)

xpando always uses its own engine; stock espanso is not supported (the stock fallbacks in
`engine_status` go away with M6). The release workflow builds the engine on each OS runner and
bundles it as a Tauri sidecar:

1. `scripts/build-engine.sh` builds `espanso` (flags below), fails unless `--version` ends in
   `-xpando`, and stages `xpando-app/src-tauri/binaries/xpando-engine-<target-triple>[.exe]`
   (macOS: aarch64, x86_64 and a `lipo` universal copy). The folder is `.gitignore`d.
2. `xpando-app/src-tauri/tauri.release.conf.json` holds `bundle.externalBin:
   ["binaries/xpando-engine"]`. It is **not** in `tauri.conf.json`: `tauri-build` fails when a
   listed sidecar is missing, which would break local `cargo check` and `tauri dev`. Only
   `release.yml` passes it: `tauri build --config src-tauri/tauri.release.conf.json`.
3. Tauri installs the sidecar as `xpando-engine[.exe]` next to the app executable, which is the
   first place `engine_status` looks (unit-tested), so the bundled engine always wins.
4. To test a bundled build locally: run `scripts/build-engine.sh`, then
   `npx tauri build --config src-tauri/tauri.release.conf.json`.

Still open: starting the engine from the app (`service`/`daemon`, M5/M6) and making sure the
installer does not register a second competing service; macOS signing/notarization of the
engine; Linux: the engine links wxWidgets dynamically, so the deb needs a `libwxgtk3.*`
dependency and the AppImage must bundle it, and the Wayland variant needs the `wayland` feature.

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

## CI

* `xpando.yml` job `engine`: Ubuntu, `cargo check -p espanso --no-default-features --features
  vendored-tls` (no wxWidgets), a grep that `-xpando` is still in `espanso/src/main.rs`, and
  `cargo test -p espanso-render`. Also runs when engine crates change.
* `release.yml` job `build`: after installing the Linux libraries (`libx11-dev libxtst-dev
  libxkbcommon-dev libdbus-1-dev libwxgtk3.*-dev`; Windows and macOS need nothing extra),
  runs `scripts/build-engine.sh` before `tauri build`.

Not verified: no engine build was run for this change (Windows needs MSVC and compiles
wxWidgets; heavy), so the release job's engine step, the `lipo` step, the Ubuntu 22.04 wxWidgets
package name and the sidecar bundling must be confirmed by the first CI run (try a `-beta` tag).
Locally verified: `cargo check --locked` and `npm run build` still pass without a sidecar, and
the detection unit test passes.

## Converting the PowerShell hack

In Advanced mode a phrase whose macro is exactly `Set-Clipboard -Value $env:ESPANSO_<NAME>`
(also `$env:ESPANSO_<NAME> | Set-Clipboard`, `-v`, `scb`, or inside a `powershell -Command "..."`
wrapper) shows an offer with a Preview. Only after **Convert** is clicked is that one macro
rewritten, to `type: set_clipboard` with `text: "{{name}}"`. Anything that does not match
exactly (extra commands, other settings such as `ignore_error`, a name that is not an earlier
macro) is never offered, and nothing is converted automatically. The file hash is checked on
write, so a file edited meanwhile is refused. The result only works with the xpando engine.
