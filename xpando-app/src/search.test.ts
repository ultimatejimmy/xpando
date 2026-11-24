// Run: node --test --experimental-strip-types src/search.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import { matchesTerms, parseQuery, type SearchDoc } from "./search.ts";

const doc: SearchDoc = {
  triggers: [":addr", ":address"],
  label: "Home address",
  comment: "Used for shipping",
  body: "12 Main Street\nSpringfield",
  folder: "personal",
  file: "work/personal.yml",
};

const hit = (q: string, ops = true) => matchesTerms(doc, parseQuery(q, ops));

test("operators off: whole query is one plain term", () => {
  assert.deepEqual(parseQuery("trigger:addr two", false), [{ field: null, value: "trigger:addr two" }]);
  assert.equal(hit("trigger:addr", false), false);
  assert.equal(hit("main street", false), true);
  assert.deepEqual(parseQuery("   ", false), []);
});

test("bare words are ANDed across all fields", () => {
  assert.equal(hit("shipping springfield"), true);
  assert.equal(hit("shipping nowhere"), false);
  assert.equal(hit(""), true);
});

test("trigger operator", () => {
  assert.equal(hit("trigger:addr"), true);
  assert.equal(hit("trigger:main"), false);
  assert.equal(hit("trigger::address"), true);
});

test("label, comment, body, folder, file operators", () => {
  assert.equal(hit("label:home"), true);
  assert.equal(hit("label:shipping"), false);
  assert.equal(hit("comment:shipping"), true);
  assert.equal(hit("body:springfield"), true);
  assert.equal(hit("body:addr"), false);
  assert.equal(hit("folder:personal"), true);
  assert.equal(hit("file:work/"), true);
  assert.equal(hit("file:other"), false);
});

test("quoted phrases keep spaces", () => {
  assert.deepEqual(parseQuery('label:"home address"', true), [{ field: "label", value: "home address" }]);
  assert.equal(hit('label:"home address"'), true);
  assert.equal(hit('label:"address home"'), false);
  assert.equal(hit('"main street"'), true);
});

test("operators combine with AND and with bare words", () => {
  assert.equal(hit("trigger:addr folder:personal"), true);
  assert.equal(hit("trigger:addr folder:nope"), false);
  assert.equal(hit("trigger:addr springfield"), true);
});

test("operator names and values are case-insensitive", () => {
  assert.equal(hit("TRIGGER:ADDR"), true);
  assert.equal(hit("Label:HOME"), true);
});

test("an operator with no value yet is ignored", () => {
  assert.deepEqual(parseQuery("trigger:", true), []);
  assert.equal(hit("trigger:"), true);
  assert.deepEqual(parseQuery('body:""', true), []);
});

test("unknown prefixes and colon triggers are bare words", () => {
  assert.deepEqual(parseQuery("http://x", true), [{ field: null, value: "http://x" }]);
  assert.deepEqual(parseQuery(":addr", true), [{ field: null, value: ":addr" }]);
  assert.equal(hit(":addr"), true);
});

test("an unclosed quote takes the rest of the query", () => {
  assert.deepEqual(parseQuery('label:"home add', true), [{ field: "label", value: "home add" }]);
});

test("a phrase with no triggers or empty fields does not crash", () => {
  const empty: SearchDoc = { triggers: [], label: "", comment: "", body: "", folder: "", file: "" };
  assert.equal(matchesTerms(empty, parseQuery("trigger:a", true)), false);
  assert.equal(matchesTerms(empty, parseQuery("", true)), true);
});
