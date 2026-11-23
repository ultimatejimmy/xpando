// Run: node --test --experimental-strip-types src/varorder.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import { placeholders, removePlaceholders, renameInText, sortByDependency, usedBy } from "./varorder.ts";

type V = { name: string; uses: string[] };
const v = (name: string, ...uses: string[]): V => ({ name, uses });
const order = (items: V[]) => sortByDependency(items, (i) => i.uses);
const names = (items: V[]) => order(items).items.map((i) => i.name);

test("independent variables keep their order", () => {
  assert.deepEqual(names([v("c"), v("a"), v("b")]), ["c", "a", "b"]);
  assert.deepEqual(order([]).items, []);
});

test("a variable moves after the ones it uses, nothing else moves", () => {
  assert.deepEqual(names([v("out", "date", "clip"), v("x"), v("date"), v("clip")]), ["x", "date", "clip", "out"]);
  assert.deepEqual(names([v("a", "b"), v("b"), v("c")]), ["b", "a", "c"]);
  assert.deepEqual(names([v("a"), v("b", "c"), v("c"), v("d")]), ["a", "c", "b", "d"]);
});

test("already ordered input is returned unchanged and without problems", () => {
  const r = order([v("date"), v("clip"), v("out", "date", "clip")]);
  assert.deepEqual(r.items.map((i) => i.name), ["date", "clip", "out"]);
  assert.deepEqual(r.problems, []);
});

test("chains are followed", () => {
  assert.deepEqual(names([v("c", "b"), v("b", "a"), v("a")]), ["a", "b", "c"]);
});

test("names that are not variables of the phrase are ignored", () => {
  const r = order([v("a", "clipboard_global"), v("b")]);
  assert.deepEqual(r.items.map((i) => i.name), ["a", "b"]);
  assert.deepEqual(r.problems, []);
});

test("the input array is not changed", () => {
  const input = [v("a", "b"), v("b")];
  order(input);
  assert.deepEqual(input.map((i) => i.name), ["a", "b"]);
});

test("a variable that uses itself is a problem naming it", () => {
  const r = order([v("a", "a"), v("b")]);
  assert.equal(r.problems.length, 1);
  assert.match(r.problems[0], /“a” uses itself/);
  assert.deepEqual(r.items.map((i) => i.name), ["a", "b"]);
});

test("two variables that use each other are a problem naming both", () => {
  const r = order([v("x"), v("a", "b"), v("b", "a")]);
  assert.equal(r.problems.length, 1);
  assert.match(r.problems[0], /“a” and “b” use each other/);
  assert.deepEqual(r.items.map((i) => i.name), ["x", "a", "b"]);
});

test("a longer circle names all its members, and not a variable that only waits for it", () => {
  const r = order([v("a", "c"), v("b", "a"), v("c", "b"), v("d", "a"), v("e")]);
  assert.equal(r.problems.length, 1);
  assert.match(r.problems[0], /“a”, “b” and “c” use each other in a circle/);
  assert.doesNotMatch(r.problems[0], /“d”/);
  assert.equal(r.items.length, 5);
  assert.equal(r.items[0].name, "e");
});

test("usedBy lists the other variables that use a name", () => {
  const items = [v("date"), v("out", "date"), v("self", "self"), v("o2", "date", "out")];
  assert.deepEqual(usedBy(items, "date", (i) => i.uses), ["out", "o2"]);
  assert.deepEqual(usedBy(items, "self", (i) => i.uses), []);
  assert.deepEqual(usedBy(items, "o2", (i) => i.uses), []);
});

test("placeholders and renameInText understand spaces and fields", () => {
  assert.deepEqual(placeholders("a {{x}} {{ y }} {{form1.name}} {x}"), ["x", "y", "form1"]);
  assert.equal(renameInText("{{ x }} {{x.f}} {{xy}}", "x", "z"), "{{ z }} {{z.f}} {{xy}}");
});

const rm = (s: string, name = "d", html = false) => removePlaceholders(s, name, html);

test("removal takes out every occurrence and counts them", () => {
  assert.deepEqual(rm("{{d}}{{d}}x{{d}}"), { text: "x", count: 3 });
  assert.deepEqual(rm("no placeholders"), { text: "no placeholders", count: 0 });
  assert.equal(rm("{{dd}} {{e}} {{d2}}").count, 0);
});

test("removal handles spaces inside braces and .field", () => {
  assert.equal(rm("a {{ d }} b").text, "a b");
  assert.equal(rm("a {{d.name}} b").text, "a b");
  assert.equal(rm("a {{  d.name  }} b").text, "a b");
});

test("removal leaves no double space and no stray space", () => {
  assert.equal(rm("Hi {{d}} there").text, "Hi there");
  assert.equal(rm("Hi {{d}}").text, "Hi");
  assert.equal(rm("{{d}} Hi").text, "Hi");
  assert.equal(rm("Dear {{d}}, hello").text, "Dear, hello");
  assert.equal(rm("line one {{d}}\nline two").text, "line one\nline two");
  assert.equal(rm("one\n{{d}} two").text, "one\ntwo");
  assert.equal(rm("({{d}} x)").text, "(x)");
  assert.equal(rm("a{{d}}b").text, "ab");
  assert.equal(rm("a {{d}}{{e}}").text, "a {{e}}");
  assert.equal(rm("{{d}} {{d}} {{d}}").text, "");
});

test("removal keeps other placeholders and newlines intact", () => {
  assert.equal(rm("{{a}}\n\n{{d}}\n\n{{b}}").text, "{{a}}\n\n\n\n{{b}}");
  assert.equal(rm("x $|$ {{d}} y").text, "x $|$ y");
});

test("removal in html does not damage tags", () => {
  assert.equal(rm("<p>Hi {{d}}</p>", "d", true).text, "<p>Hi</p>");
  assert.equal(rm("<p>{{d}} Hi</p>", "d", true).text, "<p>Hi</p>");
  assert.equal(rm("<p>a <b>{{d}}</b> c</p>", "d", true).text, "<p>a <b></b> c</p>");
  assert.equal(rm('<p>a {{d}} <a href="x">l</a></p>', "d", true).text, '<p>a <a href="x">l</a></p>');
  // A placeholder inside a tag is not touched.
  const inTag = '<a href="{{d}}">x</a> {{d}}';
  assert.deepEqual(rm(inTag, "d", true), { text: '<a href="{{d}}">x</a>', count: 1 });
});

test("a name that is not a plain word removes nothing", () => {
  assert.deepEqual(removePlaceholders("{{a}}", "a.*", false), { text: "{{a}}", count: 0 });
});
