import { test } from "node:test";
import assert from "node:assert/strict";
import { cpSync, mkdtempSync, readFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { apply, check, collect, fromSemver, isPrerelease, parseVersion, toSemver } from "./version.mjs";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..");
const script = join(repo, "scripts", "version.mjs");
const TRACKED = [
  "xpando-app/package.json",
  "xpando-app/src-tauri/tauri.conf.json",
  "xpando-app/src-tauri/Cargo.toml",
  "xpando-app/src-tauri/Cargo.lock",
  "xpando-core/Cargo.toml",
  "xpando-core/Cargo.lock",
];

function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "xpando-version-"));
  for (const f of TRACKED) {
    mkdirSync(dirname(join(dir, f)), { recursive: true });
    cpSync(join(repo, f), join(dir, f));
  }
  return dir;
}

test("derives semver per RELEASING.md", () => {
  const table = {
    "2026.11.23": "2026.1123.0",
    "2026.11.23.1": "2026.1123.1",
    "2026.11.23.2": "2026.1123.2",
    "2026.11.23-beta": "2026.1123.0-beta.1",
    "2026.11.23-beta2": "2026.1123.0-beta.2",
    "2026.1.5": "2026.105.0",
  };
  for (const [human, semver] of Object.entries(table)) {
    assert.equal(toSemver(human), semver);
    assert.equal(fromSemver(semver), human);
  }
});

test("rejects invalid versions", () => {
  for (const bad of [
    "", "v2026.11.23", "2026.11", "2026.11.23.0", "2026.11.23.01", "2026.01.05", "2026.11.5.1x",
    "2026.13.1", "2026.2.30", "2026.11.23-beta1", "2026.11.23-beta0", "2026.11.23-rc1",
    "2026.11.23.1-beta", "2026.11.23-beta.2", "26.11.23", "2026.11.23 ",
  ]) {
    assert.throws(() => parseVersion(bad), /invalid version/, JSON.stringify(bad));
  }
  assert.doesNotThrow(() => parseVersion("2024.2.29"));
});

test("prerelease flag", () => {
  assert.equal(isPrerelease("2026.11.23-beta"), true);
  assert.equal(isPrerelease("2026.11.23-beta12"), true);
  assert.equal(isPrerelease("2026.11.23"), false);
  assert.equal(isPrerelease("2026.11.23.3"), false);
});

// Semver precedence for our shape: numeric core, then a pre-release sorts before its release.
function cmp(a, b) {
  const [ac, ap] = a.split("-");
  const [bc, bp] = b.split("-");
  const an = ac.split(".").map(Number);
  const bn = bc.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (an[i] !== bn[i]) return an[i] - bn[i];
  if (!ap && !bp) return 0;
  if (!ap) return 1;
  if (!bp) return -1;
  return Number(ap.split(".")[1]) - Number(bp.split(".")[1]);
}

test("derived semver sorts like the human versions", () => {
  const ordered = [
    "2026.11.23-beta", "2026.11.23-beta2", "2026.11.23-beta10", "2026.11.23", "2026.11.23.1",
    "2026.11.23.2", "2026.11.23.10", "2026.11.24-beta", "2026.11.24", "2026.12.1", "2027.1.1",
  ];
  for (let i = 1; i < ordered.length; i++) {
    assert.ok(cmp(toSemver(ordered[i - 1]), toSemver(ordered[i])) < 0, `${ordered[i - 1]} < ${ordered[i]}`);
  }
});

test("fromSemver rejects non-derived values", () => {
  for (const bad of ["0.1.0", "2026.1123.0-beta.0", "2026.1123.1-beta.1", "2026.1302.0", "2026.1123"]) {
    assert.throws(() => fromSemver(bad), undefined, bad);
  }
});

test("apply writes every manifest and lockfile entry, nothing else", () => {
  const dir = fixture();
  const before = Object.fromEntries(TRACKED.map((f) => [f, readFileSync(join(dir, f), "utf8")]));
  assert.equal(apply(dir, "2026.11.23-beta2"), "2026.1123.0-beta.2");
  assert.deepEqual(check(dir, "2026.11.23-beta2"), []);
  for (const f of TRACKED) {
    const after = readFileSync(join(dir, f), "utf8");
    const b = before[f].split("\n");
    const a = after.split("\n");
    assert.equal(a.length, b.length, f);
    const changed = a.filter((l, i) => l !== b[i]).length;
    const expected = f === "xpando-app/src-tauri/Cargo.lock" ? 2 : 1;
    assert.equal(changed, expected, `${f} changed ${changed} lines`);
  }
  assert.match(
    readFileSync(join(dir, "xpando-app/src-tauri/Cargo.toml"), "utf8"),
    /tauri = \{ version = "=2\.12\.7"/,
  );
  assert.ok(collect(dir).every((f) => f.version === "2026.1123.0-beta.2"));
});

test("check detects drift", () => {
  const dir = fixture();
  apply(dir, "2026.11.23");
  assert.deepEqual(check(dir), []);
  apply(dir, "2026.11.24");
  assert.ok(check(dir, "2026.11.23").length >= 6);
  const r = spawnSync(process.execPath, [script, "2026.11.23", "--check", "--root", dir]);
  assert.equal(r.status, 1);
  const r2 = spawnSync(process.execPath, [script, "2026.11.24", "--check", "--root", dir]);
  assert.equal(r2.status, 0);
});

test("check without a version flags non-derived manifests", () => {
  const dir = fixture(); // the repo still carries 0.1.0 until the first release
  assert.ok(check(dir).some((p) => /not a derived/.test(p)));
  apply(dir, "2026.11.23");
  assert.deepEqual(check(dir), []);
});

test("cli: info, reverse and bad input", () => {
  const run = (...a) => spawnSync(process.execPath, [script, ...a], { encoding: "utf8" });
  assert.equal(
    run("--info", "2026.11.23-beta").stdout.replace(/\r/g, ""),
    "version=2026.11.23-beta\nsemver=2026.1123.0-beta.1\nprerelease=true\n",
  );
  assert.match(run("--info", "2026.11.23").stdout, /prerelease=false/);
  assert.equal(run("--from-semver", "2026.1123.1").stdout.trim(), "2026.11.23.1");
  assert.equal(run("v2026.11.23").status, 1);
  assert.equal(run().status, 2);
});
