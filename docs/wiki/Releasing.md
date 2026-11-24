# Releasing

xpando versions are **dates**.

| Release | Version and git tag | On GitHub |
|---|---|---|
| Stable, first of the day | `2026.11.23` | normal release (Latest) |
| Patch to that day's release | `2026.11.23.1`, `2026.11.23.2` | normal release |
| Beta | `2026.11.23-beta`, `2026.11.23-beta2`, `2026.11.23-beta3` | **pre-release** |

* The tag is exactly the version, with no `v`.
* Months and days have no leading zero (`2026.1.5`).
* A patch always has a number from 1 (never `.0`).
* Betas lead up to a stable release. A stable release after betas has the date it ships, which can differ from the beta names.
* Order for one day: `-beta` < `-beta2` < stable < `.1` < `.2`.

Build tools need strict semantic versions, so the files inside the app use a derived number (for example `2026.11.23` becomes `2026.1123.0`). You never see it; the app shows the date version. Windows uses the NSIS installer because MSI cannot hold these numbers.

## Process

1. Pick the version from today's date and write it to the manifests with the version script.
2. Update the release notes.
3. Merge to `dev` and wait for green builds on Windows, macOS and Linux.
4. Tag the commit and push the tag. The tag starts the release build (beta tags create a pre-release).
5. Smoke-test the files on the release page before announcing.

The in-app updater follows the same rule: stable users see stable releases, and an opt-in beta setting also offers pre-releases. Every release carries a signed update list (`latest.json`) and a signature file next to each update package; the updater installs only what is signed with xpando's key. `SHA256SUMS.txt` is an extra for people who download by hand. See [[Updates]].

The full agreed scheme is in the repository: [docs/xpando/RELEASING.md](https://github.com/ultimatejimmy/xpando/blob/dev/docs/xpando/RELEASING.md). Automating the release is still planned.
