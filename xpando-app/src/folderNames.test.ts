// Run: node --test --experimental-strip-types src/folderNames.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import { allFolderNames, effectiveFolder, folderOfScope, folderScope, isFolderScope, sameFolder } from "./folderNames.ts";

test("a phrase's folder is its own label, else its file's, and an empty label means none", () => {
  assert.equal(effectiveFolder("Work", "Base"), "Work");
  assert.equal(effectiveFolder(null, "Base"), "Base");
  assert.equal(effectiveFolder(undefined, null), null);
  assert.equal(effectiveFolder("", "Base"), null);
});

test("folder names are listed once each, A to Z, ignoring capitals", () => {
  assert.deepEqual(allFolderNames(["work", null, "Base", "Work"], ["Zed", "base"]), ["Base", "work", "Zed"]);
  assert.ok(sameFolder("Base", "base"));
  assert.ok(!sameFolder(null, "base"));
});

test("a folder is chosen in the list by its scope value", () => {
  assert.ok(isFolderScope(folderScope("Work")));
  assert.equal(folderOfScope(folderScope("Work")), "Work");
  assert.ok(!isFolderScope("app:word"));
});
