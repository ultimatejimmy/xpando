/**
 * Search query parsing and matching. Pure, no dependencies, so it can be tested with Node.
 *
 * Operators (Advanced mode): `trigger:`, `label:`, `folder:`, `file:`, `comment:`, `body:`.
 * Each takes a word or a "quoted phrase". Several terms must all match (AND). Anything else is
 * a bare word that searches triggers, label, comment and body, like plain search.
 */

export type Field = "trigger" | "label" | "folder" | "file" | "comment" | "body";

export const OPERATORS: readonly Field[] = ["trigger", "label", "folder", "file", "comment", "body"];

export interface Term {
  /** `null` for a bare word: searches every field. */
  field: Field | null;
  /** Lowercased text to find. */
  value: string;
}

/** The searchable text of one phrase. */
export interface SearchDoc {
  triggers: string[];
  /** The name shown for the phrase (its label, or a name taken from its comment). */
  label: string;
  comment: string;
  body: string;
  /** Display name of the folder. */
  folder: string;
  /** File path relative to the match directory. */
  file: string;
}

const FIELD_SET: ReadonlySet<string> = new Set(OPERATORS);

/**
 * Splits a query into terms. With `operators` off, the whole query is one bare term, which is
 * exactly plain substring search. A `field:` prefix with nothing after it yet (still typing) is
 * dropped; an unknown prefix such as `http:x` or a trigger like `:hi` stays a bare word.
 */
export function parseQuery(query: string, operators: boolean): Term[] {
  if (!operators) {
    const value = query.trim().toLowerCase();
    return value ? [{ field: null, value }] : [];
  }
  const terms: Term[] = [];
  let i = 0;
  const n = query.length;
  while (i < n) {
    if (/\s/.test(query[i])) {
      i++;
      continue;
    }
    let field: Field | null = null;
    const m = /^([a-z]+):/i.exec(query.slice(i));
    if (m && FIELD_SET.has(m[1].toLowerCase())) {
      field = m[1].toLowerCase() as Field;
      i += m[0].length;
    }
    let value = "";
    if (query[i] === '"') {
      const end = query.indexOf('"', i + 1);
      value = end === -1 ? query.slice(i + 1) : query.slice(i + 1, end);
      i = end === -1 ? n : end + 1;
    } else {
      let j = i;
      while (j < n && !/\s/.test(query[j])) j++;
      value = query.slice(i, j);
      i = j;
    }
    value = value.trim().toLowerCase();
    if (value) terms.push({ field, value });
  }
  return terms;
}

function fieldText(doc: SearchDoc, field: Field | null): string[] {
  switch (field) {
    case "trigger":
      return doc.triggers;
    case "label":
      return [doc.label];
    case "folder":
      return [doc.folder];
    case "file":
      return [doc.file];
    case "comment":
      return [doc.comment];
    case "body":
      return [doc.body];
    default:
      return [...doc.triggers, doc.label, doc.comment, doc.body];
  }
}

/** True when every term is found (case-insensitive substring) in its field. */
export function matchesTerms(doc: SearchDoc, terms: Term[]): boolean {
  return terms.every((t) => fieldText(doc, t.field).some((s) => s.toLowerCase().includes(t.value)));
}
