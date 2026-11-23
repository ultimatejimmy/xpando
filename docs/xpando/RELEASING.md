# Releasing xpando

How xpando is versioned and released. Nothing here is automated yet; the release workflow is
planned in M5 of [PLAN.md](PLAN.md). This page is the agreed scheme, so the automation can be
built to match it.

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

## Release process (to be automated)

1. Decide the version from today's date (see above) and write it to the three manifests using the
   derived semver. A script does this; never edit by hand.
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
7. The in-app updater channel follows the same rule: stable users only see stable and patch
   releases; an opt-in "beta" setting also offers pre-releases.

Tag patterns the workflow should use (case-sensitive):

- Stable or patch: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}(\.[0-9]+)?$`
- Beta: `^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}-beta[0-9]*$`

Anything else is not a release tag and must not publish.
