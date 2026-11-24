// Run: node --test --experimental-strip-types src/richtextUtil.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  findPlaceholders,
  isBlankHtml,
  listWords,
  normalizeImageUrl,
  normalizeLinkUrl,
} from "./richtextUtil.ts";

const url = (s: string) => {
  const r = normalizeLinkUrl(s);
  return r.ok ? r.url : `ERR:${r.error}`;
};

test("web addresses are kept as typed", () => {
  assert.equal(url("https://example.com/a?b=1#c"), "https://example.com/a?b=1#c");
  assert.equal(url("  http://example.com  "), "http://example.com");
  assert.equal(url("HTTPS://Example.com"), "HTTPS://Example.com");
  assert.equal(url("https://x.com/{{id}}"), "https://x.com/{{id}}");
});

test("obvious shortcuts get a scheme", () => {
  assert.equal(url("example.com"), "https://example.com");
  assert.equal(url("www.example.com/path"), "https://www.example.com/path");
  assert.equal(url("a@b.com"), "mailto:a@b.com");
  assert.equal(url("mailto:a@b.com"), "mailto:a@b.com");
  assert.equal(url("tel:+15551234"), "tel:+15551234");
  assert.equal(url("tel:5551234"), "tel:5551234");
  assert.equal(url("example.com:8080/x"), "https://example.com:8080/x");
  assert.ok(url("localhost:3000/x").startsWith("ERR:"), "needs a scheme or a dotted host");
});

test("unsafe or unsupported schemes are refused", () => {
  for (const bad of [
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    "data:text/html,<b>x</b>",
    "vbscript:x",
    "file:///c:/a.txt",
    "ftp://example.com",
    "java\tscript:alert(1)",
    "java script:alert(1)",
  ]) {
    assert.ok(url(bad).startsWith("ERR:"), bad);
  }
});

test("empty, spaced and malformed input is refused", () => {
  assert.ok(url("").startsWith("ERR:"));
  assert.ok(url("   ").startsWith("ERR:"));
  assert.ok(url("two words").startsWith("ERR:"));
  assert.ok(url("https://").startsWith("ERR:"));
  assert.ok(url("mailto:").startsWith("ERR:"));
  assert.ok(url("{{url}}").startsWith("ERR:"));
  assert.ok(url("nonsense").startsWith("ERR:"));
  assert.ok(url("a\u0000b.com").startsWith("ERR:"));
});

test("images need http or https", () => {
  assert.deepEqual(normalizeImageUrl("https://x.com/a.png"), { ok: true, url: "https://x.com/a.png" });
  assert.deepEqual(normalizeImageUrl("x.com/a.png"), { ok: true, url: "https://x.com/a.png" });
  assert.equal(normalizeImageUrl("data:image/png;base64,AAAA").ok, false);
  assert.equal(normalizeImageUrl("mailto:a@b.com").ok, false);
  assert.equal(normalizeImageUrl("file:///a.png").ok, false);
});

test("placeholders are found like espanso finds them", () => {
  const text = "Hi {{name}}, on {{ date }} and {{form1.name}} but not {{}} or {name} or {{a b}}.";
  const spans = findPlaceholders(text).map((s) => text.slice(s.from, s.to));
  assert.deepEqual(spans, ["{{name}}", "{{ date }}", "{{form1.name}}"]);
  assert.deepEqual(findPlaceholders("no placeholders"), []);
  assert.deepEqual(findPlaceholders("{{a}}{{b}}").length, 2);
});

test("blank html is recognised", () => {
  assert.equal(isBlankHtml(""), true);
  assert.equal(isBlankHtml("<p></p>"), true);
  assert.equal(isBlankHtml("<p>&nbsp;</p><p><br></p>"), true);
  assert.equal(isBlankHtml("<p>x</p>"), false);
  assert.equal(isBlankHtml("<hr>"), false);
  assert.equal(isBlankHtml('<p><img src="https://x.com/a.png"></p>'), false);
});

test("lists are joined in words", () => {
  assert.equal(listWords([]), "");
  assert.equal(listWords(["bold"]), "bold");
  assert.equal(listWords(["bold", "links"]), "bold and links");
  assert.equal(listWords(["bold", "links", "lists"]), "bold, links and lists");
});
