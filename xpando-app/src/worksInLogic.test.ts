// Run: node --test --experimental-strip-types src/worksInLogic.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  allAppsFolders,
  friendlyNames,
  isAppFolder,
  rememberOrigin,
  returnFolder,
  splitFolders,
  worksIn,
  worksInLabel,
  type ScopeApp,
  type ScopeFolder,
} from "./worksInLogic.ts";

const app = (exes: string[], extra: Partial<ScopeApp> = {}): ScopeApp => ({
  exe_names: exes,
  exec_simple: true,
  title_pattern: null,
  class_pattern: null,
  os: null,
  ...extra,
});

const folder = (file: string, kind: ScopeFolder["scope"]["kind"], apps: ScopeApp[] = []): ScopeFolder => ({
  file,
  name: file.replace(/^_|\.ya?ml$/g, ""),
  scope: { kind, apps },
});

test("a plain list of programs is something the setting can show and change", () => {
  const w = worksIn(folder("_word.yml", "only", [app(["winword"])]));
  assert.deepEqual(w, { kind: "apps", exes: ["winword"] });
  assert.equal(worksInLabel(w), "Only Word");
  // Several configs: the union, without repeats.
  const two = worksIn(folder("_x.yml", "only", [app(["winword", "outlook"]), app(["OUTLOOK.exe", "excel"])]));
  assert.deepEqual(two, { kind: "apps", exes: ["winword", "outlook", "excel"] });
  assert.equal(worksInLabel(two), "Only Word, Outlook, Excel");
});

test("anything else stays with the folder and is described, not replaced", () => {
  assert.equal(worksIn(folder("a.yml", "all")).kind, "all");
  assert.equal(worksIn(undefined).kind, "all");
  assert.equal(worksIn(folder("_a.yml", "only", [app(["x"], { title_pattern: "(?i)inbox" })])).kind, "custom");
  assert.equal(worksIn(folder("_a.yml", "only", [app([], { exec_simple: false })])).kind, "custom");
  assert.equal(worksIn(folder("_a.yml", "only", [app(["x"], { os: "windows" })])).kind, "custom");
  assert.equal(worksIn(folder("_a.yml", "only", [app(["x"]), app(["y"], { class_pattern: "z" })])).kind, "custom");
  assert.equal(worksIn(folder("a.yml", "except", [app(["x"])])).kind, "except");
  assert.equal(worksIn(folder("_a.yml", "nowhere")).kind, "nowhere");
  for (const k of ["custom", "except", "nowhere"] as const) assert.ok(worksInLabel({ kind: k }).length > 5);
  assert.equal(worksInLabel({ kind: "all" }), "All apps");
});

test("friendly names: curated, then what the app reported, then the file name", () => {
  const seen = [{ exe: "sdm.exe", name: "Software Deployment Manager" }];
  assert.deepEqual(friendlyNames(["WINWORD.EXE", "ms-teams.exe", "teams.exe", "sdm.exe", "other.exe"], seen), [
    "Word",
    "Teams",
    "Software Deployment Manager",
    "other",
  ]);
  assert.equal(worksInLabel({ kind: "apps", exes: ["a.exe", "b.exe", "c.exe", "d.exe"] }), "Only a, b and 2 more");
});

test("the folder list puts app folders under their own heading", () => {
  const list = [{ file: "base.yml" }, { file: "_word.yml" }, { file: "notes.yml" }, { file: "a.yml" }];
  const infos = {
    "base.yml": folder("base.yml", "all"),
    "_word.yml": folder("_word.yml", "only", [app(["winword"])]),
    "notes.yml": folder("notes.yml", "nowhere"),
    "a.yml": folder("a.yml", "except", [app(["x"])]),
  };
  const { general, apps } = splitFolders(list, infos);
  assert.deepEqual(general.map((f) => f.file), ["base.yml", "notes.yml", "a.yml"]);
  assert.deepEqual(apps.map((f) => f.file), ["_word.yml"]);
  assert.ok(isAppFolder(infos["_word.yml"]));
  assert.ok(!isAppFolder(undefined));
  assert.deepEqual(allAppsFolders(list, infos).map((f) => f.file), ["base.yml"]);
});

test("a phrase goes back to where it came from, else base.yml, else the first all-apps folder", () => {
  const cands = [{ file: "notes.yml" }, { file: "base.yml" }, { file: "personal.yml" }];
  let o = {};
  assert.equal(returnFolder(o, "_word.yml", ":sig", cands), "base.yml");
  o = rememberOrigin(o, "_word.yml", ":sig", "personal.yml");
  assert.equal(returnFolder(o, "_word.yml", ":sig", cands), "personal.yml");
  // Another phrase is not affected.
  assert.equal(returnFolder(o, "_word.yml", ":other", cands), "base.yml");
  // The origin folder is gone or no longer works everywhere.
  assert.equal(returnFolder(o, "_word.yml", ":sig", [{ file: "notes.yml" }]), "notes.yml");
  assert.equal(returnFolder(o, "_word.yml", ":sig", []), null);
});
