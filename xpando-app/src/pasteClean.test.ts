// Run: node --test --experimental-strip-types src/pasteClean.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import { cleanPastedHtml, fromClipboard, isInternalPaste, plainTextToHtml } from "./pasteClean.ts";
import { cssBorder, cssColor, cssFamily, cssFontSize, cssLen, cssLineHeight, parseDecls } from "./cssSubset.ts";
import * as F from "./pasteFixtures.ts";

const clean = (html: string) => cleanPastedHtml(html);
const SPAN = (css: string, text: string) => `<span style="${css}">${text}</span>`;
const CAL = "font-family:'Calibri',sans-serif;font-size:11pt";

test("Word text keeps bold, italic, underline, strike, colour, highlight, fonts, sizes, sub and superscript", () => {
  const { html, notes } = clean(F.WORD_FORMATTED);
  assert.deepEqual(notes, []);
  assert.ok(html.includes(`<strong>${SPAN(`${CAL};color:#c00000`, "Customer")}</strong>`), "bold + colour");
  assert.ok(html.includes(`<em>${SPAN(CAL, "very")}</em>`), "italic");
  assert.ok(html.includes(`<u>${SPAN(CAL, "important")}</u>`), "underline");
  assert.ok(html.includes(`<s>${SPAN(CAL, "old")}</s>`), "strike");
  assert.ok(html.includes(SPAN(`${CAL};background-color:yellow`, "highlighted")), "highlight");
  assert.ok(html.includes(`<sub>${SPAN(CAL, "2")}</sub>`), "subscript");
  assert.ok(html.includes(`<sup>${SPAN(CAL, "2")}</sup>`), "superscript");
});

test("Word headings, alignment, indent and line spacing are kept", () => {
  const { html } = clean(F.WORD_FORMATTED);
  assert.ok(html.startsWith(`<h1 style="text-align:center">${SPAN("font-family:'Calibri Light',sans-serif;font-size:20pt;color:#2f5496", "Monthly&nbsp;report")}</h1>\n`));
  assert.ok(html.includes('<p style="text-align:right;line-height:150%;margin-left:0.5in">'));
});

test("Word junk is gone: styles, conditional comments, mso, o:p, classes, the document wrapper", () => {
  for (const fixture of Object.values(F)) {
    const { html } = clean(fixture);
    for (const junk of ["mso-", "MsoNormal", "o:p", "<!--", "StartFragment", "@font-face", "panose", "<html", "<body", "<head", "class=", "xmlns", "Microsoft Word"]) {
      assert.ok(!html.includes(junk), `${junk} left in ${html.slice(0, 120)}`);
    }
  }
});

test("smart quotes and dashes are kept; spacing runs and no-break spaces stay readable", () => {
  const { html } = clean(F.WORD_FORMATTED);
  assert.ok(html.includes("It\u2019s "), "apostrophe");
  assert.ok(html.includes("\u201cread this\u201d&nbsp; today \u2013 "), "quotes, spacerun, dash");
  assert.ok(html.includes("Monthly&nbsp;report"));
  // A blank paragraph is kept between text, not at the ends.
  assert.ok(html.includes("<p><br></p>\n<p>"));
  assert.ok(!html.startsWith("<p><br>") && !html.endsWith("<br></p>"));
});

test("links keep only safe addresses and their formatting", () => {
  const { html } = clean(F.WORD_FORMATTED);
  assert.ok(html.includes('<a href="https://example.com/a?b=1&amp;c=2">'));
  assert.ok(html.includes('<a href="mailto:help@example.com">'));
  assert.equal(clean('<a href="javascript:alert(1)">x</a>').html, "x");
  assert.equal(clean('<a href=" JaVa\tScript:alert(1)">x</a>').html, "x");
  assert.equal(clean('<a href="file:///C:/a.docx">x</a>').html, "x");
  assert.equal(clean('<a href="#_Toc123">x</a>').html, "x");
  assert.equal(clean('<a name="_Toc123">x</a>').html, "x");
  // A link's own underline and blue are not turned into extra underline marks.
  assert.equal(
    clean('<a href="https://x.com"><span style="color:#0563C1;text-decoration:underline">l</span></a>').html,
    '<a href="https://x.com"><span style="color:#0563c1">l</span></a>',
  );
});

test("Word fake lists become real lists with nesting and the right kind", () => {
  const { html } = clean(F.WORD_LISTS);
  assert.equal(
    html,
    "<ul><li>First item<ul><li>Nested one</li><li>Nested <strong>two</strong></li></ul></li><li>Second item</li></ul>\n" +
      "<p>Steps:</p>\n" +
      "<ol><li>Open it</li><li>Press <em>go</em></li></ol>",
  );
});

test("a deeper list level without a parent item, and a bullet followed by a number, still work", () => {
  const item = (level: number, marker: string, text: string) =>
    `<p class=MsoListParagraph style='mso-list:l0 level${level} lfo1'><![if !supportLists]><span style='mso-list:Ignore'>${marker}<span>&nbsp;&nbsp;</span></span><![endif]>${text}</p>`;
  assert.equal(clean(item(2, "o", "deep") + item(1, "a)", "letters")).html, "<ul><li>deep</li></ul>\n<ol><li>letters</li></ol>");
  assert.equal(clean(item(1, "\u00b7", "a") + item(1, "1.", "b")).html, "<ul><li>a</li></ul>\n<ol><li>b</li></ol>");
});

test("real lists keep their nesting; list items keep their text formatting", () => {
  assert.equal(
    clean("<ul><li>a<ul><li><b>b</b></li></ul></li><li style='color:red'>c</li></ul>").html,
    '<ul><li>a<ul><li><strong>b</strong></li></ul></li><li><span style="color:red">c</span></li></ul>',
  );
});

test("a shaded, bordered Word table with merged cells keeps its structure and look", () => {
  const { html, notes } = clean(F.WORD_TABLE);
  assert.deepEqual(notes, []);
  assert.ok(html.startsWith('<table style="width:351pt;border-collapse:collapse;border:none">'));
  assert.ok(html.includes('<td colspan="2" style="width:351pt;border:1pt solid #000000;background-color:#d9e2f3;padding:0 5.4pt 0 5.4pt;vertical-align:top"><p style="text-align:center"><strong>Quarter</strong></p></td>'), "merged header, shading, border, centring");
  assert.ok(html.includes('<td rowspan="2" style="width:175.5pt;border:1pt solid #000000;border-top:none;'), "rowspan, one side without a border");
  assert.ok(html.includes("background-color:#fff2cc"), "cell shading");
  assert.ok(html.includes('<span style="color:red">10&nbsp;%</span>'), "text colour in a cell");
  assert.equal(html.split("<tr>").length - 1, 3);
  assert.ok(!html.includes("<p><br></p>"), "the empty paragraph after the table is dropped");
});

test("Excel cells and ranges become a table; a plain cell is still a table", () => {
  assert.equal(clean(F.EXCEL_CELL).html, '<table><tr><td style="width:48pt">Hello cell</td></tr></table>');
  const range = clean(F.EXCEL_RANGE).html;
  assert.equal(range.split("<table").length - 1, 1, "one table, not one per row");
  assert.ok(range.includes('background-color:#c6e0b4'));
  assert.ok(range.includes('<p style="text-align:right">1,5</p>'));
  assert.ok(range.includes("<td></td>"));
});

test("Google Docs: the not-bold wrapper does not bold everything; bold, italic, underline, highlight survive", () => {
  const { html } = clean(F.GOOGLE_DOCS);
  assert.ok(html.startsWith('<h2 style="line-height:1.38;margin-top:18pt;margin-bottom:6pt">'));
  assert.ok(!html.includes("<strong>Heading") && !html.includes("<strong><span style=\"font-family:'Arial',sans-serif;font-size:11pt;color:#000000\">, then"));
  assert.ok(html.includes("<strong><em><u><span"), "bold italic underline from style");
  assert.ok(html.includes("background-color:#ffff00"));
  assert.ok(html.includes('<p style="text-align:justify;line-height:1.38">'));
  assert.ok(html.includes("<ul><li>"));
});

test("Outlook keeps fonts and colour and the line breaks", () => {
  const { html } = clean(F.OUTLOOK);
  assert.ok(html.startsWith(`<p>${SPAN("font-family:'Segoe UI',sans-serif;color:#1f3864", "Hi team,")}</p>`));
  assert.ok(html.includes("<br>") && html.includes("Best regards,") && html.includes("Alex"));
});

test("web pages lose classes, ids, scripts and handlers but keep structure", () => {
  const { html, notes } = clean(F.WEB_PAGE);
  assert.equal(
    html,
    '<h3>Release <em>notes</em></h3>\n<p>Read the guide or <a href="https://example.com/x">the site</a>.</p>\n<ul><li>one</li><li>two <strong>bold</strong></li></ul>\n<blockquote>Quoted</blockquote>\n<pre><code>line 1\n  line 2</code></pre>\n<hr>',
  );
  assert.equal(notes.length, 1, "the relative picture is reported");
});

test("pictures: web and small embedded ones are kept; disk and mail ones are dropped with a note", () => {
  const { html, notes } = clean(F.WORD_IMAGES);
  assert.ok(html.includes('<img src="https://example.com/logo.png" alt="Logo" width="40" height="20">'));
  assert.ok(!html.includes("file:") && !html.includes("cid:") && !html.includes("<v:"));
  assert.equal(notes.length, 1);
  assert.match(notes[0], /2 pictures were left out/);
  assert.equal(clean('<img src="file:///C:/a.png">').html, "");
  assert.equal(clean('<img src="file:///C:/a.png">').notes.length, 1);
  assert.equal(
    clean('<img src="data:image/png;base64,iVBORw0KGgo=">').html,
    '<img src="data:image/png;base64,iVBORw0KGgo=">',
  );
  assert.equal(clean('<img src="data:image/svg+xml;base64,AAAA">').html, "");
  assert.equal(clean('<img src="data:text/html;base64,AAAA">').html, "");
  const big = `<img src="data:image/png;base64,${"A".repeat(400_001)}">`;
  assert.equal(clean(big).html, "");
});

test("text that looks like a tag stays literal text", () => {
  const { html } = clean(F.WORD_LITERAL_TAG);
  assert.ok(html.includes('<span style="font-family:\'Courier New\'">&lt;wr:forEach select="/Report/Survey"&gt;</span>'));
  assert.ok(html.includes("<p>Name: {{name}} &amp; &lt;b&gt;not bold&lt;/b&gt; 5 &lt; 6</p>"));
  assert.ok(html.includes("<p>&lt;/wr:forEach&gt;</p>"));
});

test("plain text is escaped, never read as markup", () => {
  const tag = '<wr:forEach select="/Report/Survey">';
  assert.equal(plainTextToHtml(tag), '&lt;wr:forEach select="/Report/Survey"&gt;');
  assert.equal(plainTextToHtml(`${tag}\n  {{name}} & <b>x</b>\n</wr:forEach>`), '&lt;wr:forEach select="/Report/Survey"&gt;<br>&nbsp; {{name}} &amp; &lt;b&gt;x&lt;/b&gt;<br>&lt;/wr:forEach&gt;');
  assert.equal(plainTextToHtml("a\n\nb"), "<p>a</p>\n<p>b</p>");
  assert.equal(plainTextToHtml("   \n\n "), "");
  // Plain text only: the text/plain flavour is used as it is.
  const c = fromClipboard(null, `${tag}\r\nbody`);
  assert.equal(c.kind, "plain");
  assert.equal(c.text, `${tag}\nbody`);
  assert.equal(c.html, '&lt;wr:forEach select="/Report/Survey"&gt;<br>body');
  // Escaped text inside HTML comes back as the same characters.
  assert.equal(clean("&lt;wr:forEach select=&quot;/Report/Survey&quot;&gt;").html, '&lt;wr:forEach select="/Report/Survey"&gt;');
  // A stray "<" and unknown entities are text too.
  assert.equal(clean("a < b &foo; c").html, "a &lt; b &amp;foo; c");
});

test("no text is lost from the real Word fixtures", () => {
  const words = (s: string) => s.replace(/<[^>]*>/g, " ").replace(/&nbsp;|&amp;|&lt;|&gt;/g, " ").split(/\s+/).filter(Boolean).join(" ");
  assert.equal(words(clean(F.WORD_LISTS).html), "First item Nested one Nested two Second item Steps: Open it Press go");
  assert.equal(words(clean(F.WORD_TABLE).html), "Quarter Q1 10 % 12 %");
  assert.match(words(clean(F.WORD_FORMATTED).html), /^Monthly report Dear Customer , It\u2019s very important that you \u201cread this\u201d today \u2013 highlighted , old and H 2 O, x 2 \. Visit our site or email us \. Last line$/);
});

test("hostile input does not survive", () => {
  const hostile = [
    '<p onclick="x()" style="color:red;background:url(javascript:alert(1));behavior:url(x.htc)">a</p>',
    '<span style="font-family:Arial;}</style><script>alert(1)</script>">b</span><script>alert(2)</script>',
    '<a href="jav&#x61;script:alert(1)" style="color:expression(alert(1))">c</a>',
    '<table onload="x()"><tr><td style="width:expression(alert(1))" colspan="1000">d</td></tr></table>',
    '<iframe src="https://evil.example"></iframe><object data="x"></object><embed src="x"><svg onload="x()"></svg>e',
    '<img src="javascript:alert(1)" onerror="x()"><img src=x onerror=alert(1)>',
    '<p style="margin-left:expression(1);text-align:javascript:1;line-height:url(x)">f</p>',
  ];
  for (const h of hostile) {
    const out = clean(h).html.toLowerCase();
    for (const bad of ["script", "onclick", "onerror", "onload", "javascript", "url(", "expression", "iframe", "<object", "<embed", "<svg", "behavior"]) {
      assert.ok(!out.includes(bad), `${bad} survived in ${out} (from ${h})`);
    }
  }
});

test("internal copies and empty or odd input", () => {
  assert.ok(isInternalPaste('<p data-pm-slice="1 1 []">x</p>'));
  assert.ok(!isInternalPaste("<p>x</p>"));
  assert.equal(clean("").html, "");
  assert.equal(clean("<!--StartFragment--><!--EndFragment-->").html, "");
  assert.equal(clean("<p>&nbsp;</p>").html, "");
  assert.equal(clean("just text").html, "just text");
  assert.equal(clean("<b>unclosed <i>tags").html, "<strong>unclosed <em>tags</em></strong>");
  assert.equal(fromClipboard("", "").kind, "empty");
});

test("fromClipboard decides rich or plain", () => {
  assert.equal(fromClipboard(F.WORD_FORMATTED, "x").kind, "rich");
  assert.equal(fromClipboard("<p>just text</p>", "just text").kind, "plain");
  assert.equal(fromClipboard("<p>just text</p>", "just text").text, "just text");
  assert.equal(fromClipboard("<p>just text</p>", "").kind, "rich");
  assert.equal(fromClipboard("<b>bold</b>", "bold").kind, "rich");
  assert.equal(fromClipboard("<script>x</script>", "fallback").kind, "plain");
  assert.equal(fromClipboard(F.EXCEL_CELL, "Hello cell").kind, "rich");
});

// ---- the CSS rules ---------------------------------------------------------------

test("css colours", () => {
  assert.equal(cssColor("#FF0000"), "#ff0000");
  assert.equal(cssColor("RED"), "red");
  assert.equal(cssColor("rgb(1, 2, 3)"), "rgb(1,2,3)");
  assert.equal(cssColor("windowtext"), "#000000");
  for (const bad of ["url(x)", "expression(1)", "#12", "#ggg", "rgb(a,b,c)", "javascript:1", "var(--x)", "red url(x)", "", "notacolour"]) {
    assert.equal(cssColor(bad), null, bad);
  }
});

test("css lengths, sizes, spacing", () => {
  assert.equal(cssLen("11.0pt"), "11pt");
  assert.equal(cssLen(".5in"), "0.5in");
  assert.equal(cssLen("0"), "0");
  assert.equal(cssLen("-1pt"), null);
  assert.equal(cssLen("-1pt", true), "-1pt");
  assert.equal(cssLen("12"), null);
  assert.equal(cssLen("1e3px"), null);
  assert.equal(cssLen("calc(1px)"), null);
  assert.equal(cssFontSize("14px"), "14px");
  assert.equal(cssFontSize("0pt"), null);
  assert.equal(cssFontSize("99999pt"), null);
  assert.equal(cssLineHeight("107%"), "107%");
  assert.equal(cssLineHeight("1.5"), "1.5");
  assert.equal(cssLineHeight("normal"), null);
  assert.equal(cssLineHeight("50"), null);
});

test("css families and borders", () => {
  assert.equal(cssFamily('"Times New Roman", serif'), "'Times New Roman',serif");
  assert.equal(cssFamily("Calibri"), "'Calibri'");
  assert.equal(cssFamily("a(b)"), null);
  assert.equal(cssFamily("a;b"), null);
  assert.equal(cssFamily("url(x)"), null);
  assert.equal(cssFamily("a\\62"), null);
  assert.equal(cssBorder("solid windowtext 1.0pt"), "1pt solid #000000");
  assert.equal(cssBorder("1pt solid rgb(0, 0, 0)"), "1pt solid rgb(0,0,0)");
  assert.equal(cssBorder("none"), "none");
  assert.equal(cssBorder("url(x) solid"), null);
  assert.equal(cssBorder("solid solid"), null);
});

test("declaration splitting respects quotes and brackets", () => {
  assert.deepEqual(parseDecls("a:b; C : d !important;;e"), [["a", "b"], ["c", "d"]]);
  assert.deepEqual(parseDecls("font-family:'a;b';x:url(a;b)"), [["font-family", "'a;b'"], ["x", "url(a;b)"]]);
});
