#!/usr/bin/env node
// Builds `latest.json`, the update manifest Tauri's updater reads. See docs/xpando/RELEASING.md.
//
//   node scripts/make-latest-json.mjs --dir <folder> --tag <tag> [--repo owner/name]
//        [--out latest.json] [--pub-date <RFC 3339>] [--notes <text>] [--merge <published latest.json>]
//
// With --merge the staged files are added to an already published manifest (for a platform built
// after the release, see release-macos.yml); platforms already in it are never replaced.
//
// <folder> holds the staged release files: every update package `X` next to its signature `X.sig`
// (the file the Tauri build wrote, whose text is the minisign signature). Only packages that have
// a signature are listed; a package without one, or an empty signature, is an error, so an
// unsigned file can never end up in the manifest. `version` is the derived semver of the tag
// because the updater compares semver. No dependencies.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseVersion, toSemver } from "./version.mjs";

const DEFAULT_REPO = "ultimatejimmy/xpando";
const REPO_RE = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
// A signature file is minisign text encoded as base64.
const SIG_RE = /^[A-Za-z0-9+/=\r\n]+$/;

/**
 * Which updater platform(s) an update package serves, or null when the file is not one.
 * Keys are the plugin's `{os}-{arch}`; a universal macOS app serves both Mac CPUs.
 * Windows: the NSIS setup; Linux: the AppImage (and the .deb under its own key); macOS: the
 * `.app.tar.gz`.
 */
export function platformsFor(name) {
  const lower = name.toLowerCase();
  const arm = lower.includes("aarch64") || lower.includes("arm64");
  const intel = lower.includes("amd64") || lower.includes("x86_64") || lower.includes("x64");
  if (lower.endsWith("-setup.exe")) {
    if (arm) return ["windows-aarch64"];
    if (intel) return ["windows-x86_64"];
    return null;
  }
  if (lower.endsWith(".app.tar.gz")) {
    if (lower.includes("universal")) return ["darwin-aarch64", "darwin-x86_64"];
    if (arm) return ["darwin-aarch64"];
    if (intel) return ["darwin-x86_64"];
    return null;
  }
  if (lower.endsWith(".appimage")) {
    if (arm) return ["linux-aarch64"];
    if (intel) return ["linux-x86_64"];
    return null;
  }
  if (lower.endsWith(".deb")) {
    if (arm) return ["linux-aarch64-deb"];
    if (intel) return ["linux-x86_64-deb"];
    return null;
  }
  return null;
}

function assetUrl(repo, tag, name) {
  return `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`;
}

/**
 * `files` are the staged file names, each with the text of `<name>.sig` (or null).
 * Returns the manifest object.
 */
export function buildManifest({ files, tag, repo = DEFAULT_REPO, pubDate = new Date().toISOString(), notes }) {
  if (!REPO_RE.test(repo)) throw new Error(`invalid repository "${repo}"`);
  parseVersion(tag); // throws unless the tag is a valid xpando version
  const version = toSemver(tag);
  if (Number.isNaN(Date.parse(pubDate))) throw new Error(`invalid pub_date "${pubDate}"`);
  const platforms = {};
  for (const { name, signature } of files) {
    const keys = platformsFor(name);
    if (!keys) continue;
    const sig = (signature ?? "").trim();
    // A .deb is only an update package when the build also signed it; otherwise it is just a
    // manual download. Every other package must be signed.
    if (!sig && name.toLowerCase().endsWith(".deb")) continue;
    if (!sig) throw new Error(`${name} has no signature (${name}.sig is missing or empty); refusing to publish it`);
    if (!SIG_RE.test(sig)) throw new Error(`${name}.sig does not look like a Tauri signature`);
    for (const key of keys) {
      if (platforms[key]) throw new Error(`two update packages for ${key}: ${platforms[key].file} and ${name}`);
      platforms[key] = { file: name, url: assetUrl(repo, tag, name), signature: sig };
    }
  }
  if (Object.keys(platforms).length === 0) throw new Error("no signed update packages were found");
  const clean = {};
  for (const [key, { url, signature }] of Object.entries(platforms).sort(([a], [b]) => a.localeCompare(b))) {
    clean[key] = { signature, url };
  }
  return {
    version,
    notes: notes ?? `xpando ${tag}. Release notes: https://github.com/${repo}/releases/tag/${tag}`,
    pub_date: new Date(pubDate).toISOString(),
    platforms: clean,
  };
}

/**
 * Adds the platforms of `added` (a manifest built from newly staged files) to `existing` (the
 * manifest already published with the release). Used when a platform is built after the release,
 * so a published manifest is only ever extended: an existing platform is never replaced, the
 * version and the release must match, and every entry of both must still look like what the
 * build produces, otherwise nothing is written.
 */
export function mergeManifest(existing, added, { repo = DEFAULT_REPO, tag }) {
  if (!REPO_RE.test(repo)) throw new Error(`invalid repository "${repo}"`);
  parseVersion(tag);
  const prefix = `https://github.com/${repo}/releases/download/${tag}/`;
  const check = (m, what) => {
    if (!m || typeof m !== "object" || typeof m.version !== "string" || !m.platforms || typeof m.platforms !== "object") {
      throw new Error(`${what} is not an update manifest`);
    }
    if (m.version !== toSemver(tag)) throw new Error(`${what} is for version ${m.version}, not ${toSemver(tag)}`);
    for (const [key, p] of Object.entries(m.platforms)) {
      if (!p || typeof p.url !== "string" || !p.url.startsWith(prefix)) throw new Error(`${what}: ${key} does not point at this release`);
      if (typeof p.signature !== "string" || !SIG_RE.test(p.signature.trim()) || !p.signature.trim()) {
        throw new Error(`${what}: ${key} has no valid signature`);
      }
    }
  };
  check(existing, "the published latest.json");
  check(added, "the new files");
  const platforms = { ...existing.platforms };
  for (const [key, entry] of Object.entries(added.platforms)) {
    if (platforms[key]) throw new Error(`the release already has an update package for ${key}; refusing to replace it`);
    platforms[key] = entry;
  }
  const sorted = {};
  for (const key of Object.keys(platforms).sort((a, b) => a.localeCompare(b))) sorted[key] = platforms[key];
  return { ...existing, platforms: sorted };
}

/** Reads the staged files of `dir` (and the `.sig` next to each). */
export function readStaged(dir) {
  const names = readdirSync(dir);
  const set = new Set(names);
  return names
    .filter((n) => !n.endsWith(".sig"))
    .map((name) => ({
      name,
      signature: set.has(`${name}.sig`) ? readFileSync(join(dir, `${name}.sig`), "utf8") : null,
    }));
}

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    const k = argv[i];
    if (!k.startsWith("--") || i + 1 >= argv.length) throw new Error(`bad arguments near "${k}"`);
    out[k.slice(2)] = argv[i + 1];
  }
  return out;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const a = parseArgs(process.argv.slice(2));
    if (!a.dir || !a.tag) throw new Error("usage: make-latest-json.mjs --dir <folder> --tag <tag> [--repo o/n] [--out file]");
    let manifest = buildManifest({
      files: readStaged(a.dir),
      tag: a.tag,
      repo: a.repo,
      pubDate: a["pub-date"],
      notes: a.notes,
    });
    if (a.merge) {
      manifest = mergeManifest(JSON.parse(readFileSync(a.merge, "utf8")), manifest, { repo: a.repo, tag: a.tag });
    }
    writeFileSync(a.out ?? "latest.json", `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`wrote ${a.out ?? "latest.json"}: ${manifest.version}, ${Object.keys(manifest.platforms).join(", ")}`);
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
