# Releasing xpando

How xpando is versioned and released. The scheme below is implemented by `scripts/version.mjs` and
`.github/workflows/release.yml`; the remaining M5 items (signing, updater) are in
[PLAN.md](PLAN.md).

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
7. The in-app updater channel follows the same rule (see the design note below).

What `release.yml` does: a `validate` job rejects tags that are not in the scheme and runs the script
tests; a `build` job per OS (Windows NSIS, macOS universal DMG, Linux AppImage + deb on
ubuntu-22.04) runs `version.mjs <tag>` and `--check`, `npm ci --ignore-scripts`, builds the patched engine (`scripts/build-engine.sh`, see ENGINE.md), then `tauri build` with the release-only sidecar config
with no secrets in the environment, and renames installers to the human version; a `publish` job
(the only one with `contents: write`) runs `gh release create --verify-tag --generate-notes`, with
`--prerelease --latest=false` for betas and `--latest` otherwise. All actions are pinned to full
commit SHAs. Not done yet: code signing / notarization, and a changelog agreement check.

## Updater design note (not implemented)

The Tauri updater (`tauri-plugin-updater`, a new dependency, so deferred to its own PR) reads a
JSON manifest from an endpoint and compares the **derived semver** with the running app, which
already sorts correctly. Proposed channels:

- Two static manifests published as release assets: `latest-stable.json` and `latest-beta.json`.
  The workflow would generate them from the signed bundle `.sig` files after the build matrix.
  Stable releases update both files; betas update only `latest-beta.json`.
- The app picks its endpoint from a setting (default stable; opt-in "Include beta releases").
  Both endpoints would use the GitHub release URL, e.g.
  `https://github.com/ultimatejimmy/xpando/releases/latest/download/latest-stable.json` (GitHub's
  "latest" skips pre-releases, which matches the stable channel for free); the beta manifest would
  live at a fixed tag such as `updater-beta`.
- Updates must be signed (Tauri's own minisign key pair): the private key is a repository secret
  used only in the signing step of the build job, never in install steps; the public key goes in
  `tauri.conf.json`. A beta user on `2026.1123.0-beta.2` is offered stable `2026.1123.0`
  automatically since it sorts higher; stable users are never offered a pre-release.
- Needs the signing key and Windows/macOS code-signing decisions (see M5) before it is enabled.

Tag patterns the workflow should use (case-sensitive):

- Stable or patch: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}(\.[0-9]+)?$`
- Beta: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}-beta[0-9]*$`

Anything else is not a release tag and must not publish.
