// Runs scripts/add-macos-to-release.sh against a fake `gh`, so the real upload steps can be
// checked without touching a release. Skipped where bash or sha256sum are missing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, chmodSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, delimiter } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildManifest } from "./make-latest-json.mjs";

const script = join(dirname(fileURLToPath(import.meta.url)), "add-macos-to-release.sh");
const TAG = "2026.11.24-beta2";
const REPO = "ultimatejimmy/xpando";
const SIG = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQpSVVE9\n";

const has = (cmd, args) => spawnSync(cmd, args, { encoding: "utf8" }).status === 0;
const skip = !has("bash", ["--version"]) || !has("bash", ["-c", "command -v sha256sum"])
  ? "bash or sha256sum is not available"
  : false;

const FAKE_GH = `#!/usr/bin/env bash
echo "$@" >> "$FAKE_LOG"
case "$1 $2" in
  "release download")
    dir=""
    while [ $# -gt 0 ]; do if [ "$1" = "--dir" ]; then dir="$2"; fi; shift; done
    cp "$FAKE_RELEASE_DIR"/* "$dir"/ ;;
  "release view") printf '%s' "$FAKE_BODY" ;;
  "release edit") cat > "$FAKE_LOG.notes" ;;
  *) ;;
esac
`;

/** A published release (Windows + Linux) and a finished macOS build, in a fresh folder. */
function setup({ published = true, withMacInRelease = false, macComplete = true, manifest } = {}) {
  const root = mkdtempSync(join(tmpdir(), "addmac-"));
  const rel = join(root, "release");
  const mac = join(root, "macos-files");
  const bin = join(root, "bin");
  mkdirSync(rel);
  mkdirSync(mac);
  mkdirSync(bin);
  writeFileSync(join(bin, "gh"), FAKE_GH);
  chmodSync(join(bin, "gh"), 0o755);
  const files = [`xpando_${TAG}_x64-setup.exe`, `xpando_${TAG}_amd64.AppImage`, `xpando_${TAG}_amd64.deb`];
  for (const f of files) writeFileSync(join(rel, f), `bytes of ${f}`);
  for (const f of files.slice(0, 2)) writeFileSync(join(rel, `${f}.sig`), SIG);
  if (withMacInRelease) writeFileSync(join(rel, `xpando_${TAG}_universal.dmg`), "dmg");
  const m =
    manifest ??
    buildManifest({
      files: [
        { name: files[0], signature: SIG },
        { name: files[1], signature: SIG },
      ],
      tag: TAG,
      pubDate: "2026-11-24T16:00:00Z",
      notes: "published notes",
    });
  if (published) writeFileSync(join(rel, "latest.json"), `${JSON.stringify(m, null, 2)}\n`);
  writeFileSync(join(rel, "SHA256SUMS.txt"), "old sums\n");
  writeFileSync(join(mac, `xpando_${TAG}_universal.dmg`), "mac dmg bytes");
  writeFileSync(join(mac, `xpando_${TAG}_universal.app.tar.gz`), "mac update bytes");
  if (macComplete) writeFileSync(join(mac, `xpando_${TAG}_universal.app.tar.gz.sig`), SIG);
  return { root, rel, mac, bin, log: join(root, "gh.log"), work: join(root, "work") };
}

function run(s, extra = {}) {
  return spawnSync("bash", [script], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${s.bin}${delimiter}${process.env.PATH}`,
      TAG,
      REPO,
      MAC_DIR: s.mac,
      WORK: s.work,
      FAKE_LOG: s.log,
      FAKE_RELEASE_DIR: s.rel,
      FAKE_BODY: "xpando 2026.11.24-beta2. Not included in this release (its build did not run): macOS.",
      ...extra,
    },
  });
}

const calls = (s) => (existsSync(s.log) ? readFileSync(s.log, "utf8").trim().split("\n") : []);
const uploads = (s) => calls(s).filter((c) => c.startsWith("release upload"));

test("adds macOS: macOS files first (no overwrite), then latest.json and checksums with overwrite", { skip }, () => {
  const s = setup();
  const r = run(s);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  const up = uploads(s);
  assert.equal(up.length, 2, calls(s).join("\n"));
  assert.match(up[0], /_universal\.dmg/);
  assert.match(up[0], /_universal\.app\.tar\.gz/);
  assert.doesNotMatch(up[0], /--clobber/);
  assert.doesNotMatch(up[0], /latest\.json/);
  assert.match(up[1], /latest\.json/);
  assert.match(up[1], /SHA256SUMS\.txt/);
  assert.match(up[1], /--clobber/);
  // What would be published: Windows and Linux untouched, macOS added.
  const before = JSON.parse(readFileSync(join(s.rel, "latest.json"), "utf8"));
  const after = JSON.parse(readFileSync(join(s.work, "out", "latest.json"), "utf8"));
  assert.deepEqual(after.platforms["windows-x86_64"], before.platforms["windows-x86_64"]);
  assert.deepEqual(after.platforms["linux-x86_64"], before.platforms["linux-x86_64"]);
  assert.ok(after.platforms["darwin-aarch64"] && after.platforms["darwin-x86_64"]);
  // The checksum list covers every file, old and new, and not itself.
  const sums = readFileSync(join(s.work, "out", "SHA256SUMS.txt"), "utf8");
  for (const f of readdirSync(s.mac)) assert.ok(sums.includes(f), f);
  assert.ok(sums.includes("x64-setup.exe"));
  assert.ok(!sums.includes("SHA256SUMS"));
  assert.ok(!sums.includes("latest.json"));
});

test("the release notes line about macOS is updated", { skip }, () => {
  const s = setup();
  assert.equal(run(s).status, 0);
  assert.match(readFileSync(`${s.log}.notes`, "utf8"), /macOS was added after the release was published\./);
});

test("a dry run checks everything and uploads nothing", { skip }, () => {
  const s = setup();
  const r = run(s, { DRY_RUN: "true" });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.equal(uploads(s).length, 0);
  assert.ok(!calls(s).some((c) => c.startsWith("release edit")));
  assert.ok(existsSync(join(s.work, "out", "latest.json")));
  assert.match(r.stdout, /dry run/);
});

test("an incomplete macOS build leaves the release alone", { skip }, () => {
  const s = setup({ macComplete: false });
  const r = run(s);
  assert.equal(r.status, 0);
  assert.equal(calls(s).length, 0, "it must not even contact the release");
});

test("a release that already has macOS is left alone", { skip }, () => {
  const s = setup({ withMacInRelease: true });
  const r = run(s);
  assert.equal(r.status, 0);
  assert.equal(uploads(s).length, 0);
});

test("a published manifest that already lists macOS is refused, nothing uploaded", { skip }, () => {
  const s = setup();
  const m = buildManifest({
    files: [{ name: `xpando_${TAG}_universal.app.tar.gz`, signature: SIG }],
    tag: TAG,
    pubDate: "2026-11-24T16:00:00Z",
  });
  writeFileSync(join(s.rel, "latest.json"), JSON.stringify(m));
  const r = run(s);
  assert.notEqual(r.status, 0);
  assert.equal(uploads(s).length, 0);
});

test("a manifest for another version, or without a manifest, is refused", { skip }, () => {
  const other = setup();
  writeFileSync(
    join(other.rel, "latest.json"),
    JSON.stringify(
      buildManifest({ files: [{ name: "xpando_2026.11.23-beta_x64-setup.exe", signature: SIG }], tag: "2026.11.23-beta", pubDate: "2026-11-23T00:00:00Z" }),
    ),
  );
  assert.notEqual(run(other).status, 0);
  assert.equal(uploads(other).length, 0);

  const none = setup({ published: false });
  assert.notEqual(run(none).status, 0);
  assert.equal(uploads(none).length, 0);
});

test("a manifest that points at a different release is refused", { skip }, () => {
  const s = setup();
  const m = JSON.parse(readFileSync(join(s.rel, "latest.json"), "utf8"));
  m.platforms["windows-x86_64"].url = "https://example.com/evil.exe";
  writeFileSync(join(s.rel, "latest.json"), JSON.stringify(m));
  assert.notEqual(run(s).status, 0);
  assert.equal(uploads(s).length, 0);
});
