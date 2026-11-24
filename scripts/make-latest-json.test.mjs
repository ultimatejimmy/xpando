import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildManifest, mergeManifest, platformsFor, readStaged } from "./make-latest-json.mjs";

const script = join(dirname(fileURLToPath(import.meta.url)), "make-latest-json.mjs");
const SIG = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQpSVVE9\n";
const DATE = "2026-11-23T12:00:00Z";

const files = (tag) => [
  { name: `xpando_${tag}_x64-setup.exe`, signature: SIG },
  { name: `xpando_${tag}_universal.app.tar.gz`, signature: SIG },
  { name: `xpando_${tag}_amd64.AppImage`, signature: SIG },
  { name: `xpando_${tag}_universal.dmg`, signature: null }, // manual download only
  { name: `xpando_${tag}_amd64.deb`, signature: null },
];

test("platform keys follow the updater's {os}-{arch} naming", () => {
  assert.deepEqual(platformsFor("xpando_1_x64-setup.exe"), ["windows-x86_64"]);
  assert.deepEqual(platformsFor("xpando_1_arm64-setup.exe"), ["windows-aarch64"]);
  assert.deepEqual(platformsFor("xpando_1_universal.app.tar.gz"), ["darwin-aarch64", "darwin-x86_64"]);
  assert.deepEqual(platformsFor("xpando_1_amd64.AppImage"), ["linux-x86_64"]);
  assert.deepEqual(platformsFor("xpando_1_aarch64.AppImage"), ["linux-aarch64"]);
  assert.deepEqual(platformsFor("xpando_1_amd64.deb"), ["linux-x86_64-deb"]);
  for (const other of ["SHA256SUMS.txt", "xpando_1_universal.dmg", "latest.json", "x.exe.sig", "xpando_1_x64_en-US.msi"]) {
    assert.equal(platformsFor(other), null, other);
  }
});

test("manifest has the version as semver, notes, date and a url+signature per platform", () => {
  const m = buildManifest({ files: files("2026.11.23").slice(0, 4), tag: "2026.11.23", pubDate: DATE });
  assert.equal(m.version, "2026.1123.0");
  assert.equal(m.pub_date, "2026-11-23T12:00:00.000Z");
  assert.match(m.notes, /2026\.11\.23/);
  assert.deepEqual(Object.keys(m.platforms), ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "windows-x86_64"]);
  assert.deepEqual(m.platforms["windows-x86_64"], {
    signature: SIG.trim(),
    url: "https://github.com/ultimatejimmy/xpando/releases/download/2026.11.23/xpando_2026.11.23_x64-setup.exe",
  });
  assert.equal(m.platforms["darwin-aarch64"].url, m.platforms["darwin-x86_64"].url);
});

test("a signed deb is listed under its own key; an unsigned one is left out", () => {
  const f = files("2026.11.23");
  assert.equal(buildManifest({ files: f, tag: "2026.11.23", pubDate: DATE }).platforms["linux-x86_64-deb"], undefined);
  f[4].signature = SIG;
  assert.ok(buildManifest({ files: f, tag: "2026.11.23", pubDate: DATE }).platforms["linux-x86_64-deb"]);
});

test("a beta tag gives the derived beta semver", () => {
  const rs = (t) => buildManifest({ files: files(t).slice(0, 4), tag: t, pubDate: DATE }).version;
  assert.equal(rs("2026.11.23-beta2"), "2026.1123.0-beta.2");
  assert.equal(rs("2026.11.23.1"), "2026.1123.1");
});

test("a package without a signature is refused, never listed", () => {
  const f = files("2026.11.23").slice(0, 4);
  f[0].signature = null;
  assert.throws(() => buildManifest({ files: f, tag: "2026.11.23", pubDate: DATE }), /no signature/);
  f[0].signature = "  \n";
  assert.throws(() => buildManifest({ files: f, tag: "2026.11.23", pubDate: DATE }), /no signature/);
  f[0].signature = "not base64 !!";
  assert.throws(() => buildManifest({ files: f, tag: "2026.11.23", pubDate: DATE }), /does not look like/);
});

test("bad tags, repositories, dates and empty sets are refused", () => {
  const f = files("2026.11.23").slice(0, 4);
  assert.throws(() => buildManifest({ files: f, tag: "v2026.11.23", pubDate: DATE }), /invalid version/);
  assert.throws(() => buildManifest({ files: f, tag: "2026.11.23", repo: "a/b/c", pubDate: DATE }), /invalid repository/);
  assert.throws(() => buildManifest({ files: f, tag: "2026.11.23", pubDate: "soon" }), /pub_date/);
  assert.throws(() => buildManifest({ files: [{ name: "a.dmg", signature: null }], tag: "2026.11.23", pubDate: DATE }), /no signed/);
  const dup = [...f, { name: "other_x64-setup.exe", signature: SIG }];
  assert.throws(() => buildManifest({ files: dup, tag: "2026.11.23", pubDate: DATE }), /two update packages/);
});

test("file names are url-encoded", () => {
  const m = buildManifest({ files: [{ name: "xpando 1_x64-setup.exe", signature: SIG }], tag: "2026.11.23", pubDate: DATE });
  assert.ok(m.platforms["windows-x86_64"].url.endsWith("/xpando%201_x64-setup.exe"));
});

test("the command line reads a folder and writes latest.json", () => {
  const dir = mkdtempSync(join(tmpdir(), "xpando-latest-"));
  for (const f of files("2026.11.23").slice(0, 4)) {
    writeFileSync(join(dir, f.name), "binary");
    if (f.signature) writeFileSync(join(dir, `${f.name}.sig`), f.signature);
  }
  assert.equal(readStaged(dir).length, 4);
  const out = join(dir, "latest.json");
  const run = (args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
  const ok = run(["--dir", dir, "--tag", "2026.11.23", "--out", out, "--pub-date", DATE]);
  assert.equal(ok.status, 0, ok.stderr);
  assert.equal(JSON.parse(readFileSync(out, "utf8")).version, "2026.1123.0");
  const bad = run(["--dir", dir, "--tag", "nope"]);
  assert.equal(bad.status, 1);
  assert.match(bad.stderr, /invalid version/);
});

// ---- adding a platform to a published manifest (release-macos.yml) ----

const TAG = "2026.11.24-beta2";
const published = () =>
  buildManifest({ files: files(TAG).filter((f) => /setup\.exe|AppImage/.test(f.name)), tag: TAG, pubDate: DATE });
const mac = () =>
  buildManifest({ files: files(TAG).filter((f) => f.name.endsWith(".app.tar.gz")), tag: TAG, pubDate: DATE });

test("merging adds the macOS entries and keeps Windows and Linux exactly as published", () => {
  const before = published();
  const merged = mergeManifest(before, mac(), { tag: TAG });
  assert.deepEqual(Object.keys(merged.platforms), ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "windows-x86_64"]);
  assert.deepEqual(merged.platforms["windows-x86_64"], before.platforms["windows-x86_64"]);
  assert.deepEqual(merged.platforms["linux-x86_64"], before.platforms["linux-x86_64"]);
  assert.equal(merged.version, before.version);
  assert.equal(merged.pub_date, before.pub_date);
  assert.equal(merged.notes, before.notes);
});

test("merging never replaces a platform that is already published", () => {
  const withMac = mergeManifest(published(), mac(), { tag: TAG });
  assert.throws(() => mergeManifest(withMac, mac(), { tag: TAG }), /already has an update package for darwin/);
  const win = buildManifest({ files: files(TAG).filter((f) => f.name.includes("setup")), tag: TAG, pubDate: DATE });
  assert.throws(() => mergeManifest(published(), win, { tag: TAG }), /already has an update package for windows/);
});

test("merging refuses a manifest for another version or another release", () => {
  const other = buildManifest({ files: files("2026.11.23-beta").slice(0, 1), tag: "2026.11.23-beta", pubDate: DATE });
  assert.throws(() => mergeManifest(other, mac(), { tag: TAG }), /not 2026/);
  const foreign = published();
  foreign.platforms["windows-x86_64"].url = "https://example.com/evil.exe";
  assert.throws(() => mergeManifest(foreign, mac(), { tag: TAG }), /does not point at this release/);
  const unsigned = published();
  unsigned.platforms["windows-x86_64"].signature = "";
  assert.throws(() => mergeManifest(unsigned, mac(), { tag: TAG }), /valid signature/);
  assert.throws(() => mergeManifest({ nope: 1 }, mac(), { tag: TAG }), /not an update manifest/);
});

test("the command line merges into a published latest.json", () => {
  const dir = mkdtempSync(join(tmpdir(), "mlj-merge-"));
  writeFileSync(join(dir, `xpando_${TAG}_universal.app.tar.gz`), "x");
  writeFileSync(join(dir, `xpando_${TAG}_universal.app.tar.gz.sig`), SIG);
  const pub = join(dir, "published.json");
  writeFileSync(pub, JSON.stringify(published()));
  const out = join(dir, "latest.json");
  const r = spawnSync(process.execPath, [script, "--dir", dir, "--tag", TAG, "--merge", pub, "--out", out], { encoding: "utf8" });
  assert.equal(r.status, 0, r.stderr);
  const m = JSON.parse(readFileSync(out, "utf8"));
  assert.deepEqual(Object.keys(m.platforms), ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "windows-x86_64"]);
  // A second run on the result must fail rather than replace anything.
  const again = spawnSync(process.execPath, [script, "--dir", dir, "--tag", TAG, "--merge", out, "--out", join(dir, "x.json")], { encoding: "utf8" });
  assert.notEqual(again.status, 0);
});
