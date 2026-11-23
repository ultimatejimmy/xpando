/**
 * Pure helpers for the rich text editor: address validation and placeholder finding.
 * No dependencies, so they can be tested with Node. The Rust sanitiser
 * (`xpando_core::htmledit`) is the authority on what is saved; these only give the user
 * early, readable feedback and must never accept something the sanitiser would drop.
 */

export type UrlResult = { ok: true; url: string } | { ok: false; error: string };

const SCHEME = /^([a-z][a-z0-9+.-]*):/i;
const EMAIL = /^[^@\s/:]+@[^@\s/:]+\.[^@\s/:]+$/;
const DOMAIN = /^[a-z0-9-]+(\.[a-z0-9-]+)+([/:?#].*)?$/i;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

const KNOWN = ["http", "https", "mailto", "tel"];
const HELP = "Enter a web address like https://example.com, an email address, or a tel: number.";

function hasContentAfter(url: string, prefix: string): boolean {
  return url.length > prefix.length;
}

/** Checks an address typed for a link; adds `https://` or `mailto:` when it is obvious. */
export function normalizeLinkUrl(input: string): UrlResult {
  const s = input.trim();
  if (s === "") return { ok: false, error: "Enter an address." };
  if (CONTROL.test(s)) return { ok: false, error: "The address contains characters that are not allowed." };
  if (/\s/.test(s)) return { ok: false, error: "An address can't contain spaces." };

  const scheme = SCHEME.exec(s)?.[1].toLowerCase();
  // "localhost:3000" or "host:80" look like a scheme; treat a number after the colon as a port.
  if (scheme && (KNOWN.includes(scheme) || !/^[a-z][a-z0-9+.-]*:\d/i.test(s))) {
    if (scheme === "http" || scheme === "https") {
      return /^https?:\/\/./i.test(s) && hasContentAfter(s, `${scheme}://`)
        ? { ok: true, url: s }
        : { ok: false, error: "The web address is missing its host name." };
    }
    if (scheme === "mailto" || scheme === "tel") {
      return hasContentAfter(s, `${scheme}:`)
        ? { ok: true, url: s }
        : { ok: false, error: `Nothing follows “${scheme}:”.` };
    }
    return { ok: false, error: `“${scheme}:” links are not supported. Use http, https, mailto or tel.` };
  }
  if (EMAIL.test(s)) return { ok: true, url: `mailto:${s}` };
  if (DOMAIN.test(s)) return { ok: true, url: `https://${s}` };
  return { ok: false, error: HELP };
}

/** Checks an address typed for an image: only http and https are kept. */
export function normalizeImageUrl(input: string): UrlResult {
  const r = normalizeLinkUrl(input);
  if (!r.ok) return r;
  if (!/^https?:\/\//i.test(r.url)) {
    return { ok: false, error: "Images must be at an http or https address." };
  }
  return r;
}

export interface Span {
  from: number;
  to: number;
}

/** espanso's variable syntax: {{name}} or {{name.field}}, spaces allowed inside the braces. */
export const PLACEHOLDER = /\{\{\s*\w+(?:\.\w+)?\s*\}\}/g;

/** Positions of every `{{placeholder}}` in `text`. */
export function findPlaceholders(text: string): Span[] {
  const out: Span[] = [];
  for (const m of text.matchAll(PLACEHOLDER)) {
    out.push({ from: m.index, to: m.index + m[0].length });
  }
  return out;
}

/** Whether an HTML body shows nothing: no text, no image, no rule. */
export function isBlankHtml(html: string): boolean {
  if (/<(img|hr)\b/i.test(html)) return false;
  const text = html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;| /g, " ");
  return text.trim() === "";
}

/** "bold", "bold and links", "bold, links and lists". */
export function listWords(items: string[]): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}
