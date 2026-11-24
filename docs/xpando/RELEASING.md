# Releasing xpando

How xpando is versioned and released. The scheme below is implemented by `scripts/version.mjs` and
`.github/workflows/release.yml`; the remaining M5 items (signing and notarization) are in
[PLAN.md](PLAN.md). The in-app updater is described in the last section.

## Version scheme (date-based)

Versions are the date of the release.

| Release | Version / git tag | GitHub release |
|---|---|---|
| Stable, first release on a day | `2026.11.23` | normal release (marked "Latest") |
| Patch to that day's release | `2026.11.23.1`, `2026.11.23.2`, ... | normal release |
| Beta | `2026.11.23-beta`, `2026.11.23-beta2`, `2026.11.23-beta3`, ... | **pre-release** |

Rules:

- The git tag is exactly the version, with no `v` prefix: tag `2026.11.23`, not `v2026.11.23`.
- A patch version always has a numeric fourth part: `.1`, `.2`, never `.0`. The first release of a
  day has no fourth part.
- Betas lead up to a stable release: `2026.11.23-beta` is the first beta for the `2026.11.23`
  release, `-beta2` the second, and so on. The first beta has no number.
- **Every beta is published as a GitHub pre-release** (never "Latest"). Stable and patch releases
  are normal releases.
- The first stable release after a series of betas has the date it ships, which may differ from the
  date in the betas' names (a `2026.11.23-beta3` can be followed by stable `2026.11.25`).

Ordering, oldest to newest, for one day: `2026.11.23-beta` < `2026.11.23-beta2` < `2026.11.23` <
`2026.11.23.1` < `2026.11.23.2`. The next day's versions sort after all of these.

Month and day are written **without zero padding** (`2026.1.5`, not `2026.01.05`); this is the
default until changed, since the examples (`2026.11.23`) don't show a single-digit month or day.

## Matching the version to the build tools (decided; internal only)

Cargo, Tauri, npm and the Tauri updater require
[semantic versioning](https://semver.org): exactly `MAJOR.MINOR.PATCH`, optional `-prerelease`, no
leading zeros. The scheme above is not valid semver in two places:

1. Four-part patch versions such as `2026.11.23.1`.
2. Windows MSI installers limit the first two parts to 255 (`2026` and `1123` are too large), so
   **use the NSIS installer for Windows, not MSI**. (NSIS has no such limit.)

Decision: keep the human version above for tags, release names, the README and the
"About" screen, and **derive** a semver for `Cargo.toml`, `tauri.conf.json` and `package.json`.
The updater compares the semver, so the derived value must sort the same way.

`YYYY.M.D[.P][-betaN]`  ->  `YYYY.<M*100+D>.<P>[-beta.<N>]`

| Human version / tag | Derived semver |
|---|---|
| `2026.11.23` | `2026.1123.0` |
| `2026.11.23.1` | `2026.1123.1` |
| `2026.11.23.2` | `2026.1123.2` |
| `2026.11.23-beta` | `2026.1123.0-beta.1` |
| `2026.11.23-beta2` | `2026.1123.0-beta.2` |
| `2026.1.5` | `2026.105.0` |

This orders correctly (a pre-release sorts before its release in semver, numeric pre-release parts
compare numerically) and every value is valid semver. The mapping is a pure function: one small
script (`scripts/version.*`) converts in both directions and the release workflow checks that the
tag, the three manifests and the changelog agree before building. Because it is reversible,
the UI always shows the human version.

Users never see the derived value; it only exists so the build tools and the updater work.

## Version script

`node scripts/version.mjs <version>` validates the human version (no `v`, no zero padding, a real
date, patch >= 1, first beta unnumbered and later betas `-beta2`+; `2026.11.23-beta1` and
`2026.11.23.0` are rejected) and writes the derived semver into `xpando-app/package.json`,
`xpando-app/src-tauri/tauri.conf.json`, `xpando-app/src-tauri/Cargo.toml` and
`xpando-core/Cargo.toml`. It also updates only the `xpando-app` / `xpando-core` entries in the two
`Cargo.lock` files, so `cargo --locked` keeps working. `xpando-app/package-lock.json` is left alone
(`npm ci` does not check the root version). Other modes: `--check [version]` (manifests match the
version, or agree with each other), `--info <version>` (prints `version`, `semver`, `prerelease`
for CI), `--from-semver <semver>` (reverse mapping). Tests: `node --test "scripts/*.test.mjs"`.

## Release process

1. Decide the version from today's date (see above) and write it to the manifests with
   `node scripts/version.mjs <version>` (never edit by hand) and commit the result.
2. Update the changelog / release notes for the version.
3. Merge to `dev`, wait for CI to be green on Windows, macOS and Linux.
4. Create the tag (the human version) on that commit and push it. The tag triggers the release
   workflow.
5. The workflow builds installers on each OS (Windows NSIS `.exe`, macOS `.dmg`, Linux
   `.AppImage`/`.deb`), signs where certificates exist, and creates the GitHub release:
   - tag matches a beta pattern (`-beta` or `-betaN`): create it with **`--prerelease`** and do
     not mark it as latest;
   - otherwise: a normal release, marked latest.
6. Smoke-test the artifacts from the release page before announcing.
7. The in-app updater follows the same rule (see "In-app updater" below). It only works if the release has `SHA256SUMS.txt`, which the workflow adds.

What `release.yml` does: a `validate` job rejects tags that are not in the scheme and runs the script
tests; a `build` job per OS (Windows NSIS, macOS universal DMG, Linux AppImage + deb on
ubuntu-22.04) runs `version.mjs <tag>` and `--check`, `npm ci --ignore-scripts`, builds the patched engine (`scripts/build-engine.sh`, see ENGINE.md), then `tauri build` with the release-only sidecar config
with no secrets in the environment, and renames installers to the human version; a `publish` job
(the only one with `contents: write`) first writes `SHA256SUMS.txt` (the `sha256sum` of every installer; see the updater section) and then runs `gh release create --verify-tag --generate-notes` with the installers and that file, with
`--prerelease --latest=false` for betas and `--latest` otherwise. All actions are pinned to full
commit SHAs. Not done yet: code signing / notarization, and a changelog agreement check.

## In-app updater

Implemented without `tauri-plugin-updater` (no new dependency). The logic is in
`xpando-core/src/update.rs` (pure and tested); `xpando-app/src-tauri/src/update.rs` does the
network and the launching; `xpando-app/src/Updates.tsx` is the banner and the Settings section.
The user-facing description is the wiki page [Updates](../wiki/Updates.md).

**Where it looks.** `GET https://api.github.com/repos/ultimatejimmy/xpando/releases` (the list,
newest first, 30 entries). Only the system's `curl` (Windows 10 and later has `curl.exe`; a
PowerShell request is the fallback on older Windows) makes requests, with a 15 s connect timeout,
a total time limit, a size limit, TLS 1.2+, https only and the User-Agent `xpando-updater`. Nothing
else is sent: no version, no machine or user details.

**Which release.** The newest one that is not a draft, has a valid version tag, is newer than the
running version, is not the skipped version, and has both an installer for this OS/CPU and
`SHA256SUMS.txt`. Stable users never get a beta; a release marked pre-release on GitHub counts as a
beta even if its tag has no `-beta`. Beta users (Settings, "Get beta versions") get betas and
any newer stable release. Nothing older than the running version is ever offered, so switching
from beta back to stable keeps the beta until a newer stable release exists (the Settings section
says so). The running version is the human version mapped back from the derived semver
(`Version::from_semver`); a development build (`0.1.0`) has none and never checks.

**Installer files.** Windows `*-setup.exe` (NSIS), macOS `.dmg` (universal), Linux `.AppImage`
(amd64; the `.deb` is for manual installs). The release workflow names them
`xpando_<human version>_<arch>...`.

**`SHA256SUMS.txt`.** Written by the `publish` job from the files it uploads (`sha256sum -- *`,
verified with `--check`) and attached to the release. Format: `<64 hex digits>  <file name>`.

**Install.** The window only says which version to install; the backend looks the release up
again. Downloads only from `https://github.com/ultimatejimmy/xpando/releases/download/<tag>/<file>`
(strict check of host, path, no query or odd characters). Redirects are followed one hop at a time
and each target must be https on github.com or GitHub's own download hosts
(`objects.githubusercontent.com`, `release-assets.githubusercontent.com`,
`github-releases.githubusercontent.com`, `api.github.com`), at most four. The files go to a new
folder under the system temp folder. The installer's SHA-256 must equal its entry in
`SHA256SUMS.txt`; a missing, malformed or conflicting entry or a mismatch deletes the download and
stops. On Windows the installer is then started (after a second hash check) and xpando exits; on
macOS and Linux the file is shown in the file manager and the user installs it. Full automatic
install there is future work.

**Settings** (`settings.yml`, next to `data_dir`): `update_channel: stable|beta` (default
`stable`; an unknown value counts as stable), `auto_check: true`, `last_update_check` (seconds since
1970), `skipped_version`. The daily check runs at start when more than 24 hours have passed and the
open window asks once a day whether it is due again. It fails silently; the button reports
errors.

**Not covered.** Downloads are checksummed against a file from the same release, not signed, so
the check protects against a corrupted or swapped download, not against a compromised release. A
signature check needs the code-signing decisions in M5 (Authenticode / notarization) and is future
work.

Tag patterns the workflow uses (case-sensitive):

- Stable or patch: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}(\.[0-9]+)?$`
- Beta: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}-beta[0-9]*$`

Anything else is not a release tag and must not publish.

## Signing (plan)

Today updates are checked against `SHA256SUMS.txt` from the same release. That catches a corrupted
download but not a tampered release, so signing comes in layers:

1. **Windows Authenticode.** Apply to the SignPath Foundation (free for open source; espanso uses it) once there is a public release; see [SIGNPATH.md](SIGNPATH.md).
   The release workflow submits the installer for signing. The app then checks, before running an
   update, that `Get-AuthenticodeSignature` reports a valid signature from xpando's publisher
   (built-in PowerShell, no new package).
2. **macOS signing and notarization.** Not planned: it needs a paid Apple Developer account. macOS builds stay unsigned and the Install page explains how to open them.
3. **Signed update manifest.** When the registries are reachable again, move to Tauri's updater
   (minisign signatures, public key built into the app, private key in a GitHub secret). Hand-writing
   signature code is not worth the risk.
4. **Build provenance.** GitHub artifact attestations (pinned action) tie each installer to the
   workflow run that built it.

## Engine file lock

The engine runs as its own process (`xpando-engine.exe`). Before an update the app stops it, and
the NSIS installer does the same again through `installer-hooks.nsh` (enabled only in
`tauri.release.conf.json`), so the installer never meets a locked file.
