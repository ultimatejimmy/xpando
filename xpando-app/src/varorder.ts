/**
 * Pure helpers for a phrase's variables: the order they are applied in, and keeping the
 * `{{name}}` placeholders in the text in step with them. No dependencies, so they run under
 * Node for the tests (`node --test --experimental-strip-types src/varorder.test.ts`).
 *
 * Why order matters: espanso evaluates a phrase's variables top to bottom, and a variable can
 * use `{{earlier}}` ones in its parameters (an `echo` of `{{date}}_{{clipboard}}`), so a
 * variable has to come AFTER the ones it uses.
 */

/** espanso's variable syntax: {{name}} or {{name.field}}, spaces allowed inside the braces. */
const PLACEHOLDER = /\{\{\s*(\w+)(?:\.\w+)?\s*\}\}/g;

/** The variable names used by the placeholders in `s`, in order, with repeats. */
export function placeholders(s: string): string[] {
  return [...s.matchAll(PLACEHOLDER)].map((m) => m[1]);
}

/** Replaces the variable name in every placeholder of `s`, keeping spaces and `.field`. */
export function renameInText(s: string, from: string, to: string): string {
  return s.replace(/\{\{(\s*)(\w+)((?:\.\w+)?\s*\}\})/g, (all, a: string, name: string, rest: string) =>
    name === from ? `{{${a}${to}${rest}` : all,
  );
}

// ---------------------------------------------------------------------------------------
// Order
// ---------------------------------------------------------------------------------------

export interface Ordered<T> {
  /** The items in an order where each comes after the ones it uses. */
  items: T[];
  /** Plain-language problems: an item that uses itself, or items that use each other. */
  problems: string[];
}

function quoted(names: string[]): string {
  const q = names.map((n) => `“${n}”`);
  return q.length <= 1 ? q.join("") : `${q.slice(0, -1).join(", ")} and ${q[q.length - 1]}`;
}

/**
 * Stable dependency sort. `usesOf(item)` lists the names the item's parameters use; names
 * that are not items (global variables, typos) are ignored. An item moves only as far as it
 * must, to just after the last item it uses; everything else keeps its relative order.
 *
 * Items that use themselves or each other cannot be ordered: they stay in their original
 * relative order after everything else that could be placed, and `problems` names them.
 */
export function sortByDependency<T extends { name: string }>(items: T[], usesOf: (item: T) => string[]): Ordered<T> {
  const n = items.length;
  const indexOfName = new Map<string, number>();
  items.forEach((it, i) => {
    if (!indexOfName.has(it.name)) indexOfName.set(it.name, i);
  });
  const problems: string[] = [];
  const deps: Set<number>[] = items.map((it, i) => {
    const set = new Set<number>();
    for (const name of usesOf(it)) {
      const j = indexOfName.get(name);
      if (j === undefined) continue;
      if (j === i) {
        const msg = `“${it.name}” uses itself, which can't work. Remove {{${it.name}}} from its own settings.`;
        if (!problems.includes(msg)) problems.push(msg);
      } else {
        set.add(j);
      }
    }
    return set;
  });

  const placed: boolean[] = new Array<boolean>(n).fill(false);
  const out: T[] = [];
  for (;;) {
    let pick = -1;
    for (let i = 0; i < n; i++) {
      if (placed[i]) continue;
      if ([...deps[i]].every((d) => placed[d])) {
        pick = i;
        break;
      }
    }
    if (pick < 0) break;
    placed[pick] = true;
    out.push(items[pick]);
  }

  const stuck = [];
  for (let i = 0; i < n; i++) if (!placed[i]) stuck.push(i);
  if (stuck.length > 0) {
    // Which of the stuck items really sit on a loop (the others only wait for one)?
    const reach = (from: number): Set<number> => {
      const seen = new Set<number>();
      const todo = [from];
      while (todo.length > 0) {
        const cur = todo.pop() as number;
        for (const d of deps[cur]) {
          if (!seen.has(d)) {
            seen.add(d);
            todo.push(d);
          }
        }
      }
      return seen;
    };
    const reachOf = new Map<number, Set<number>>(stuck.map((i) => [i, reach(i)]));
    const grouped = new Set<number>();
    for (const i of stuck) {
      if (grouped.has(i) || !(reachOf.get(i) as Set<number>).has(i)) continue;
      const group = stuck.filter((j) => j === i || ((reachOf.get(i) as Set<number>).has(j) && (reachOf.get(j) as Set<number>).has(i)));
      group.forEach((j) => grouped.add(j));
      const names = group.map((j) => items[j].name);
      problems.push(
        names.length === 2
          ? `${quoted(names)} use each other, so neither can come first. Remove one of the two references.`
          : `${quoted(names)} use each other in a circle, so none can come first. Break the circle.`,
      );
    }
    for (const i of stuck) out.push(items[i]);
  }
  return { items: out, problems };
}

/** Names of the items (other than `name` itself) whose parameters use the variable `name`. */
export function usedBy<T extends { name: string }>(items: T[], name: string, usesOf: (item: T) => string[]): string[] {
  return items.filter((it) => it.name !== name && usesOf(it).includes(name)).map((it) => it.name);
}

// ---------------------------------------------------------------------------------------
// Removing placeholders from the text
// ---------------------------------------------------------------------------------------

export interface Removal {
  text: string;
  /** How many placeholders were taken out. */
  count: number;
}

const isBlank = (c: string | undefined): boolean => c === " " || c === "\t";

/** Tags that sit inside a line of text: the space around them still belongs to the text. */
const INLINE_TAGS = new Set(["a", "b", "strong", "i", "em", "u", "s", "strike", "del", "ins", "code", "span", "mark", "sub", "sup"]);

interface Neighbour {
  /** The character, "\n" when a block tag (a paragraph edge, a line break) is in the way, undefined at the very end. */
  ch: string | undefined;
  /** Where that character is, or -1 when there is none to delete. */
  at: number;
}

/** What `s` shows just before position `i`, looking through inline tags without touching them. */
function visibleBefore(s: string, i: number, html: boolean): Neighbour {
  let p = i - 1;
  for (;;) {
    if (p < 0) return { ch: undefined, at: -1 };
    if (html && s[p] === ">") {
      const open = s.lastIndexOf("<", p);
      const tag = open < 0 ? null : /^<\/?([a-z][a-z0-9]*)/i.exec(s.slice(open, p + 1));
      if (tag) {
        if (!INLINE_TAGS.has(tag[1].toLowerCase())) return { ch: "\n", at: -1 };
        p = open - 1;
        continue;
      }
    }
    return { ch: s[p], at: p };
  }
}

/** What `s` shows from position `i` on, looking through inline tags without touching them. */
function visibleAfter(s: string, i: number, html: boolean): Neighbour {
  let p = i;
  for (;;) {
    if (p >= s.length) return { ch: undefined, at: -1 };
    if (html && s[p] === "<") {
      const close = s.indexOf(">", p);
      const tag = close < 0 ? null : /^<\/?([a-z][a-z0-9]*)/i.exec(s.slice(p, close + 1));
      if (tag) {
        if (!INLINE_TAGS.has(tag[1].toLowerCase())) return { ch: "\n", at: -1 };
        p = close + 1;
        continue;
      }
    }
    return { ch: s[p], at: p };
  }
}

/**
 * Removes every `{{name}}`, `{{ name }}` and `{{name.field}}` from `text`, and one adjacent
 * space so no double space (or a space before a comma or at the end of a line) is left.
 *
 * With `html`, `text` is an HTML body: placeholders are plain text inside it. One that sits
 * inside a tag (an attribute) is left alone so no markup is damaged. Tags are never changed:
 * the space tidying looks through inline tags (`Dear <b>{{x}}</b>, y` loses the space before
 * the tag) and treats block tags such as `<p>` and `<br>` as the start or end of a line.
 */
export function removePlaceholders(text: string, name: string, html = false): Removal {
  if (!/^\w+$/.test(name)) return { text, count: 0 };
  const re = new RegExp(`\\{\\{\\s*${name}(?:\\.\\w+)?\\s*\\}\\}`);
  let out = text;
  let count = 0;
  let from = 0;
  for (;;) {
    const m = re.exec(out.slice(from));
    if (!m) break;
    const start = from + m.index;
    const end = start + m[0].length;
    if (html && out.lastIndexOf("<", start) > out.lastIndexOf(">", start)) {
      from = end;
      continue;
    }
    const before = visibleBefore(out, start, html);
    const after = visibleAfter(out, end, html);
    const endsRun =
      after.ch === undefined || after.ch === "\n" || after.ch === "\r" || /[,.;:!?)\]]/.test(after.ch);
    const startsRun = before.ch === undefined || before.ch === "\n" || before.ch === "\r" || before.ch === "(";
    let next = start;
    if (isBlank(before.ch) && (isBlank(after.ch) || endsRun)) {
      out = out.slice(0, before.at) + out.slice(before.at + 1, start) + out.slice(end);
      next = start - 1;
    } else if (isBlank(after.ch) && startsRun) {
      out = out.slice(0, start) + out.slice(end, after.at) + out.slice(after.at + 1);
    } else {
      out = out.slice(0, start) + out.slice(end);
    }
    from = next;
    count++;
  }
  return { text: out, count };
}
