/**
 * The small set of CSS that rich text keeps, and the strict checks for each value. This is the
 * TypeScript twin of the allow-list in `xpando_core::htmledit`, used by the paste cleaner and
 * the editor so that what they accept is what the Rust sanitiser keeps. The Rust code remains
 * the authority; every value here is matched against a positive pattern and rewritten in one
 * canonical spelling (so `11.0pt` becomes `11pt`). Nothing else is ever passed on: no `url()`,
 * `expression()`, `@import`, escapes or comments.
 *
 * No dependencies, so it runs under Node tests.
 */

export type Decl = [name: string, value: string];

const COLOR_NAMES = new Set([
  "aqua", "black", "blue", "fuchsia", "gray", "grey", "green", "lime", "maroon", "navy", "olive", "orange",
  "purple", "red", "silver", "teal", "white", "yellow", "cyan", "magenta", "brown", "pink", "gold", "darkblue",
  "darkgreen", "darkred", "darkgray", "darkgrey", "lightgray", "lightgrey", "lightblue", "lightgreen",
  "lightyellow", "violet", "indigo", "coral", "crimson", "transparent",
]);
const GENERIC_FAMILIES = new Set(["serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui"]);

/** Splits `a:b; c:d` into lower-case names and raw values. Quotes and brackets protect a `;`. */
export function parseDecls(style: string | null | undefined): Decl[] {
  if (!style) return [];
  const parts: string[] = [];
  let cur = "";
  let quote = "";
  let depth = 0;
  for (const c of style.slice(0, 4000)) {
    if (quote) {
      cur += c;
      if (c === quote) quote = "";
    } else if (c === '"' || c === "'") {
      quote = c;
      cur += c;
    } else if (c === "(") {
      depth++;
      cur += c;
    } else if (c === ")") {
      depth = Math.max(0, depth - 1);
      cur += c;
    } else if (c === ";" && depth === 0) {
      parts.push(cur);
      cur = "";
    } else cur += c;
  }
  parts.push(cur);
  const out: Decl[] = [];
  for (const p of parts) {
    const at = p.indexOf(":");
    if (at < 0) continue;
    const name = p.slice(0, at).trim().toLowerCase();
    const value = p
      .slice(at + 1)
      .trim()
      .replace(/\s*!important\s*$/i, "")
      .trim();
    if (name && value && value.length <= 300) out.push([name, value]);
  }
  return out;
}

export function cssColor(v: string): string | null {
  const t = v.trim().toLowerCase();
  if (t.startsWith("#")) {
    const h = t.slice(1);
    return [3, 4, 6, 8].includes(h.length) && /^[0-9a-f]+$/.test(h) ? t : null;
  }
  if (t === "windowtext") return "#000000";
  if (t === "window") return "#ffffff";
  if (COLOR_NAMES.has(t)) return t;
  const m = /^(rgba?)\(([^()]*)\)$/.exec(t);
  if (!m) return null;
  const parts = m[2].split(/[\s,/]+/).filter(Boolean);
  if (parts.length !== 3 && parts.length !== 4) return null;
  if (!parts.every((p) => /^\d{1,3}(\.\d{1,3})?%?$/.test(p))) return null;
  return `${parts.length === 4 ? "rgba" : "rgb"}(${parts.join(",")})`;
}

/** A number with a unit; zero is written `0`. */
export function cssLen(v: string, neg = false, pct = true): string | null {
  const t = v.trim().toLowerCase();
  const m = /^([+-]?)(\d{0,4})(?:\.(\d{1,3}))?(pt|px|em|rem|in|cm|mm|%)?$/.exec(t);
  if (!m) return null;
  const [, sign, int, frac, unit] = m;
  if (int === "" && frac === undefined) return null;
  if (sign === "-" && !neg) return null;
  if (unit === "%" && !pct) return null;
  const f = Number(`${sign}${int || "0"}.${frac ?? "0"}`);
  if (f === 0) return "0";
  if (!unit) return null;
  return `${f}${unit}`;
}

const nonzero = (v: string | null) => (v === "0" ? null : v);

export function cssFontSize(v: string): string | null {
  const s = cssLen(v, false, true);
  if (!s || s === "0") return null;
  const f = parseFloat(s);
  return f > 0 && f <= 999 ? s : null;
}

export function cssLineHeight(v: string): string | null {
  const t = v.trim().toLowerCase();
  if (t === "normal") return null;
  const len = cssLen(t, false, true);
  if (len) return len === "0" ? null : len;
  if (!/^\d{1,2}(\.\d{1,3})?$/.test(t)) return null;
  const f = Number(t);
  return f >= 0.5 && f <= 10 ? String(f) : null;
}

export function cssFamily(v: string): string | null {
  const names: string[] = [];
  for (const part of v.split(",")) {
    let p = part.trim();
    if (p.length >= 2 && ((p[0] === '"' && p.endsWith('"')) || (p[0] === "'" && p.endsWith("'")))) {
      p = p.slice(1, -1).trim();
    }
    if (p === "" || [...p].length > 60) return null;
    if (!/^[\p{L}\p{N} ._-]+$/u.test(p)) return null;
    p = p.split(/\s+/).join(" ");
    names.push(GENERIC_FAMILIES.has(p.toLowerCase()) ? p.toLowerCase() : `'${p}'`);
    if (names.length > 6) return null;
  }
  return names.length > 0 ? names.join(",") : null;
}

export function cssAlign(v: string): string | null {
  const t = v.trim().toLowerCase();
  return ["left", "center", "right", "justify"].includes(t) ? t : null;
}

function cssValign(v: string): string | null {
  const t = v.trim().toLowerCase();
  return ["top", "middle", "bottom", "baseline"].includes(t) ? t : null;
}

export function cssBorder(v: string): string | null {
  // Spaces inside brackets (`rgb(0, 0, 0)`) do not separate parts.
  let squashed = "";
  let depth = 0;
  for (const c of v.trim().toLowerCase()) {
    if (c === "(") depth++;
    if (c === ")") depth = Math.max(0, depth - 1);
    if (/\s/.test(c) && depth > 0) continue;
    squashed += c;
  }
  let width: string | null = null;
  let style: string | null = null;
  let color: string | null = null;
  const tokens = squashed.split(/\s+/).filter(Boolean);
  if (tokens.length === 0 || tokens.length > 4) return null;
  for (const tok of tokens) {
    let c: string | null;
    let w: string | null;
    if (["none", "hidden", "solid", "dashed", "dotted", "double"].includes(tok)) {
      if (style) return null;
      style = tok;
    } else if (["thin", "medium", "thick"].includes(tok)) {
      if (width) return null;
      width = tok;
    } else if ((c = cssColor(tok))) {
      if (color) return null;
      color = c;
    } else if ((w = cssLen(tok, false, false))) {
      if (width) return null;
      width = w;
    } else if (tok === "auto") {
      continue;
    } else return null;
  }
  if (style === "none" || style === "hidden") return "none";
  const parts = [width, style, color].filter((x): x is string => !!x);
  return parts.length > 0 ? parts.join(" ") : null;
}

function four(v: string, neg: boolean): [string, string, string, string] | null {
  const vals: string[] = [];
  for (const p of v.trim().split(/\s+/)) {
    const l = cssLen(p, neg, true);
    if (l === null) return null;
    vals.push(l);
  }
  const [a, b, c, d] = vals;
  switch (vals.length) {
    case 1:
      return [a, a, a, a];
    case 2:
      return [a, b, a, b];
    case 3:
      return [a, b, c, b];
    case 4:
      return [a, b, c, d];
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------------------
// Style groups
// ---------------------------------------------------------------------------------------

export interface TextStyle {
  color: string | null;
  background: string | null;
  family: string | null;
  size: string | null;
}
export interface BlockStyle {
  align: string | null;
  lineHeight: string | null;
  marginTop: string | null;
  marginBottom: string | null;
  marginLeft: string | null;
  textIndent: string | null;
}
export interface BoxStyle {
  background: string | null;
  width: string | null;
  valign: string | null;
  padding: string | null;
  collapse: string | null;
  /** `border`, `border-top`, `border-right`, `border-bottom`, `border-left`. */
  borders: (string | null)[];
}

export const BORDER_PROPS = ["border", "border-top", "border-right", "border-bottom", "border-left"];

export const emptyText = (): TextStyle => ({ color: null, background: null, family: null, size: null });
export const emptyBlock = (): BlockStyle => ({
  align: null,
  lineHeight: null,
  marginTop: null,
  marginBottom: null,
  marginLeft: null,
  textIndent: null,
});
export const emptyBox = (): BoxStyle => ({
  background: null,
  width: null,
  valign: null,
  padding: null,
  collapse: null,
  borders: [null, null, null, null, null],
});

export function textStyleFrom(decls: Decl[], into: TextStyle = emptyText()): TextStyle {
  const t = { ...into };
  for (const [n, v] of decls) {
    if (n === "color") t.color = cssColor(v) ?? t.color;
    else if (n === "background" || n === "background-color" || n === "mso-highlight") {
      const c = cssColor(v);
      if (c) t.background = c === "transparent" ? null : c;
    } else if (n === "font-family") t.family = cssFamily(v) ?? t.family;
    else if (n === "font-size") t.size = cssFontSize(v) ?? t.size;
  }
  return t;
}

export function blockStyleFrom(decls: Decl[]): BlockStyle {
  const b = emptyBlock();
  for (const [n, v] of decls) {
    if (n === "text-align") b.align = cssAlign(v);
    else if (n === "line-height") b.lineHeight = cssLineHeight(v);
    else if (n === "margin") {
      const f = four(v, false);
      if (f) {
        b.marginTop = nonzero(f[0]);
        b.marginBottom = nonzero(f[2]);
        b.marginLeft = nonzero(f[3]);
      }
    } else if (n === "margin-top") b.marginTop = nonzero(cssLen(v));
    else if (n === "margin-bottom") b.marginBottom = nonzero(cssLen(v));
    else if (n === "margin-left") b.marginLeft = nonzero(cssLen(v));
    else if (n === "text-indent") b.textIndent = nonzero(cssLen(v, true));
  }
  return b;
}

export function boxStyleFrom(decls: Decl[]): BoxStyle {
  const b = emptyBox();
  for (const [n, v] of decls) {
    if (n === "background" || n === "background-color") {
      const c = cssColor(v);
      b.background = c && c !== "transparent" ? c : null;
    } else if (n === "width") b.width = nonzero(cssLen(v));
    else if (n === "vertical-align") b.valign = cssValign(v);
    else if (n === "padding") {
      const f = four(v, false);
      b.padding = f && f.join(" ") !== "0 0 0 0" ? f.join(" ") : null;
    } else if (n === "border-collapse") {
      const t = v.trim().toLowerCase();
      b.collapse = t === "collapse" || t === "separate" ? t : null;
    } else {
      const i = BORDER_PROPS.indexOf(n);
      if (i >= 0) b.borders[i] = cssBorder(v);
    }
  }
  return b;
}

const join = (pairs: [string, string | null][]) =>
  pairs
    .filter((p): p is [string, string] => p[1] !== null && p[1] !== "")
    .map(([n, v]) => `${n}:${v}`)
    .join(";");

export function textStyleCss(t: TextStyle): string {
  return join([
    ["font-family", t.family],
    ["font-size", t.size],
    ["color", t.color],
    ["background-color", t.background],
  ]);
}

export function blockStyleCss(b: BlockStyle): string {
  return join([
    ["text-align", b.align],
    ["line-height", b.lineHeight],
    ["margin-top", b.marginTop],
    ["margin-bottom", b.marginBottom],
    ["margin-left", b.marginLeft],
    ["text-indent", b.textIndent],
  ]);
}

export function boxStyleCss(b: BoxStyle): string {
  return join([
    ["width", b.width],
    ["border-collapse", b.collapse],
    ...BORDER_PROPS.map((n, i): [string, string | null] => [n, b.borders[i]]),
    ["background-color", b.background],
    ["padding", b.padding],
    ["vertical-align", b.valign],
  ]);
}

/** Normalises a `style` string to the block properties kept (for editor attributes). */
export const cleanBlockCss = (style: string | null | undefined) => blockStyleCss(blockStyleFrom(parseDecls(style)));
export const cleanBoxCss = (style: string | null | undefined) => boxStyleCss(boxStyleFrom(parseDecls(style)));

/** Legacy `<font size=1..7>` in points. */
export const FONT_TAG_SIZES: Record<string, string> = {
  "1": "8pt",
  "2": "10pt",
  "3": "12pt",
  "4": "14pt",
  "5": "18pt",
  "6": "24pt",
  "7": "36pt",
};
