#!/usr/bin/env bash
# Adds a macOS build to a release that was published without it. Used by release-macos.yml.
#
#   TAG=2026.11.24-beta2 REPO=owner/name MAC_DIR=macos-files WORK=work DRY_RUN=true \
#     bash scripts/add-macos-to-release.sh
#
# MAC_DIR holds the macOS build: *_universal.dmg, *_universal.app.tar.gz and its .sig. The
# published release is only ever extended. Nothing is uploaded unless every check passes, and with
# DRY_RUN=true nothing is uploaded at all (the checks and the new files are still produced in WORK).
# Needs: bash, node, sha256sum and the GitHub CLI (`gh`) with a token that can write releases.
set -euo pipefail

: "${TAG:?TAG is required}"
: "${REPO:?REPO is required}"
MAC_DIR="${MAC_DIR:-macos-files}"
WORK="${WORK:-work}"
DRY_RUN="${DRY_RUN:-false}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

say() { echo "add-macos: $*"; }
warn() { echo "::warning::add-macos: $*"; }
fail() { echo "::error::add-macos: $*" >&2; exit 1; }

# 1. The macOS build must be complete, or the release is left exactly as it is.
for pattern in '*_universal.dmg' '*_universal.app.tar.gz' '*_universal.app.tar.gz.sig'; do
  # shellcheck disable=SC2086
  ls "$MAC_DIR"/$pattern >/dev/null 2>&1 || { warn "no $pattern for $TAG in $MAC_DIR; leaving the release as it is"; exit 0; }
done

rm -rf "$WORK"
mkdir -p "$WORK/published" "$WORK/out" "$WORK/all"

# 2. What is published now.
gh release download "$TAG" --repo "$REPO" --dir "$WORK/published"
ls -l "$WORK/published"
[ -s "$WORK/published/latest.json" ] || fail "$TAG has no latest.json to add to"
if ls "$WORK"/published/*_universal.dmg >/dev/null 2>&1; then
  warn "$TAG already has a macOS build; not adding another"
  exit 0
fi

# 3. The extended manifest: the published one plus the macOS entries (the script refuses to
# replace a platform, and checks the version, the URLs and the signatures).
node "$here/make-latest-json.mjs" --dir "$MAC_DIR" --tag "$TAG" --repo "$REPO" \
  --merge "$WORK/published/latest.json" --out "$WORK/out/latest.json"
cat "$WORK/out/latest.json"

# Every platform that was published must still be there, unchanged, and macOS must be added.
node -e '
const fs = require("fs");
const before = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const after = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
for (const [k, v] of Object.entries(before.platforms)) {
  if (JSON.stringify(after.platforms[k]) !== JSON.stringify(v)) throw new Error("platform " + k + " changed or disappeared");
}
for (const k of ["darwin-aarch64", "darwin-x86_64"]) {
  if (!after.platforms[k]) throw new Error("macOS entry " + k + " was not added");
}
for (const k of ["version", "notes", "pub_date"]) {
  if (before[k] !== after[k]) throw new Error(k + " changed");
}
console.log("manifest check ok:", Object.keys(after.platforms).join(", "));
' "$WORK/published/latest.json" "$WORK/out/latest.json"

# 4. Checksums over every installer file, old and new.
cp "$WORK"/published/* "$WORK/all/"
rm -f "$WORK/all/latest.json" "$WORK/all/SHA256SUMS.txt"
cp "$MAC_DIR"/* "$WORK/all/"
(cd "$WORK/all" && sha256sum -- * > ../out/SHA256SUMS.txt)
(cd "$WORK/all" && sha256sum --check --strict ../out/SHA256SUMS.txt)
if grep -q 'SHA256SUMS' "$WORK/out/SHA256SUMS.txt"; then fail "the checksum list must not list itself"; fi

if [ "$DRY_RUN" = "true" ]; then
  say "dry run: everything checked out; would upload:"
  ls "$MAC_DIR"
  say "and replace latest.json and SHA256SUMS.txt. Nothing was uploaded."
  exit 0
fi

# 5. Upload. The macOS files first (no overwrite), so latest.json never points at a missing file.
gh release upload "$TAG" "$MAC_DIR"/* --repo "$REPO"
gh release upload "$TAG" "$WORK/out/latest.json" "$WORK/out/SHA256SUMS.txt" --repo "$REPO" --clobber

# 6. The release notes said macOS was not included.
body="$(gh release view "$TAG" --repo "$REPO" --json body --jq .body || true)"
new="${body//Not included in this release (its build did not run): macOS./macOS was added after the release was published.}"
if [ -n "$body" ] && [ "$new" != "$body" ]; then
  printf '%s' "$new" | gh release edit "$TAG" --repo "$REPO" --notes-file -
fi
say "done: macOS added to $TAG"
