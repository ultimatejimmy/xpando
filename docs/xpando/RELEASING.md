# Releasing xpando

How xpando is versioned and released. The scheme below is implemented by `scripts/version.mjs` and
`.github/workflows/release.yml`; the remaining M5 items (code signing and notarization) are in
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
7. The in-app updater follows the same rule (see "In-app updater" below). It only works if the release has a signed `latest.json`, which the workflow adds. The repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` must exist (see "Signing").

What `release.yml` does: a `validate` job rejects tags that are not in the scheme and runs the script
tests; a `build` job per OS (Windows NSIS, macOS universal DMG, Linux AppImage + deb on
ubuntu-22.04) runs `version.mjs <tag>` and `--check`, `npm ci --ignore-scripts` (no secrets in the
environment), builds the patched engine (`scripts/build-engine.sh`, see ENGINE.md), then `tauri build` with the
release-only config (sidecar and `createUpdaterArtifacts`). **Only that step gets the two signing secrets**;
Tauri signs each update package and writes `<package>.sig`. The installers and `.sig` files are renamed to the
human version. The macOS build uses the bundles `dmg,app`: the `.dmg` is for people, the signed
`.app.tar.gz` is what the updater installs. A `publish` job (the only one with `contents: write`) writes
`SHA256SUMS.txt` (for manual downloads), generates `latest.json` with `scripts/make-latest-json.mjs`
(tested by `scripts/make-latest-json.test.mjs`), and runs `gh release create --verify-tag
--generate-notes` with the installers, `.sig` files, `SHA256SUMS.txt` and `latest.json`, with
`--prerelease --latest=false` for betas and `--latest` otherwise. All actions are pinned to full
commit SHAs. Not done yet: Authenticode / notarization, and a changelog agreement check.

### When a platform does not build

The three builds are independent: one that fails or never gets a runner does not stop the others
(macOS runners on GitHub are scarce, and a macOS job can be cancelled after waiting about 15 minutes
with no runner). The `publish` job runs anyway, as long as the signed Windows installer exists, and
the release notes name any platform that is missing. `latest.json` then has no entry for it, so the
updater offers nothing on that platform until it is added.

### Adding macOS to a published release (`release-macos.yml`)

A separate workflow builds macOS for a release that was published without it and adds it. The
steps that change the release are in `scripts/add-macos-to-release.sh`, which is tested against a
fake `gh` (`scripts/add-macos-to-release.test.mjs`), including the cases that must change nothing.

- **By hand:** Actions, "release-macos", Run workflow, give the tag (for example
  `2026.11.24-beta2`). Or from a terminal (a dry run; add `-f dry_run=false` for the real one):
  `gh workflow run release-macos.yml --repo ultimatejimmy/xpando -f tag=2026.11.24-beta2`.
  `--repo` is needed because the clone has more than one remote. If `gh` answers with a permission
  error, a restricted `GITHUB_TOKEN` is set in the shell: clear it for the command (in PowerShell,
  `$env:GITHUB_TOKEN = $null`) so `gh` uses its normal login.
- **Which Mac runner:** each GitHub macOS pool has its own queue, and the default one
  (`macos-latest`) is often empty-handed. The run form has a "runner" choice: `macos-latest`
  (macOS 26, Apple Silicon), `macos-15` (Apple Silicon), `macos-15-intel` and `macos-26-intel`
  (Intel; the universal app is cross-built, so it is the same result). From a terminal add
  `-f runner=macos-15-intel`. If one pool never starts (GitHub cancels the job after about 15
  minutes without a runner), try another. For the nightly run, set the repository variable
  `RELEASE_MACOS_RUNNER` to the pool that works best. The free labels are listed in the
  [runner-images README](https://github.com/actions/runner-images); `macos-14` was retired in
  November 2026 and the `-large` and `-xlarge` ones are paid.
- **Dry run first:** `dry_run` is on by default for manual runs. It builds macOS and runs every
  check (merged `latest.json`, checksums) but uploads nothing, and the log says what it would do.
  Do one dry run, then run it again with `dry_run` off.
- **Nightly:** the schedule (07:17 UTC, when runners are least busy) is **off until you set the
  repository variable `RELEASE_MACOS_NIGHTLY` to `true`** (Settings, Secrets and variables, Actions,
  Variables). Switch it on after one dry run and one real run have worked. It looks at the last 10
  published releases from the last three weeks and builds macOS for each one without a
  `_universal.dmg`; when all have macOS it does nothing.
- **What it changes:** it builds the tagged source with the same steps and signing as `release.yml`
  (the signing secrets are only in the build step), uploads the `.dmg`, the update package and its
  `.sig`, and then replaces `latest.json` and `SHA256SUMS.txt`. The new `latest.json` is the
  published one plus the macOS entries (`make-latest-json.mjs --merge`): a platform that is already
  listed is never replaced, the version and the release URLs are checked, and the script stops
  without uploading anything if a Windows or Linux entry would change or disappear. The macOS files
  go up first so `latest.json` never points at a file that is not there yet.
- **Safety:** it uses the same concurrency group as `release.yml` for that tag, so it never runs at
  the same time as a release. If macOS was added meanwhile, it does nothing. If the macOS build is
  missing or incomplete, the release is left exactly as it was. If no Mac runner is available the
  job waits and is cancelled by GitHub after a while; the next run tries again.
- **Release notes:** a note saying macOS was not included is changed to say it was added later.
- **If a bad `latest.json` ever gets published:** download `latest.json` from an earlier run's
  artifact or rebuild it with `scripts/make-latest-json.mjs` from the release's files and upload it
  with `gh release upload <tag> latest.json --clobber`.

## In-app updater

Release selection is our own; download, signature check and install are Tauri's official
`tauri-plugin-updater` (2.14.1). The selection logic is in
`xpando-core/src/update.rs` (pure and tested); `xpando-app/src-tauri/src/update.rs` reads the release
list and drives the plugin; `xpando-app/src/Updates.tsx` is the banner and the Settings section.
The user-facing description is the wiki page [Updates](../wiki/Updates.md).

**Where it looks.** `GET https://api.github.com/repos/ultimatejimmy/xpando/releases` (the list,
newest first, 30 entries). Only the system's `curl` (Windows 10 and later has `curl.exe`; a
PowerShell request is the fallback on older Windows) makes requests, with a 15 s connect timeout,
a total time limit, a size limit, TLS 1.2+, https only and the User-Agent `xpando-updater`. Nothing
else is sent: no version, no machine or user details.

**Which release.** The newest one that is not a draft, has a valid version tag, is newer than the
running version, is not the skipped version, and has both an installer for this OS/CPU and the
signed manifest `latest.json`. Stable users never get a beta; a release marked pre-release on GitHub counts as a
beta even if its tag has no `-beta`. Beta users (Settings, "Get beta versions") get betas and
any newer stable release. Nothing older than the running version is ever offered, so switching
from beta back to stable keeps the beta until a newer stable release exists (the Settings section
says so). The running version is the human version mapped back from the derived semver
(`Version::from_semver`); a development build (`0.1.0`) has none and never checks.

**Release files.** Per platform, the *update package* (what the plugin installs) and its minisign
signature `<package>.sig`, both named `xpando_<human version>_<arch>...`:

| Platform | Update package | Also shipped (manual download) |
|---|---|---|
| Windows | `*-setup.exe` (NSIS) | |
| macOS | `*_universal.app.tar.gz` (universal; serves `darwin-aarch64` and `darwin-x86_64`) | `.dmg` |
| Linux | `.AppImage` (amd64) | `.deb` (listed in `latest.json` only if the build also signed it) |

**`latest.json`.** The update manifest the plugin reads, generated by the `publish` job with
`scripts/make-latest-json.mjs` from the staged files: `version` (the derived **semver** of the tag,
because the plugin compares semver), `notes`, `pub_date`, and under `platforms` one `{signature, url}`
per target key (`windows-x86_64`, `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `-deb` variants),
where `signature` is the text of the package's `.sig`. The script fails the release if an update
package has no signature. `SHA256SUMS.txt` (`sha256sum` of every file, `<64 hex>  <name>`) is still
attached, for people who check a manual download; the updater does not read it.

**Install.** The window only says which version to install; the backend looks the release up
again with the same rules. It builds the plugin's updater with exactly one endpoint, that release's
`https://github.com/ultimatejimmy/xpando/releases/download/<tag>/latest.json` (strict check of host,
path, no query or odd characters). The plugin downloads the package for this OS/CPU, **verifies the
minisign signature against the public key in `tauri.conf.json`** (`plugins.updater.pubkey`) and
refuses on any mismatch; xpando also requires that the manifest's version is the version it offered.
Then: Windows stops the engine (`stop_engine`, also done by `installer-hooks.nsh`), starts the NSIS
installer in passive mode (small progress window, no questions) and the plugin exits xpando; macOS
replaces the app bundle and Linux replaces the running AppImage in place, and the user restarts
xpando. A Linux copy that is not an AppImage (for example the `.deb`) cannot replace itself, so
xpando opens the release page in the browser instead (the only fallback left). The release *list*
is still read with `curl` as described above; redirects there are followed one hop at a time and
each target must be https on github.com or GitHub's own download hosts. The updater plugin's own
download uses its built-in HTTP client.

**Settings** (`settings.yml`, next to `data_dir`): `update_channel: stable|beta` (default
`stable`; an unknown value counts as stable), `auto_check: true`, `last_update_check` (seconds since
1970), `skipped_version`. The daily check runs at start when more than 24 hours have passed and the
open window asks once a day whether it is due again. It fails silently; the button reports
errors.

**Not covered.** The signature protects the update package, not `latest.json` itself: an attacker who
could replace a release's files cannot make xpando install anything not signed with our key, but could
replay an older genuinely signed package under a newer version number. Tauri's `requireSignedVersion`
option closes this once the signer records the version in the signature; the current CLI does not
(checked with `tauri signer sign`), so it is off. The key is therefore the whole trust anchor: keep
it safe (see "Signing"). Windows Authenticode and macOS notarization are separate, still open.

Tag patterns the workflow uses (case-sensitive):

- Stable or patch: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}(\.[0-9]+)?$`
- Beta: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}-beta[0-9]*$`

Anything else is not a release tag and must not publish.

## Signing

Updates are checked by signature, not just a checksum, and signing comes in layers:

1. **Windows Authenticode** (open). Apply to the SignPath Foundation (free for open source; espanso
   uses it) once there is a public release; see [SIGNPATH.md](SIGNPATH.md). The release workflow would submit the installer for signing. The app could then also
   check, before running an update, that `Get-AuthenticodeSignature` reports a valid signature from
   xpando's publisher.
2. **macOS signing and notarization** (not planned). It needs a paid Apple Developer account, so macOS builds stay unsigned and the Install page explains how to open them.
3. **Signed updates, Tauri's updater** (**done**, this section). minisign signatures; the public key is
   built into the app (`xpando-app/src-tauri/tauri.conf.json`, `plugins.updater.pubkey`), the private
   key is a GitHub secret. Tauri's updater plugin verifies every download; xpando installs nothing
   that does not verify.
4. **Build provenance** (open). GitHub artifact attestations (pinned action) tie each installer to
   the workflow run that built it.

### What the maintainer must set up once

The key pair was generated with `npx tauri signer generate` outside the repository. Add two
**repository secrets** (GitHub: Settings, Secrets and variables, Actions, New repository secret):

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the full contents of the private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the key's password |

Without them the release build fails (no `.sig` files are produced). Keep the private key file and its
password in a password manager or other backup that is not the repository: **if the private key is
lost, existing installs can never be updated again** (they only trust the old public key) and users
must install the next version by hand. The workflow gives the secrets only to the `tauri build`
step; the install steps and the `publish` job never see them. Forks and pull requests do not get
secrets, so only tag builds on the main repository can produce signed releases.

### Rotating the key

Rotate if the private key may have leaked, or on a schedule you choose.

1. Generate a new pair: `npx tauri signer generate -w <new key file>` (outside the repository) and
   set a password.
2. A key change cannot be shipped to old installs signed with the new key: they only trust the
   old public key. So first release a version **signed with the old key** whose `tauri.conf.json`
   already contains the **new** public key. Installs of that version trust the new key.
3. Then replace both repository secrets with the new key and password, and sign later releases with
   the new key. (Installs older than step 2 that skip that version cannot update across the change and
   need a manual install.)
4. After a leak, assume releases can be forged until step 2 has been installed widely; say so in the
   release notes and rotate the secrets immediately so no further release is signed with the old key.

## Engine file lock

The engine runs as its own process (`xpando-engine.exe`). Before an update the app stops it, and
the NSIS installer does the same again through `installer-hooks.nsh` (enabled only in
`tauri.release.conf.json`), so the installer never meets a locked file.

## Inherited espanso workflows

xpando keeps espanso's workflow files in the repository so upstream changes still merge cleanly, but
the ones xpando does not use are **disabled on GitHub** (Actions, the workflow, "..." menu, Disable;
or `gh workflow disable "<name>" --repo ultimatejimmy/xpando`): "ARM CI", "Create release draft",
"Dev Release" (it would publish espanso-named builds on every push to `dev`), "nix build" and
"Update flake". Turn one back on with `gh workflow enable`. "CI" (the engine build checks on pull
requests) stays on. Those macOS jobs and xpando's own macOS checks wait for the same scarce
runners as the release, so a pull request's macOS checks may be cancelled with no runner assigned;
that is not a code failure.
