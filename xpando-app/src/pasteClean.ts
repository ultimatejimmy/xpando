/**
 * Cleans HTML from the clipboard (Word, Outlook, Excel, web pages, Google Docs) into the
 * formatting rich text keeps. Pure TypeScript with no DOM, so it runs under Node tests.
 *
 * Kept: bold, italic, underline, strikethrough, sub/superscript, text colour, highlight,
 * font and size, links, headings, paragraph alignment, indents and spacing, real lists
 * (Word's "fake" lists are rebuilt, with nesting), tables (borders, shading, widths, merged
 * cells), horizontal rules, quotes, code, and pictures that are on the web or small enough to
 * store inside the text. Dropped: scripts, Word's hidden junk (`mso-*`, conditional comments,
 * `<o:p>`), and pictures that live on the user's disk (`file:`, `cid:`).
 *
 * The Rust sanitiser (`xpando_core::htmledit`) stays the authority; its output spelling is
 * used here (blocks separated by a newline, a lone paragraph without `<p>`, one `<span style>`
 * per run), so it leaves this module's output unchanged.
 *
 * Text is never lost: anything that is not markup stays as text, escaped, so a typed
 * `<wr:forEach select="/Report/Survey">` arrives as those characters.
 */
import {
  blockStyleCss,
  blockStyleFrom,
  boxStyleCss,
  boxStyleFrom,
  cssAlign,
  emptyBlock,
  emptyText,
  FONT_TAG_SIZES,
  parseDecls,
  textStyleCss,
  textStyleFrom,
} from "./cssSubset.ts";
import type { BlockStyle, BoxStyle, Decl, TextStyle } from "./cssSubset.ts";

export interface CleanResult {
  html: string;
  /** What could not be kept, in plain words, for a note under the editor. */
  notes: string[];
}

const NBSP = "\xa0";
/** Largest picture kept inside the text, in characters of its address. */
const MAX_DATA_IMAGE = 400_000;

// ---------------------------------------------------------------------------------------
// Parsing (small, forgiving HTML reader)
// ---------------------------------------------------------------------------------------

interface El {
  tag: string;
  attrs: Record<string, string>;
  children: Node[];
}
type Node = El | string;

const VOID = new Set(["br", "hr", "img", "meta", "link", "col", "input", "base", "area", "wbr", "source", "param"]);
/** Elements whose text content is raw (never markup). Their content is dropped anyway. */
const RAW = new Set(["script", "style", "title", "textarea", "xml", "noscript"]);

const NAMED: Record<string, number> = {
  amp: 0x26, lt: 0x3c, gt: 0x3e, quot: 0x22, apos: 0x27, nbsp: 0xa0,
  ldquo: 0x201c, rdquo: 0x201d, lsquo: 0x2018, rsquo: 0x2019, ndash: 0x2013, mdash: 0x2014,
  hellip: 0x2026, copy: 0xa9, reg: 0xae, trade: 0x2122, bull: 0x2022, middot: 0xb7, euro: 0x20ac,
  deg: 0xb0, times: 0xd7, laquo: 0xab, raquo: 0xbb, sect: 0xa7, para: 0xb6,
};

export function decodeEntities(s: string): string {
  if (!s.includes("&")) return s;
  return s.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z][a-z0-9]*);/gi, (m, name: string) => {
    if (name[0] === "#") {
      const code = name[1] === "x" || name[1] === "X" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      if (!Number.isFinite(code) || code <= 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return m;
      return String.fromCodePoint(code);
    }
    const known = NAMED[name.toLowerCase()];
    return known === undefined ? m : String.fromCodePoint(known);
  });
}

function parseAttrs(src: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  const re = /([^\s"'<>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
  for (let m = re.exec(src); m; m = re.exec(src)) {
    const name = m[1].toLowerCase();
    if (!(name in attrs)) attrs[name] = decodeEntities(m[2] ?? m[3] ?? m[4] ?? "");
  }
  return attrs;
}

/** Keeps only what lies between Word's `StartFragment` and `EndFragment` markers, if present. */
function fragmentOf(html: string): string {
  const s = html.indexOf("<!--StartFragment-->");
  if (s < 0) return html;
  const from = s + "<!--StartFragment-->".length;
  const e = html.indexOf("<!--EndFragment-->", from);
  return e < 0 ? html.slice(from) : html.slice(from, e);
}

const BLOCK_BOUNDARY = new Set(["li", "td", "th", "blockquote", "div", "table", "ul", "ol", "body", "html"]);
const P_CLOSERS = new Set([
  "p", "div", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "table", "blockquote", "pre", "hr", "li", "tr", "td", "th",
]);

function parse(html: string): El {
  const root: El = { tag: "#root", attrs: {}, children: [] };
  const stack: El[] = [root];
  const top = () => stack[stack.length - 1];
  const closeIf = (match: (t: string) => boolean, stopAt: (t: string) => boolean) => {
    for (let i = stack.length - 1; i > 0; i--) {
      const t = stack[i].tag;
      if (match(t)) {
        stack.length = i;
        return;
      }
      if (stopAt(t)) return;
    }
  };
  const text = (t: string) => {
    if (t === "") return;
    const kids = top().children;
    const last = kids[kids.length - 1];
    if (typeof last === "string") kids[kids.length - 1] = last + t;
    else kids.push(t);
  };

  let i = 0;
  const n = html.length;
  while (i < n) {
    const lt = html.indexOf("<", i);
    if (lt < 0) {
      text(decodeEntities(html.slice(i)));
      break;
    }
    if (lt > i) text(decodeEntities(html.slice(i, lt)));
    i = lt;
    const next = html[i + 1] ?? "";
    if (html.startsWith("<!--", i)) {
      const end = html.indexOf("-->", i + 4);
      i = end < 0 ? n : end + 3;
      continue;
    }
    if (next === "!" || next === "?") {
      // Doctype, `<![if ...]>`, `<![endif]>`, processing instructions: dropped, content stays.
      const end = html.indexOf(">", i);
      i = end < 0 ? n : end + 1;
      continue;
    }
    const closing = next === "/";
    if (!/[a-zA-Z]/.test(closing ? (html[i + 2] ?? "") : next)) {
      text("<"); // a lone `<` is text
      i += 1;
      continue;
    }
    // Find the end of the tag, honouring quotes.
    let j = i + (closing ? 2 : 1);
    let quote = "";
    while (j < n) {
      const c = html[j];
      if (quote) {
        if (c === quote) quote = "";
      } else if (c === '"' || c === "'") quote = c;
      else if (c === ">") break;
      j++;
    }
    const inner = html.slice(i + (closing ? 2 : 1), j);
    i = j + 1;
    const m = /^([a-zA-Z][a-zA-Z0-9:_-]*)([\s\S]*)$/.exec(inner);
    if (!m) continue;
    const tag = m[1].toLowerCase();
    if (closing) {
      for (let k = stack.length - 1; k > 0; k--) {
        if (stack[k].tag === tag) {
          stack.length = k;
          break;
        }
      }
      continue;
    }
    const selfClosed = /\/\s*$/.test(m[2]);
    const el: El = { tag, attrs: parseAttrs(m[2].replace(/\/\s*$/, "")), children: [] };

    // Implied end tags, as browsers do.
    if (P_CLOSERS.has(tag)) closeIf((t) => t === "p", (t) => BLOCK_BOUNDARY.has(t));
    if (tag === "li") closeIf((t) => t === "li", (t) => t === "ul" || t === "ol");
    if (tag === "td" || tag === "th") closeIf((t) => t === "td" || t === "th", (t) => t === "tr" || t === "table");
    if (tag === "tr") closeIf((t) => t === "tr", (t) => t === "table");

    top().children.push(el);
    if (RAW.has(tag) && !selfClosed) {
      const close = new RegExp(`</${tag}\\s*>`, "i").exec(html.slice(i));
      i = close ? i + close.index + close[0].length : n;
      continue;
    }
    if (!VOID.has(tag) && !selfClosed) stack.push(el);
  }
  return root;
}

// ---------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------

interface Marks {
  link?: string;
  b?: boolean;
  i?: boolean;
  u?: boolean;
  s?: boolean;
  code?: boolean;
  /** 1 subscript, 2 superscript. */
  script?: number;
  ts: TextStyle;
}
type Inline =
  | { t: "text"; text: string; m: Marks }
  | { t: "br"; word: boolean }
  | { t: "img"; src: string; alt: string; width?: string; height?: string; link?: string };

interface Cell {
  header: boolean;
  colspan: number;
  rowspan: number;
  style: BoxStyle;
  blocks: Block[];
}
type Block =
  | { k: "p"; inl: Inline[]; bs: BlockStyle }
  | { k: "h"; level: 1 | 2 | 3; inl: Inline[]; bs: BlockStyle }
  | { k: "li"; level: number; ordered: boolean; inl: Inline[] } // a Word "fake list" paragraph, grouped later
  | { k: "list"; ordered: boolean; items: Item[] }
  | { k: "quote"; blocks: Block[] }
  | { k: "pre"; text: string }
  | { k: "table"; style: BoxStyle; rows: Cell[][] }
  | { k: "hr" };
interface Item {
  blocks: Block[];
}

interface Ctx {
  images: number;
}

/** What flows down from a parent: text formatting and paragraph alignment. */
interface Inherit {
  marks: Marks;
  align: string | null;
}

const NO_MARKS: Marks = { ts: emptyText() };

const DROP = new Set([
  "head", "script", "style", "title", "xml", "noscript", "textarea", "iframe", "object", "embed", "svg", "template",
  "select", "option", "button", "input", "video", "audio", "canvas", "applet", "meta", "link", "base", "o:p",
]);

function declsOf(el: El): Decl[] {
  let d = parseDecls(el.attrs.style);
  if (el.tag === "font") {
    const f: Decl[] = [];
    if (el.attrs.color) f.push(["color", el.attrs.color]);
    if (el.attrs.face) f.push(["font-family", el.attrs.face]);
    const sz = FONT_TAG_SIZES[(el.attrs.size ?? "").trim()];
    if (sz) f.push(["font-size", sz]);
    d = [...f, ...d];
  }
  return d;
}

function isDropped(el: El): boolean {
  if (DROP.has(el.tag)) return true;
  if (el.tag.startsWith("v:") || el.tag.startsWith("w:") || el.tag.startsWith("o:")) return true;
  const st = Object.fromEntries(parseDecls(el.attrs.style));
  if (st["mso-list"]?.toLowerCase() === "ignore") return true; // Word's list marker
  if (st.display?.toLowerCase() === "none" || st["mso-hide"]?.toLowerCase() === "all") return true;
  const e = st["mso-element"]?.toLowerCase();
  if (e === "comment-list" || e === "comment") return true; // Word comments, not part of the text
  return false;
}

function safeLink(href: string | undefined): string | undefined {
  if (!href) return undefined;
  const h = href.replace(/[\x00-\x20\x7f-\x9f]/g, "");
  return /^(https?:|mailto:|tel:)./i.test(h) ? h : undefined;
}

function safeImage(src: string): string | undefined {
  const s = src.replace(/[\x00-\x20\x7f-\x9f]/g, "");
  if (/^https?:\/\/./i.test(s)) return s;
  const d = /^data:image\/(png|jpeg|jpg|gif|webp);base64,([A-Za-z0-9+/=]+)$/i.exec(s);
  if (d && s.length <= MAX_DATA_IMAGE) return `data:image/${d[1].toLowerCase()};base64,${d[2]}`;
  return undefined;
}

interface Flags {
  bold?: boolean;
  italic?: boolean;
  underline: boolean;
  strike: boolean;
  script?: number;
}

/** Applies the character formatting in `decls` onto a copy of `marks`. */
function applyText(marks: Marks, decls: Decl[], skipBackground = false): { m: Marks; f: Flags } {
  const use = skipBackground
    ? decls.filter(([n]) => n !== "background" && n !== "background-color" && n !== "mso-highlight")
    : decls;
  const m: Marks = { ...marks, ts: textStyleFrom(use, marks.ts) };
  const f: Flags = { underline: false, strike: false };
  for (const [n, raw] of use) {
    const v = raw.trim().toLowerCase();
    if (n === "font-weight") {
      if (v === "bold" || v === "bolder") f.bold = true;
      else if (v === "normal" || v === "lighter") f.bold = false;
      else if (/^\d+$/.test(v)) f.bold = parseInt(v, 10) >= 600;
    } else if (n === "font-style") {
      if (v === "italic" || v === "oblique") f.italic = true;
      else if (v === "normal") f.italic = false;
    } else if (n === "text-decoration" || n === "text-decoration-line") {
      f.underline = v.includes("underline");
      f.strike = v.includes("line-through");
    } else if (n === "vertical-align") {
      if (v === "sub") f.script = 1;
      else if (v === "super") f.script = 2;
      else if (v === "baseline") f.script = 0;
    }
  }
  if (f.script !== undefined) m.script = f.script || undefined;
  if (f.bold !== undefined) m.b = f.bold;
  if (f.italic !== undefined) m.i = f.italic;
  if (f.underline && !m.link) m.u = true; // links are underlined anyway
  if (f.strike) m.s = true;
  return { m, f };
}

function imageDigits(v: string | undefined): string | undefined {
  return v && /^\d{1,5}$/.test(v.trim()) ? v.trim() : undefined;
}

function inlineOf(nodes: Node[], marks: Marks, out: Inline[], ctx: Ctx): void {
  for (const node of nodes) {
    if (typeof node === "string") {
      if (node !== "") out.push({ t: "text", text: node, m: marks });
      continue;
    }
    if (isDropped(node)) continue;
    const st = Object.fromEntries(parseDecls(node.attrs.style));
    const tag = node.tag;
    if (tag === "br") {
      const word = (st["mso-special-character"] ?? "").toLowerCase() === "line-break";
      const pb = (st["page-break-before"] ?? st["page-break-after"] ?? "").toLowerCase();
      if (pb !== "always") out.push({ t: "br", word });
      continue;
    }
    if (tag === "img") {
      const src = safeImage((node.attrs.src ?? "").trim());
      if (src) {
        out.push({ t: "img", src, alt: node.attrs.alt ?? "", width: imageDigits(node.attrs.width), height: imageDigits(node.attrs.height), link: marks.link });
      } else ctx.images++;
      continue;
    }
    inlineOf(node.children, tagMarks(node, marks), out, ctx);
  }
}

/** The formatting an inline element adds: its tag and its `style`. */
function tagMarks(node: El, marks: Marks): Marks {
  {
    const tag = node.tag;
    const { m, f } = applyText(marks, declsOf(node));
    switch (tag) {
      case "b":
      case "strong":
        m.b = f.bold !== false;
        break;
      case "i":
      case "em":
      case "cite":
      case "var":
      case "dfn":
        m.i = f.italic !== false;
        break;
      case "u":
      case "ins":
        m.u = true;
        break;
      case "s":
      case "strike":
      case "del":
        m.s = true;
        break;
      case "code":
      case "kbd":
      case "samp":
      case "tt":
        m.code = true;
        break;
      case "sub":
        m.script = 1;
        break;
      case "sup":
        m.script = 2;
        break;
      case "a": {
        const href = safeLink(node.attrs.href);
        if (href && !marks.link) m.link = href;
        break;
      }
      default:
        break;
    }
    return m;
  }
}

// ---------------------------------------------------------------------------------------
// Whitespace and serialising inline content
// ---------------------------------------------------------------------------------------

export function escapeText(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\xa0/g, "&nbsp;");
}
function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function sameMarks(a: Marks, b: Marks): boolean {
  return (
    a.link === b.link &&
    !a.b === !b.b &&
    !a.i === !b.i &&
    !a.u === !b.u &&
    !a.s === !b.s &&
    !a.code === !b.code &&
    (a.script ?? 0) === (b.script ?? 0) &&
    textStyleCss(a.ts) === textStyleCss(b.ts)
  );
}

/** Collapses whitespace like a browser, keeping runs that contain no-break spaces. */
function normaliseInline(inl: Inline[]): Inline[] {
  const out: Inline[] = [];
  let atStart = true; // nothing yet, or just after a line break: leading space is dropped
  let lastSpace = false;
  let prevWord = false;
  for (const x of inl) {
    if (x.t === "br") {
      // Word writes a second `<br>` for its own "no line break support" branch.
      const prev = out[out.length - 1];
      if (x.word && prevWord && prev?.t === "br") continue;
      // Spaces before a line break are not visible.
      if (prev?.t === "text") prev.text = prev.text.replace(/ +$/, "");
      out.push(x);
      atStart = true;
      lastSpace = false;
      prevWord = x.word;
      continue;
    }
    prevWord = false;
    if (x.t === "img") {
      out.push(x);
      atStart = false;
      lastSpace = false;
      continue;
    }
    let text = "";
    for (const seg of x.text.match(/[ \t\r\n\xa0]+|[^ \t\r\n\xa0]+/g) ?? []) {
      if (/^[ \t\r\n\xa0]/.test(seg)) {
        if (!seg.includes(NBSP)) {
          if (atStart || lastSpace) continue;
          text += " ";
          lastSpace = true;
        } else {
          for (let k = 0; k < seg.length; k++) text += k % 2 === 0 ? NBSP : " ";
          lastSpace = seg.length % 2 === 0;
        }
      } else {
        text += seg;
        lastSpace = false;
      }
      atStart = false;
    }
    if (text === "") continue;
    const prev = out[out.length - 1];
    if (prev?.t === "text" && sameMarks(prev.m, x.m)) prev.text += text;
    else out.push({ t: "text", text, m: x.m });
  }
  // Trim the end: trailing line breaks and spaces are not wanted.
  while (out.length > 0) {
    const last = out[out.length - 1];
    if (last.t === "br") out.pop();
    else if (last.t === "text") {
      last.text = last.text.replace(/ +$/, "");
      if (last.text === "") out.pop();
      else break;
    } else break;
  }
  return out;
}

function isBlank(inl: Inline[]): boolean {
  return inl.every((x) => x.t === "text" && x.text.replace(/[ \xa0]/g, "") === "");
}

const TAGS = ["a", "strong", "em", "u", "s", "code", "sub", "sup", "span"];

/** The mark at `level` for an item, or null. Line breaks carry none; pictures only a link. */
function markKey(x: Inline, level: number): string | null {
  if (x.t === "br") return null;
  const m = x.t === "img" ? ({ link: x.link, ts: emptyText() } as Marks) : x.m;
  switch (level) {
    case 0:
      return m.link ?? null;
    case 1:
      return m.b ? "" : null;
    case 2:
      return m.i ? "" : null;
    case 3:
      return m.u ? "" : null;
    case 4:
      return m.s ? "" : null;
    case 5:
      return m.code ? "" : null;
    case 6:
      return m.script === 1 ? "" : null;
    case 7:
      return m.script === 2 ? "" : null;
    default:
      return textStyleCss(m.ts) || null;
  }
}

/** Nests runs the way the Rust sanitiser writes them: link, strong, em, u, s, code, sub, sup, span. */
function inlineHtml(inl: Inline[], level = 0): string {
  if (level === TAGS.length) {
    let html = "";
    for (const x of inl) {
      if (x.t === "br") html += "<br>";
      else if (x.t === "img") {
        html += `<img src="${escapeAttr(x.src)}"${x.alt ? ` alt="${escapeAttr(x.alt)}"` : ""}${x.width ? ` width="${x.width}"` : ""}${x.height ? ` height="${x.height}"` : ""}>`;
      } else html += escapeText(x.text);
    }
    return html;
  }
  let html = "";
  for (let i = 0; i < inl.length; ) {
    const key = markKey(inl[i], level);
    let j = i + 1;
    while (j < inl.length && markKey(inl[j], level) === key) j++;
    const inner = inlineHtml(inl.slice(i, j), level + 1);
    if (key === null) html += inner;
    else if (level === 0) html += `<a href="${escapeAttr(key)}">${inner}</a>`;
    else if (level === 8) html += `<span style="${escapeAttr(key)}">${inner}</span>`;
    else html += `<${TAGS[level]}>${inner}</${TAGS[level]}>`;
    i = j;
  }
  return html;
}

// ---------------------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------------------

const BLOCK_TAGS = new Set([
  "p", "div", "section", "article", "header", "footer", "main", "nav", "aside", "center", "address", "figure",
  "figcaption", "form", "fieldset", "details", "summary", "dl", "dt", "dd", "body", "html", "ul", "ol", "li",
  "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "hr", "table", "thead", "tbody", "tfoot", "tr", "td",
  "th", "caption", "colgroup", "col", "menu", "dir",
]);

function textOf(nodes: Node[]): string {
  let s = "";
  for (const n of nodes) {
    if (typeof n === "string") s += n;
    else if (n.tag === "br") s += "\n";
    else if (!isDropped(n)) s += textOf(n.children);
  }
  return s;
}

/** Word's marker text (`1.`, `a)`, a bullet) of a fake list paragraph, if it has one. */
function wordMarker(el: El): string | undefined {
  let found: string | undefined;
  const walk = (nodes: Node[]) => {
    for (const n of nodes) {
      if (typeof n === "string" || found !== undefined) continue;
      if (Object.fromEntries(parseDecls(n.attrs.style))["mso-list"]?.toLowerCase() === "ignore") {
        found = textOf(n.children).replace(/[\s\xa0]+/g, "");
      } else walk(n.children);
    }
  };
  walk(el.children);
  return found;
}

function wordListInfo(el: El): { level: number; ordered: boolean } | undefined {
  const raw = Object.fromEntries(parseDecls(el.attrs.style))["mso-list"]?.toLowerCase();
  if (!raw || raw === "none" || raw === "ignore") return undefined;
  const lm = /level(\d+)/.exec(raw);
  if (!lm) return undefined;
  const marker = wordMarker(el) ?? "";
  const ordered = /^\(?([0-9]+|[a-z]|[ivxlcdm]+)[.)]$/i.test(marker);
  return { level: Math.max(1, Math.min(9, parseInt(lm[1], 10))), ordered };
}

function groupWordLists(blocks: Block[]): Block[] {
  const out: Block[] = [];
  type Open = { level: number; list: Extract<Block, { k: "list" }> };
  let stack: Open[] = [];
  for (const b of blocks) {
    if (b.k !== "li") {
      stack = [];
      out.push(b);
      continue;
    }
    while (stack.length > 0 && stack[stack.length - 1].level > b.level) stack.pop();
    let cur = stack[stack.length - 1];
    if (cur && cur.level === b.level && cur.list.ordered !== b.ordered) {
      stack.pop();
      cur = stack[stack.length - 1];
    }
    if (!cur || cur.level < b.level) {
      const list: Extract<Block, { k: "list" }> = { k: "list", ordered: b.ordered, items: [] };
      const parent = cur?.list.items[cur.list.items.length - 1];
      if (parent) parent.blocks.push(list);
      else out.push(list);
      stack.push({ level: b.level, list });
      cur = stack[stack.length - 1];
    }
    cur.list.items.push({ blocks: [{ k: "p", inl: b.inl, bs: emptyBlock() }] });
  }
  return out;
}

function containsBlock(el: El): boolean {
  return el.children.some((c) => typeof c !== "string" && !isDropped(c) && (BLOCK_TAGS.has(c.tag) || containsBlock(c)));
}

const hasBlockChild =(el: El) => el.children.some((c) => typeof c !== "string" && BLOCK_TAGS.has(c.tag) && !isDropped(c));

function blockStyleOf(el: El, decls: Decl[], inh: Inherit): BlockStyle {
  const bs = blockStyleFrom(decls);
  if (!bs.align) bs.align = el.tag === "center" ? "center" : ((el.attrs.align && cssAlign(el.attrs.align)) || inh.align);
  return bs;
}

function legacyWidth(v: string | undefined): Decl[] {
  const t = (v ?? "").trim();
  const m = /^(\d{1,4})(%?)$/.exec(t);
  return m ? [["width", `${m[1]}${m[2] || "px"}`]] : [];
}

function parseRows(nodes: Node[], inh: Inherit, ctx: Ctx, rows: Cell[][], before: Block[]): void {
  for (const n of nodes) {
    if (typeof n === "string" || isDropped(n)) continue;
    if (n.tag === "thead" || n.tag === "tbody" || n.tag === "tfoot") parseRows(n.children, inh, ctx, rows, before);
    else if (n.tag === "caption") before.push(...blocksOf(n.children, ctx, inh));
    else if (n.tag === "tr") {
      const rowDecls = declsOf(n);
      if (n.attrs.bgcolor) rowDecls.unshift(["background-color", n.attrs.bgcolor]);
      const rowBox = boxStyleFrom(rowDecls);
      const cells: Cell[] = [];
      for (const c of n.children) {
        if (typeof c === "string" || (c.tag !== "td" && c.tag !== "th") || isDropped(c)) continue;
        if (cells.length >= 100) break;
        cells.push(parseCell(c, rowBox, inh, ctx));
      }
      if (cells.length > 0 && rows.length < 2000) rows.push(cells);
    }
  }
}

function span(v: string | undefined): number {
  const n = parseInt((v ?? "").trim(), 10);
  return Number.isFinite(n) && n >= 1 && n <= 100 ? n : 1;
}

function parseCell(cell: El, row: BoxStyle, inh: Inherit, ctx: Ctx): Cell {
  const legacy: Decl[] = [];
  if (cell.attrs.bgcolor) legacy.push(["background-color", cell.attrs.bgcolor]);
  if (cell.attrs.align) legacy.push(["text-align", cell.attrs.align]);
  if (cell.attrs.valign) legacy.push(["vertical-align", cell.attrs.valign]);
  legacy.push(...legacyWidth(cell.attrs.width), ...declsOf(cell));
  const style = boxStyleFrom(legacy);
  if (!style.background) style.background = row.background;
  const align = (() => {
    const a = legacy.filter(([n]) => n === "text-align").pop();
    return (a && cssAlign(a[1])) || inh.align;
  })();
  // Text colour and font cascade into the cell; its background does not become a highlight.
  const { m } = applyText(inh.marks, legacy, true);
  const blocks = blocksOf(cell.children, ctx, { marks: m, align });
  if (blocks.length === 0) blocks.push({ k: "p", inl: [], bs: emptyBlock() });
  return { header: cell.tag === "th", colspan: span(cell.attrs.colspan), rowspan: span(cell.attrs.rowspan), style, blocks };
}

function parseTable(el: El, inh: Inherit, ctx: Ctx, out: Block[]): void {
  const decls: Decl[] = [];
  if (el.attrs.bgcolor) decls.push(["background-color", el.attrs.bgcolor]);
  decls.push(...legacyWidth(el.attrs.width), ...declsOf(el));
  const style = boxStyleFrom(decls);
  const rows: Cell[][] = [];
  const before: Block[] = [];
  const { m } = applyText(inh.marks, decls, true);
  parseRows(el.children, { marks: m, align: inh.align }, ctx, rows, before);
  // `border="1"` draws a thin border around every cell.
  if (/^[1-9]\d*$/.test((el.attrs.border ?? "").trim())) {
    for (const c of rows.flat()) {
      if (!c.style.borders.some((b) => b !== null)) c.style.borders[0] = "1px solid #000000";
    }
    style.collapse ??= "collapse";
  }
  out.push(...before);
  if (rows.length > 0) out.push({ k: "table", style, rows });
}

function blocksOf(nodes: Node[], ctx: Ctx, inh: Inherit): Block[] {
  const blocks: Block[] = [];
  let run: Node[] = []; // inline nodes waiting for a block to hold them
  const flush = () => {
    if (run.length === 0) return;
    const inl: Inline[] = [];
    inlineOf(run, inh.marks, inl, ctx);
    run = [];
    const n = normaliseInline(inl);
    if (n.length > 0 && !isBlank(n)) blocks.push({ k: "p", inl: n, bs: { ...emptyBlock(), align: inh.align } });
  };
  const ROWISH = new Set(["tr", "thead", "tbody", "tfoot", "col", "colgroup"]);
  for (let at = 0; at < nodes.length; at++) {
    const node = nodes[at];
    if (typeof node !== "string" && !BLOCK_TAGS.has(node.tag) && !isDropped(node) && containsBlock(node)) {
      // An inline element wrapped around blocks (Google Docs wraps a whole page in `<b>`).
      flush();
      blocks.push(...blocksOf(node.children, ctx, { marks: tagMarks(node, inh.marks), align: inh.align }));
      continue;
    }
    if (typeof node !== "string" && (node.tag === "tr" || node.tag === "tbody") && !isDropped(node)) {
      // Rows outside a table (a copied range of cells) are kept together as one table.
      const rows: Node[] = [node];
      while (at + 1 < nodes.length) {
        const nx = nodes[at + 1];
        if (typeof nx === "string" ? nx.trim() === "" : ROWISH.has(nx.tag)) {
          if (typeof nx !== "string") rows.push(nx);
          at++;
        } else break;
      }
      flush();
      parseTable({ tag: "table", attrs: {}, children: rows }, inh, ctx, blocks);
      continue;
    }
    if (typeof node === "string" || !BLOCK_TAGS.has(node.tag)) {
      if (typeof node !== "string" && isDropped(node)) continue;
      run.push(node);
      continue;
    }
    if (isDropped(node)) continue;
    flush();
    const tag = node.tag;
    const decls = declsOf(node);
    const cell = tag === "td" || tag === "th" || tag === "table" || tag === "tr";
    const { m } = applyText(inh.marks, decls, cell);
    const bs = blockStyleOf(node, decls, inh);
    const below: Inherit = { marks: m, align: bs.align };
    const inline = (): Inline[] => {
      const inl: Inline[] = [];
      inlineOf(node.children, m, inl, ctx);
      return normaliseInline(inl);
    };
    if (tag === "hr") {
      blocks.push({ k: "hr" });
    } else if (/^h[1-6]$/.test(tag)) {
      const n = inline();
      if (n.length > 0 && !isBlank(n)) blocks.push({ k: "h", level: Math.min(3, Number(tag[1])) as 1 | 2 | 3, inl: n, bs });
    } else if (tag === "p" || tag === "li") {
      const info = tag === "p" ? wordListInfo(node) : undefined;
      if (info) {
        blocks.push({ k: "li", level: info.level, ordered: info.ordered, inl: inline() });
      } else if (hasBlockChild(node)) {
        blocks.push(...blocksOf(node.children, ctx, below)); // a paragraph holding blocks
      } else {
        const n = inline();
        blocks.push({ k: "p", inl: isBlank(n) ? [] : n, bs }); // an empty one is a blank line
      }
    } else if (tag === "ul" || tag === "ol") {
      const items: Item[] = [];
      let loose: Node[] = [];
      const flushLoose = () => {
        const b = blocksOf(loose, ctx, below);
        loose = [];
        if (b.length > 0) items.push({ blocks: b });
      };
      for (const c of node.children) {
        if (typeof c !== "string" && c.tag === "li") {
          flushLoose();
          const { m: lm } = applyText(m, declsOf(c));
          const b = blocksOf(c.children, ctx, { marks: lm, align: below.align });
          items.push({ blocks: b.length > 0 ? b : [{ k: "p", inl: [], bs: emptyBlock() }] });
        } else loose.push(c);
      }
      flushLoose();
      if (items.length > 0) blocks.push({ k: "list", ordered: tag === "ol", items });
    } else if (tag === "blockquote") {
      const b = blocksOf(node.children, ctx, below);
      if (b.length > 0) blocks.push({ k: "quote", blocks: b });
    } else if (tag === "pre") {
      blocks.push({ k: "pre", text: textOf(node.children).replace(/^\n/, "").replace(/\s+$/, "") });
    } else if (tag === "table") {
      parseTable(node, { marks: m, align: inh.align }, ctx, blocks);
    } else if (tag === "thead" || tag === "tbody" || tag === "tfoot" || tag === "tr") {
      // Rows outside a table (a copied range of cells) are kept as a table.
      parseTable({ tag: "table", attrs: {}, children: [node] }, inh, ctx, blocks);
    } else {
      // div, section, center, a stray td, ...
      blocks.push(...blocksOf(node.children, ctx, below));
    }
  }
  flush();
  return groupWordLists(blocks);
}

function listHtml(l: Extract<Block, { k: "list" }>): string {
  const tag = l.ordered ? "ol" : "ul";
  const items = l.items.map((it) => `<li>${containerHtml(it.blocks)}</li>`);
  return `<${tag}>${items.join("")}</${tag}>`;
}

const plainStyle = (bs: BlockStyle) => blockStyleCss(bs) === "";
const styleAttr = (css: string) => (css ? ` style="${escapeAttr(css)}"` : "");

/** Content of a list item or cell: a single plain paragraph is written without `<p>`. */
function containerHtml(bs: Block[]): string {
  const [first, ...rest] = bs;
  if (first?.k === "p" && first.inl.length > 0 && plainStyle(first.bs) && rest.every((b) => b.k === "list")) {
    return inlineHtml(first.inl) + rest.map((b) => blockHtml(b)).join("");
  }
  if (bs.length === 1 && first.k === "p" && first.inl.length === 0 && plainStyle(first.bs)) return "";
  return bs.map((b) => blockHtml(b)).join("");
}

function tableHtml(b: Extract<Block, { k: "table" }>): string {
  let s = `<table${styleAttr(boxStyleCss(b.style))}>`;
  for (const row of b.rows) {
    s += "<tr>";
    for (const c of row) {
      const tag = c.header ? "th" : "td";
      s += `<${tag}${c.colspan > 1 ? ` colspan="${c.colspan}"` : ""}${c.rowspan > 1 ? ` rowspan="${c.rowspan}"` : ""}${styleAttr(boxStyleCss(c.style))}>${containerHtml(c.blocks)}</${tag}>`;
    }
    s += "</tr>";
  }
  return `${s}</table>`;
}

function blockHtml(b: Block, lone = false): string {
  switch (b.k) {
    case "p":
      if (b.inl.length === 0) return `<p${styleAttr(blockStyleCss(b.bs))}><br></p>`;
      if (lone && plainStyle(b.bs)) return inlineHtml(b.inl);
      return `<p${styleAttr(blockStyleCss(b.bs))}>${inlineHtml(b.inl)}</p>`;
    case "h":
      return `<h${b.level}${styleAttr(blockStyleCss(b.bs))}>${inlineHtml(b.inl)}</h${b.level}>`;
    case "li":
      return `<p>${inlineHtml(b.inl)}</p>`;
    case "list":
      return listHtml(b);
    case "table":
      return tableHtml(b);
    case "quote":
      return `<blockquote>${containerHtml(b.blocks)}</blockquote>`;
    case "pre":
      return `<pre><code>${escapeText(b.text)}</code></pre>`;
    case "hr":
      return "<hr>";
  }
}

function describe(images: number): string[] {
  const out: string[] = [];
  if (images > 0) {
    out.push(
      images === 1
        ? "One picture was left out because it is not on the web. Only pictures with a web address (https) can be kept."
        : `${images} pictures were left out because they are not on the web. Only pictures with a web address (https) can be kept.`,
    );
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Public
// ---------------------------------------------------------------------------------------

/** True for HTML that came from this editor itself (copy and paste inside xpando). */
export function isInternalPaste(html: string): boolean {
  return html.includes("data-pm-slice");
}

const isBlankPara = (b: Block) => b.k === "p" && b.inl.length === 0;

/** Cleans clipboard HTML into the allowed subset. Safe to call on any string. */
export function cleanPastedHtml(html: string): CleanResult {
  const ctx: Ctx = { images: 0 };
  const root = parse(fragmentOf(html));
  const blocks = blocksOf(root.children, ctx, { marks: NO_MARKS, align: null });
  // Leading and trailing blank paragraphs are not wanted.
  while (blocks.length > 0 && isBlankPara(blocks[0])) blocks.shift();
  while (blocks.length > 0 && isBlankPara(blocks[blocks.length - 1])) blocks.pop();
  const lone = blocks.length === 1 && blocks[0].k === "p";
  const out = blocks.map((b) => blockHtml(b, lone)).join("\n");
  return { html: out, notes: describe(ctx.images) };
}

/** Plain text as HTML: blank lines separate paragraphs, a single line break is `<br>`. */
export function plainTextToHtml(text: string): string {
  const t = text.replace(/\r\n?/g, "\n").replace(/\0/g, "");
  const paras = t
    .split(/\n{2,}/)
    .map((p) => p.replace(/^\n+|\n+$/g, ""))
    .filter((p) => p.trim() !== "");
  const html = paras.map((p) =>
    p
      .split("\n")
      .map((line) => escapeText(line.replace(/ {2,}/g, (run) => run.replace(/ /g, (_c, k: number) => (k % 2 === 0 ? NBSP : " ")))))
      .join("<br>"),
  );
  if (html.length === 1) return html[0];
  return html.map((h) => `<p>${h}</p>`).join("\n");
}

/** What the clipboard held, reduced to one cleaned result. */
export interface ClipboardContent {
  /** `rich` when HTML with real formatting was found, otherwise `plain`. */
  kind: "rich" | "plain" | "empty";
  html: string;
  /** The plain text, for plain mode. */
  text: string;
  notes: string[];
}

const FORMATTED = /<(strong|em|u|s|a|code|h[1-3]|ul|ol|blockquote|pre|hr|img|table|sub|sup)[\s>]|style="/;

/**
 * Chooses between the clipboard's HTML and plain text. HTML that cleans to nothing falls back
 * to the text; HTML with no formatting at all (what many programs copy for plain text) counts
 * as plain, and the text is used as it is.
 */
export function fromClipboard(html: string | null | undefined, text: string | null | undefined): ClipboardContent {
  const plain = (text ?? "").replace(/\r\n?/g, "\n");
  const hasText = plain.trim() !== "";
  if (html && html.trim() !== "") {
    const c = cleanPastedHtml(html);
    if (c.html.trim() !== "") {
      return { kind: FORMATTED.test(c.html) || !hasText ? "rich" : "plain", html: c.html, text: plain, notes: c.notes };
    }
  }
  if (hasText) return { kind: "plain", html: plainTextToHtml(plain), text: plain, notes: [] };
  return { kind: "empty", html: "", text: "", notes: [] };
}
