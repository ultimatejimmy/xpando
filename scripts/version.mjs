#!/usr/bin/env node
// xpando date-based version tool. See docs/xpando/RELEASING.md.
//
//   node scripts/version.mjs <version>            write the derived semver into the manifests
//   node scripts/version.mjs <version> --check    verify the manifests already match <version>
//   node scripts/version.mjs --check              verify the manifests agree with each other
//   node scripts/version.mjs --info <version>     print version/semver/prerelease as key=value lines
//   node scripts/version.mjs --from-semver <sv>   print the human version for a derived semver
//
// Add `--root <dir>` to operate on another checkout (used by the tests). No dependencies.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// YYYY.M.D, then optionally .P (patch, >= 1) or -beta / -betaN (N >= 2). No zero padding.
const VERSION_RE =
  /^(\d{4})\.([1-9]|1[0-2])\.([1-9]|[12]\d|3[01])(?:\.([1-9]\d*)|-beta([2-9]|[1-9]\d+)?)?$/;
const SEMVER_RE = /^(\d{4})\.([1-9]\d{2,3})\.(0|[1-9]\d*)(?:-beta\.([1-9]\d*))?$/;

export function parseVersion(version) {
  const m = VERSION_RE.exec(version);
  if (!m) {
    throw new Error(
      `invalid version "${version}": expected YYYY.M.D, YYYY.M.D.P (P >= 1) or YYYY.M.D-beta[N] (N >= 2), no zero padding`,
    );
  }
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(Date.UTC(year, month - 1, day));
  if (date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) {
    throw new Error(`invalid version "${version}": ${year}-${month}-${day} is not a real date`);
  }
  const beta = version.includes("-beta");
  return {
    year,
    month,
    day,
    patch: m[4] ? Number(m[4]) : 0,
    beta: beta ? (m[5] ? Number(m[5]) : 1) : null,
  };
}

export function isPrerelease(version) {
  return parseVersion(version).beta !== null;
}

/** YYYY.M.D[.P][-betaN] -> YYYY.<M*100+D>.<P>[-beta.<N>] */
export function toSemver(version) {
  const v = parseVersion(version);
  const base = `${v.year}.${v.month * 100 + v.day}.${v.patch}`;
  return v.beta === null ? base : `${base}-beta.${v.beta}`;
}

/** Inverse of toSemver. */
export function fromSemver(semver) {
  const m = SEMVER_RE.exec(semver);
  if (!m) throw new Error(`"${semver}" is not a derived xpando semver`);
  const md = Number(m[2]);
  const [month, day] = [Math.floor(md / 100), md % 100];
  const patch = Number(m[3]);
  const beta = m[4] ? Number(m[4]) : null;
  if (beta !== null && patch !== 0) throw new Error(`"${semver}" has both a patch and a beta`);
  let human = `${m[1]}.${month}.${day}`;
  if (beta === null) {
    if (patch > 0) human += `.${patch}`;
  } else {
    human += beta === 1 ? "-beta" : `-beta${beta}`;
  }
  parseVersion(human); // validates the date
  return human;
}

const FILES = [
  { path: "xpando-app/package.json", kind: "json" },
  { path: "xpando-app/src-tauri/tauri.conf.json", kind: "json" },
  { path: "xpando-app/src-tauri/Cargo.toml", kind: "toml" },
  { path: "xpando-core/Cargo.toml", kind: "toml" },
];
// Cargo.lock records the version of the workspace's own crates; `cargo --locked` fails if it lags.
const LOCKS = [
  { path: "xpando-app/src-tauri/Cargo.lock", crates: ["xpando-app", "xpando-core"] },
  { path: "xpando-core/Cargo.lock", crates: ["xpando-core"] },
];

// First top-level-looking version line (JSON: 2-space indent; TOML: inside [package]).
const JSON_VERSION = /^( {0,2}"version"\s*:\s*")([^"]*)(")/m;
const TOML_VERSION = /^(version\s*=\s*")([^"]*)(")/m;

function tomlPackageRange(text) {
  const start = text.search(/^\[package\]\s*$/m);
  if (start < 0) throw new Error("no [package] table");
  const rest = text.slice(start + 1);
  const next = rest.search(/^\[/m);
  return [start, next < 0 ? text.length : start + 1 + next];
}

function readVersion(file, text) {
  if (file.kind === "json") {
    const m = JSON_VERSION.exec(text);
    if (!m) throw new Error(`${file.path}: no "version" field`);
    return m[2];
  }
  const [a, b] = tomlPackageRange(text);
  const m = TOML_VERSION.exec(text.slice(a, b));
  if (!m) throw new Error(`${file.path}: no version in [package]`);
  return m[2];
}

function writeVersion(file, text, semver) {
  if (file.kind === "json") return text.replace(JSON_VERSION, `$1${semver}$3`);
  const [a, b] = tomlPackageRange(text);
  return text.slice(0, a) + text.slice(a, b).replace(TOML_VERSION, `$1${semver}$3`) + text.slice(b);
}

// Cargo.lock blocks are `[[package]]\nname = "x"\nversion = "y"`; only the named crates change.
function lockRe(crate) {
  return new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${crate}"\\r?\\nversion = ")([^"]*)(")`);
}

export function collect(root) {
  const found = [];
  for (const f of FILES) {
    found.push({ path: f.path, version: readVersion(f, readFileSync(join(root, f.path), "utf8")) });
  }
  for (const l of LOCKS) {
    const text = readFileSync(join(root, l.path), "utf8");
    for (const c of l.crates) {
      const m = lockRe(c).exec(text);
      if (!m) throw new Error(`${l.path}: no [[package]] entry for ${c}`);
      found.push({ path: `${l.path} (${c})`, version: m[2] });
    }
  }
  return found;
}

export function apply(root, version) {
  const semver = toSemver(version);
  for (const f of FILES) {
    const p = join(root, f.path);
    writeFileSync(p, writeVersion(f, readFileSync(p, "utf8"), semver));
  }
  for (const l of LOCKS) {
    const p = join(root, l.path);
    let text = readFileSync(p, "utf8");
    for (const c of l.crates) {
      if (!lockRe(c).test(text)) throw new Error(`${l.path}: no [[package]] entry for ${c}`);
      text = text.replace(lockRe(c), `$1${semver}$3`);
    }
    writeFileSync(p, text);
  }
  return semver;
}

/** Returns a list of problems (empty when everything agrees). */
export function check(root, version) {
  const found = collect(root);
  const problems = [];
  let expected;
  if (version) {
    expected = toSemver(version);
  } else {
    expected = found[0].version;
    try {
      fromSemver(expected);
    } catch (e) {
      problems.push(`${found[0].path}: ${e.message}`);
    }
  }
  for (const f of found) {
    if (f.version !== expected) problems.push(`${f.path}: found ${f.version}, expected ${expected}`);
  }
  return problems;
}

function main(argv) {
  const args = [...argv];
  let root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const flag = (name) => {
    const i = args.indexOf(name);
    if (i < 0) return false;
    args.splice(i, 1);
    return true;
  };
  const ri = args.indexOf("--root");
  if (ri >= 0) {
    root = resolve(args[ri + 1] ?? "");
    args.splice(ri, 2);
  }
  const doCheck = flag("--check");
  const info = flag("--info");
  const reverse = flag("--from-semver");
  const [arg] = args;
  if (args.length > 1 || (!arg && !doCheck)) {
    console.error(
      "usage: version.mjs <version> [--check] | --check | --info <version> | --from-semver <semver> [--root <dir>]",
    );
    return 2;
  }
  try {
    if (reverse) {
      console.log(fromSemver(arg));
    } else if (info) {
      console.log(`version=${arg}\nsemver=${toSemver(arg)}\nprerelease=${isPrerelease(arg)}`);
    } else if (doCheck) {
      if (arg) parseVersion(arg);
      const problems = check(root, arg);
      if (problems.length) {
        console.error(`version check failed:\n  ${problems.join("\n  ")}`);
        return 1;
      }
      console.log(`ok: manifests agree on ${collect(root)[0].version}`);
    } else {
      const semver = apply(root, arg);
      console.log(`${arg} -> ${semver}`);
    }
    return 0;
  } catch (e) {
    console.error(`error: ${e.message}`);
    return 1;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
