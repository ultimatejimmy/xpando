import { createContext, useCallback, useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Braces,
  Calendar,
  ChevronDown,
  ChevronRight,
  Clipboard,
  ClipboardCopy,
  ClipboardList,
  FileCode2,
  Info,
  ListChecks,
  MessageSquare,
  Plus,
  Shuffle,
  Terminal,
  TextCursorInput,
  Trash2,
  Undo2,
  X,
} from "lucide-react";
import { useAdvanced } from "./AdvancedPanels";
import type { RichHandle } from "./richtextUtil";
import { placeholders, removePlaceholders, renameInText, sortByDependency, usedBy } from "./varorder";
import "./vars.css";

// ---------------------------------------------------------------------------------------
// Model (mirrors xpando_core::vars; the backend validates again and is authoritative)
// ---------------------------------------------------------------------------------------

export type ParamValue = { t: "text"; v: string } | { t: "list"; v: string[] } | { t: "raw"; v: string };

export interface VarParam {
  key: string;
  value: ParamValue;
}

/** A variable as exchanged with the backend. `source` is its position in the file now. */
export interface VarSpec {
  name: string;
  type: string;
  params: VarParam[];
  source: number | null;
}

/** A variable in the editor; `key` identifies the card across edits and reordering. */
interface Item extends VarSpec {
  key: number;
}

const KNOWN_TYPES = ["date", "clipboard", "echo", "random", "choice", "shell", "script", "form", "set_clipboard"] as const;
const TYPE_LABELS: Record<string, string> = {
  date: "Date and time",
  clipboard: "Clipboard",
  echo: "Fixed text",
  random: "Random pick from a list",
  choice: "Pick from a list when expanding",
  shell: "Output of a command",
  script: "Output of a program",
  form: "Form (asks several questions)",
  set_clipboard: "Set the clipboard",
};

/** Whether to show the file's own names (YAML keys, type names) next to the plain labels. */
const AdvancedContext = createContext(false);

/** The name used in the file, shown small next to a plain-language label in Advanced mode. */
function KeyTag({ name }: { name: string }) {
  const advanced = useContext(AdvancedContext);
  return advanced ? <code className="var-key">{name}</code> : null;
}
/** Advanced mode only: what a macro is called in the file, next to the plain word. */
export function VarsAka() {
  const [advanced] = useAdvanced();
  return advanced ? (
    <span className="var-aka">
      {" "}
      (<code>vars:</code> in the YAML)
    </span>
  ) : null;
}

/** The plain-language name of a macro's type, for lists and summaries. */
export function macroLabel(type: string): string {
  return TYPE_LABELS[type] ?? (type || "Unknown macro");
}

const SHELLS = ["bash", "cmd", "powershell", "pwsh", "sh", "zsh", "fish", "nu", "wsl", "wsl2"];
const NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

function isKnown(type: string): boolean {
  return (KNOWN_TYPES as readonly string[]).includes(type);
}

function getParam(item: VarSpec, key: string): ParamValue | undefined {
  return item.params.find((p) => p.key === key)?.value;
}

function textOf(item: VarSpec, key: string): string {
  const v = getParam(item, key);
  if (!v) return "";
  return v.t === "list" ? v.v.join("\n") : v.v;
}

function listOf(item: VarSpec, key: string): string[] {
  const v = getParam(item, key);
  if (!v) return [];
  if (v.t === "list") return v.v;
  return v.t === "text" && v.v !== "" ? [v.v] : [];
}

/** Sets or (with `null`) removes a parameter, keeping the others in order. */
function withParam(item: Item, key: string, value: ParamValue | null): Item {
  const has = item.params.some((p) => p.key === key);
  let params: VarParam[];
  if (value === null) params = item.params.filter((p) => p.key !== key);
  else if (has) params = item.params.map((p) => (p.key === key ? { key, value } : p));
  else params = [...item.params, { key, value }];
  return { ...item, params };
}

const text = (v: string): ParamValue => ({ t: "text", v });

/** Keys the type-specific form handles; everything else is shown as a generic parameter. */
const HANDLED: Record<string, string[]> = {
  date: ["format", "offset", "locale", "tz"],
  clipboard: [],
  echo: ["echo"],
  random: ["choices"],
  choice: ["values"],
  shell: ["cmd", "shell", "trim", "debug"],
  script: ["args", "trim", "debug", "ignore_error"],
  form: ["layout", "fields"],
  set_clipboard: ["text"],
};

// ---------------------------------------------------------------------------------------
// Placeholders, validation, rename (mirrors of the Rust helpers)
// ---------------------------------------------------------------------------------------

export { placeholders, renameInText };

function renameInValue(v: ParamValue, from: string, to: string): ParamValue {
  if (v.t === "text") return { t: "text", v: renameInText(v.v, from, to) };
  if (v.t === "raw") return { t: "raw", v: renameInText(v.v, from, to) };
  return { t: "list", v: v.v.map((x) => renameInText(x, from, to)) };
}

function paramStrings(v: ParamValue): string[] {
  return v.t === "list" ? v.v : [v.v];
}

/** The names of other variables a variable's parameters use, e.g. `{{date}}` in an echo. */
function usesOf(s: VarSpec): string[] {
  return s.params.flatMap((p) => paramStrings(p.value).flatMap((t) => placeholders(t)));
}

/** Variables in the order espanso needs: each after the ones it uses (see varorder.ts). */
function orderSpecs<T extends VarSpec>(list: T[]) {
  return sortByDependency(list, usesOf);
}

/** Per-variable error text (null when fine), same rules as the backend. */
export function validateItems(items: VarSpec[]): (string | null)[] {
  return items.map((s, i) => {
    if (s.name === "") return "Give the macro a name.";
    if (!NAME_RE.test(s.name))
      return "Names can use letters, digits and _, and can't start with a digit.";
    if (items.some((o, j) => j !== i && o.name === s.name)) return "Another macro already has this name.";
    if (!/^[A-Za-z0-9_-]+$/.test(s.type)) return "The type needs letters, digits, - or _ only.";
    const keys = s.params.map((p) => p.key);
    if (keys.some((k) => !/^[A-Za-z0-9_-]+$/.test(k))) return "Every setting needs a name (letters, digits, - or _).";
    if (new Set(keys).size !== keys.length) return "A setting name is used twice.";
    const blank = (k: string) => textOf(s, k).trim() === "";
    const noList = (k: string) => listOf(s, k).every((x) => x.trim() === "") && getParam(s, k)?.t !== "raw";
    switch (s.type) {
      case "echo":
        return getParam(s, "echo") === undefined ? "Enter the text for this macro." : null;
      case "shell":
        return blank("cmd") ? "Enter the command to run." : null;
      case "random":
        return noList("choices") ? "Add at least one option." : null;
      case "choice":
        return noList("values") ? "Add at least one option." : null;
      case "script":
        return noList("args") ? "Enter the program to run." : null;
      case "form":
        return blank("layout") ? "Enter the form layout." : null;
      default:
        return null;
    }
  });
}

export function analyzeItems(items: VarSpec[], body: string): string[] {
  const out: string[] = [];
  const inBody = placeholders(body);
  const seen = new Set<string>();
  for (const n of inBody) {
    if (!items.some((s) => s.name === n) && !seen.has(n)) {
      seen.add(n);
      out.push(
        `The text uses {{${n}}}, but this phrase has no macro with that name. That only works if a global macro provides it.`,
      );
    }
  }
  // A macro nothing uses is not listed here: its own card already says so, with a Remove button.
  return out;
}

// ---------------------------------------------------------------------------------------
// Date preview (strftime subset, as espanso/chrono writes it)
// ---------------------------------------------------------------------------------------

export const DATE_TOKENS: [string, string][] = [
  ["%Y", "year, 2026"],
  ["%y", "year, 26"],
  ["%m", "month, 01-12"],
  ["%d", "day, 01-31"],
  ["%e", "day, space padded"],
  ["%H", "hour, 00-23"],
  ["%I", "hour, 01-12"],
  ["%M", "minute"],
  ["%S", "second"],
  ["%p", "AM or PM"],
  ["%B", "month name"],
  ["%b", "month, short"],
  ["%A", "weekday name"],
  ["%a", "weekday, short"],
  ["%j", "day of year"],
  ["%F", "same as %Y-%m-%d"],
  ["%T", "same as %H:%M:%S"],
  ["%%", "a percent sign"],
];

function pad(n: number, w = 2, c = "0"): string {
  return String(n).padStart(w, c);
}

export function formatDate(format: string, d: Date, locale?: string): string {
  const loc = locale && locale.trim() !== "" ? locale.trim() : undefined;
  const intl = (opts: Intl.DateTimeFormatOptions): string => {
    try {
      return new Intl.DateTimeFormat(loc, opts).format(d);
    } catch {
      return new Intl.DateTimeFormat(undefined, opts).format(d);
    }
  };
  const doy = Math.floor((Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) - Date.UTC(d.getFullYear(), 0, 0)) / 86400000);
  const f = (c: string): string | null => {
    switch (c) {
      case "Y": return String(d.getFullYear());
      case "y": return pad(d.getFullYear() % 100);
      case "m": return pad(d.getMonth() + 1);
      case "d": return pad(d.getDate());
      case "e": return pad(d.getDate(), 2, " ");
      case "H": return pad(d.getHours());
      case "I": return pad(d.getHours() % 12 === 0 ? 12 : d.getHours() % 12);
      case "M": return pad(d.getMinutes());
      case "S": return pad(d.getSeconds());
      case "p": return d.getHours() < 12 ? "AM" : "PM";
      case "B": return intl({ month: "long" });
      case "b":
      case "h": return intl({ month: "short" });
      case "A": return intl({ weekday: "long" });
      case "a": return intl({ weekday: "short" });
      case "j": return pad(doy, 3);
      case "F": return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      case "T": return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
      case "R": return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
      case "D": return `${pad(d.getMonth() + 1)}/${pad(d.getDate())}/${pad(d.getFullYear() % 100)}`;
      case "c": return intl({ dateStyle: "medium", timeStyle: "medium" });
      case "x": return intl({ dateStyle: "short" });
      case "X": return intl({ timeStyle: "medium" });
      case "s": return String(Math.floor(d.getTime() / 1000));
      case "%": return "%";
      default: return null;
    }
  };
  let out = "";
  for (let i = 0; i < format.length; i++) {
    if (format[i] === "%" && i + 1 < format.length) {
      const r = f(format[i + 1]);
      if (r === null) out += format.slice(i, i + 2);
      else out += r;
      i++;
    } else {
      out += format[i];
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Macros
// ---------------------------------------------------------------------------------------

interface MacroDef {
  id: string;
  label: string;
  hint: string;
  icon: ReactNode;
  /** Variable to create; null for text that needs none. */
  make: ((name: string) => VarSpec) | null;
  baseName: string;
  /** Offered only in Advanced mode: technical, or needed only for building other macros. */
  advanced?: boolean;
}

const spec = (name: string, type: string, params: VarParam[]): VarSpec => ({ name, type, params, source: null });

const MACROS: MacroDef[] = [
  {
    id: "date",
    label: "Date",
    hint: "Today's date or time",
    icon: <Calendar size={16} aria-hidden="true" />,
    baseName: "date",
    make: (n) => spec(n, "date", [{ key: "format", value: text("%Y-%m-%d") }]),
  },
  {
    id: "clipboard",
    label: "Clipboard",
    hint: "What you copied last",
    icon: <Clipboard size={16} aria-hidden="true" />,
    baseName: "clipboard",
    make: (n) => spec(n, "clipboard", []),
  },
  {
    id: "shell",
    advanced: true,
    label: "Command output",
    hint: "Output of a command",
    icon: <Terminal size={16} aria-hidden="true" />,
    baseName: "output",
    make: (n) => spec(n, "shell", [{ key: "cmd", value: text("") }]),
  },
  {
    id: "random",
    label: "Random pick",
    hint: "One of several, at random",
    icon: <Shuffle size={16} aria-hidden="true" />,
    baseName: "random",
    make: (n) => spec(n, "random", [{ key: "choices", value: { t: "list", v: ["Option 1", "Option 2"] } }]),
  },
  {
    id: "choice",
    label: "Choice list",
    hint: "You pick when it expands",
    icon: <ListChecks size={16} aria-hidden="true" />,
    baseName: "choice",
    make: (n) => spec(n, "choice", [{ key: "values", value: { t: "list", v: ["Option 1", "Option 2"] } }]),
  },
  {
    id: "echo",
    advanced: true,
    label: "Fixed text",
    hint: "Text to reuse in other macros",
    icon: <MessageSquare size={16} aria-hidden="true" />,
    baseName: "text",
    make: (n) => spec(n, "echo", [{ key: "echo", value: text("") }]),
  },
  {
    // Needs xpando's engine build: stock espanso rejects this variable type.
    id: "set_clipboard",
    advanced: true,
    label: "Set clipboard",
    hint: "Fills the clipboard",
    icon: <ClipboardCopy size={16} aria-hidden="true" />,
    baseName: "setclip",
    make: (n) => spec(n, "set_clipboard", [{ key: "text", value: text("") }]),
  },
  {
    id: "script",
    advanced: true,
    label: "Program output",
    hint: "Output of a program",
    icon: <FileCode2 size={16} aria-hidden="true" />,
    baseName: "script",
    make: (n) => spec(n, "script", [{ key: "args", value: { t: "list", v: [""] } }]),
  },
  {
    id: "form",
    advanced: true,
    label: "Form",
    hint: "Asks several questions",
    icon: <ClipboardList size={16} aria-hidden="true" />,
    baseName: "form",
    make: (n) => spec(n, "form", [{ key: "layout", value: text("") }]),
  },
  {
    id: "cursor",
    label: "Cursor position",
    hint: "Where the cursor lands",
    icon: <TextCursorInput size={16} aria-hidden="true" />,
    baseName: "",
    make: null,
  },
];

function uniqueName(base: string, items: VarSpec[]): string {
  const taken = new Set(items.map((s) => s.name));
  if (!taken.has(base)) return base;
  for (let i = 2; ; i++) if (!taken.has(`${base}${i}`)) return `${base}${i}`;
}

// ---------------------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------------------

/** `{{a}}`, `{{a}} and {{b}}`, `{{a}}, {{b}} and {{c}}`. */
function listNames(names: string[]): string {
  const q = names.map((n) => `{{${n}}}`);
  return q.length <= 1 ? q.join("") : `${q.slice(0, -1).join(", ")} and ${q[q.length - 1]}`;
}

/** What applying a name draft did: nothing to apply, renamed, or refused (the box says why). */
type CommitResult = "none" | "applied" | "problem";

/** What Undo takes back: a removed macro (and the text it was cut from) or a changed type. */
type UndoInfo =
  | { kind: "remove"; item: Item; index: number; bodyBefore: string; bodyAfter: string }
  | { kind: "type"; before: Item; afterType: string };

/** A message under the cards, with Undo while it can still restore things exactly. */
interface Notice {
  tone: "info" | "error";
  text: string;
  undo?: UndoInfo;
}

interface UseVarsOptions {
  file: string;
  /** Index of the phrase in its file, or null for a new phrase. */
  index: number | null;
  /** Whether the phrase has variables in the file now (they are loaded then). */
  hasVars: boolean;
  body: string;
  setBody: (body: string) => void;
  /** `id` of the plain text area, used to insert at the cursor in plain mode. */
  bodyId: string;
  /** The rich text editor's handle, used to insert at the cursor in rich mode. */
  richRef?: RefObject<RichHandle | null>;
  /** False when the body is not text xpando edits (markdown, image): macros can't be inserted. */
  bodyEditable: boolean;
  /** The body is HTML (rich text): placeholders are removed from it without touching tags. */
  bodyIsHtml?: boolean;
  /** Called after the body was changed from here, so a rich text editor can show the new text. */
  onBodyReplaced?: () => void;
}

export interface VarsApi {
  /** The Insert macro button and menu; render it above the text box. */
  menu: ReactNode;
  /** The Variables section. */
  section: ReactNode;
  /** The variable list differs from the file. */
  dirty: boolean;
  /** The variables could be loaded and can be saved. */
  editable: boolean;
  /** Errors that block saving. */
  problems: string[];
  /** Soft problems. */
  warnings: string[];
  /** Applies a macro name that is still being typed (for Ctrl+S). */
  commitPendingName: () => "none" | "applied" | "problem";
  /** The complete list to send to the backend; null when not editable. */
  specs: VarSpec[] | null;
}

let nextKey = 1;

/** Whether the Variables section was left open; remembered until the app is closed. */
let sectionOpenMemory = false;

export function useVars(opts: UseVarsOptions): VarsApi {
  const { index, hasVars, body, setBody, bodyId, richRef, bodyEditable, bodyIsHtml = false, onBodyReplaced, file } = opts;
  const [notice, setNotice] = useState<Notice | null>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [original, setOriginal] = useState<VarSpec[]>([]);
  const [state, setState] = useState<"loading" | "ready" | "error">(index !== null && hasVars ? "loading" : "ready");
  const [loadError, setLoadError] = useState("");
  const [announce, setAnnounce] = useState("");
  const [announceAdd, setAnnounceAdd] = useState("");
  const [focusKey, setFocusKey] = useState<number | null>(null);
  const [open, setOpenState] = useState(sectionOpenMemory);
  const sectionId = useId();
  const focusUndoNext = useRef(false);

  useEffect(() => {
    if (index === null || !hasVars) return;
    let cancelled = false;
    invoke<VarSpec[]>("get_phrase_vars", { file, index })
      .then((specs) => {
        if (cancelled) return;
        const loaded = specs.map((s) => ({ ...s, key: nextKey++ }));
        setOriginal(specs);
        setItems(loaded);
        setState("ready");
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(String(e));
        setState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [file, index, hasVars]);

  const editable = state === "ready";
  // `items` is the order the user made them in; what is shown and saved is the order
  // espanso needs (each variable after the ones it uses).
  const ordered = useMemo(() => orderSpecs(items), [items]);
  const shown = ordered.items;
  const specs = useMemo<VarSpec[]>(() => shown.map(({ key: _k, ...s }) => s), [shown]);
  const originalOrdered = useMemo(() => orderSpecs(original).items, [original]);
  const dirty = editable && JSON.stringify(specs) !== JSON.stringify(originalOrdered);
  const errors = useMemo(() => validateItems(specs), [specs]);

  const problems = useMemo(() => {
    if (!editable) return [];
    const own = errors.flatMap((e, i) => (e ? [`Macro “${shown[i].name || i + 1}”: ${e}`] : []));
    return [...own, ...ordered.problems];
  }, [errors, shown, ordered.problems, editable]);

  // A card that moves because of a new reference must not lose the cursor.
  const lastFocus = useRef<HTMLElement | null>(null);
  const orderKey = shown.map((it) => it.key).join(",");
  useLayoutEffect(() => {
    const el = lastFocus.current;
    if (el && el.isConnected && document.activeElement !== el && document.activeElement === document.body) el.focus();
  }, [orderKey]);
  const warnings = useMemo(() => (editable ? analyzeItems(specs, body) : []), [editable, specs, body]);

  const hasProblem = problems.length > 0;
  const hadProblem = useRef(false);
  useEffect(() => {
    // A problem inside the section opens it, so it can be seen and fixed.
    if (hasProblem && !hadProblem.current) setOpenState(true);
    hadProblem.current = hasProblem;
  }, [hasProblem]);

  const toggleOpen = useCallback(() => {
    setOpenState((o) => {
      sectionOpenMemory = !o;
      return !o;
    });
  }, []);

  // Name boxes apply their draft on blur or Enter; Ctrl+S has neither, so it asks them to.
  const committers = useRef(new Map<number, () => CommitResult>());
  const registerCommit = useCallback((key: number, commit: (() => CommitResult) | null) => {
    if (commit) committers.current.set(key, commit);
    else committers.current.delete(key);
  }, []);
  /** Applies a name that is still being typed. "problem" means it was refused and the box says why. */
  const commitPendingName = useCallback((): CommitResult => {
    for (const commit of committers.current.values()) {
      const r = commit();
      if (r !== "none") return r; // only one box can have focus, so at most one draft is pending
    }
    return "none";
  }, []);

  const update = useCallback((key: number, fn: (it: Item) => Item) => {
    setItems((all) => all.map((it) => (it.key === key ? fn(it) : it)));
  }, []);

  /** Changes the body from here; a rich text editor is told to show it. */
  function replaceBody(next: string) {
    setBody(next);
    if (bodyIsHtml) onBodyReplaced?.();
  }

  /**
   * Renames a variable once the user is done typing the name (leaving the field or Enter):
   * the text and the other variables' settings follow. Returns why it can't, or null.
   */
  function renameItem(key: number, name: string): string | null {
    const item = items.find((it) => it.key === key);
    if (!item || name === item.name) return null;
    if (name === "") return "A macro needs a name.";
    if (!NAME_RE.test(name)) {
      return `“${name}” can't be a name: use letters, digits and _, and don't start with a digit.`;
    }
    if (items.some((o) => o.key !== key && o.name === name)) return `Another macro is already called “${name}”.`;
    const prev = item.name;
    const users = usedBy(items, prev, usesOf);
    const inText = bodyEditable && placeholders(body).includes(prev);
    setItems((all) =>
      all.map((it) => ({
        ...it,
        name: it.key === key ? name : it.name,
        params: it.params.map((p) => ({ key: p.key, value: renameInValue(p.value, prev, name) })),
      })),
    );
    if (inText) replaceBody(renameInText(body, prev, name));
    const where = [...(inText ? ["in the text"] : []), ...(users.length > 0 ? [`in ${listNames(users)}`] : [])];
    setNotice({
      tone: "info",
      text: `Renamed {{${prev}}} to {{${name}}}${where.length > 0 ? ` ${where.join(" and ")}` : ""}.`,
    });
    return null;
  }

  function removeItem(key: number) {
    const at = items.findIndex((it) => it.key === key);
    if (at < 0) return;
    const item = items[at];
    const users = usedBy(items, item.name, usesOf);
    if (users.length > 0) {
      setNotice({
        tone: "error",
        text: `{{${item.name}}} is used by ${listNames(users)}; change that first.`,
      });
      return;
    }
    const r = bodyEditable ? removePlaceholders(body, item.name, bodyIsHtml) : { text: body, count: 0 };
    setItems((all) => all.filter((it) => it.key !== key));
    if (r.count > 0) replaceBody(r.text);
    setNotice({
      tone: "info",
      text: r.count > 0 ? `Removed {{${item.name}}} from the text too.` : `Removed the macro “${item.name}”.`,
      undo: { kind: "remove", item, index: at, bodyBefore: body, bodyAfter: r.text },
    });
    // The card (and the focus in it) is gone: move to Undo, which also announces what happened.
    focusUndoNext.current = true;
  }

  /** Changes what a macro fills in. That clears its settings, so the old ones are kept for Undo. */
  function changeType(key: number, type: string) {
    const item = items.find((it) => it.key === key);
    if (!item || item.type === type) return;
    const fresh = MACROS.find((m) => m.id === type)?.make?.(item.name);
    setItems((all) => all.map((it) => (it.key === key ? { ...it, type, params: fresh ? fresh.params : [] } : it)));
    setNotice(
      item.params.length > 0
        ? {
            tone: "info",
            text: `Changed {{${item.name}}} to ${macroLabel(type)}; its old settings were cleared.`,
            undo: { kind: "type", before: item, afterType: type },
          }
        : null,
    );
  }

  function undo() {
    const u = notice?.undo;
    if (!u) return;
    if (u.kind === "type") {
      setItems((all) => all.map((it) => (it.key === u.before.key ? u.before : it)));
      setNotice({ tone: "info", text: `Brought back the settings of {{${u.before.name}}}.` });
      return;
    }
    setItems((all) => {
      const next = [...all];
      next.splice(Math.min(u.index, next.length), 0, u.item);
      return next;
    });
    if (u.bodyBefore !== u.bodyAfter) replaceBody(u.bodyBefore);
    setNotice({ tone: "info", text: `Brought back {{${u.item.name}}}.` });
  }

  // After a removal the card (and the focus in it) is gone: move to Undo, which also reads out what happened.
  useEffect(() => {
    if (focusUndoNext.current && notice?.undo) {
      focusUndoNext.current = false;
      document.getElementById(`${sectionId}-undo`)?.focus();
    }
  }, [notice, sectionId]);

  // Undo only while it can restore things exactly: for a removal the text is as it was left and
  // the name is free; for a type change the macro still has the type it was changed to.
  useEffect(() => {
    const u = notice?.undo;
    if (!u) return;
    if (u.kind === "type") {
      const now = items.find((o) => o.key === u.before.key);
      if (!now || now.type !== u.afterType) setNotice(null);
      return;
    }
    if ((u.bodyBefore !== u.bodyAfter && body !== u.bodyAfter) || items.some((o) => o.name === u.item.name)) {
      setNotice(null);
    }
  }, [notice, body, items]);

  // Variables that nothing uses: not the text, not another variable's settings.
  const unusedKeys = useMemo(() => {
    const inBody = new Set(placeholders(body));
    return new Set(
      shown
        .filter((it) => it.name !== "" && !inBody.has(it.name) && usedBy(shown, it.name, usesOf).length === 0)
        .map((it) => it.key),
    );
  }, [shown, body]);

  const addItem = useCallback((make: (name: string) => VarSpec, base: string): string => {
    const name = uniqueName(base, items);
    const item: Item = { ...make(name), key: nextKey++ };
    setItems([...items, item]);
    setOpenState(true);
    return name;
  }, [items]);

  /** Whether text can be put at the cursor right now (the rich editor may still be loading). */
  function canInsertNow(): boolean {
    return !bodyIsHtml || !!richRef?.current;
  }

  function insertAtCursor(snippet: string) {
    if (bodyIsHtml) {
      // The rich editor keeps focus and the caret after the text, and reports the new HTML.
      richRef?.current?.insertText(snippet);
      return;
    }
    const el = document.getElementById(bodyId) as HTMLTextAreaElement | null;
    const start = el ? el.selectionStart : body.length;
    const end = el ? el.selectionEnd : body.length;
    // Through the browser's own editing path, so Ctrl+Z still undoes it (and earlier typing).
    if (el) {
      el.focus();
      el.setSelectionRange(start, end);
      if (document.execCommand("insertText", false, snippet)) return;
    }
    setBody(body.slice(0, start) + snippet + body.slice(end));
    if (el) {
      const pos = start + snippet.length;
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(pos, pos);
      });
    }
  }

  function insertMacro(m: MacroDef) {
    if (!canInsertNow()) {
      setAnnounce("The text editor is still loading. Try again in a moment.");
      return;
    }
    if (!m.make) {
      if (bodyIsHtml) return; // the cursor position is offered in plain text only
      insertAtCursor("$|$");
      setAnnounce("Inserted the cursor position marker.");
      return;
    }
    const name = addItem(m.make, m.baseName);
    insertAtCursor(`{{${name}}}`);
    setAnnounce(`Inserted {{${name}}} and added a ${m.label} macro named ${name}.`);
  }

  /** The section's Add menu: makes the variable without putting it in the text. */
  function addMacro(m: MacroDef) {
    if (!m.make) return;
    const name = addItem(m.make, m.baseName);
    setFocusKey(nextKey - 1);
    setAnnounceAdd(`Added a ${m.label} macro named ${name}. Open below.`);
  }

  const menu =
    editable && bodyEditable ? (
      <MacroMenu kind="insert" onPick={insertMacro} announce={announce} rich={bodyIsHtml} />
    ) : null;

  const section = (
    <VarsSection
      state={state}
      loadError={loadError}
      items={shown}
      errors={errors}
      onFocusCapture={(el) => (lastFocus.current = el)}
      open={open}
      onToggle={toggleOpen}
      problemCount={problems.length}
      warningCount={warnings.length}
      onUpdate={update}
      onRename={renameItem}
      onRemove={removeItem}
      unusedKeys={unusedKeys}
      notice={notice}
      onUndo={undo}
      onChangeType={changeType}
      registerCommit={registerCommit}
      onDismissNotice={() => setNotice(null)}
      addMenu={<MacroMenu kind="add" onPick={addMacro} announce={announceAdd} />}
      focusKey={focusKey}
      canInsert={menu !== null}
      sectionId={sectionId}
    />
  );

  return { menu, section, dirty, editable, problems, warnings, specs: editable ? specs : null, commitPendingName };
}

// ---------------------------------------------------------------------------------------
// Insert macro menu
// ---------------------------------------------------------------------------------------

function MacroMenu({
  kind,
  onPick,
  announce,
  rich = false,
}: {
  kind: "insert" | "add";
  onPick: (m: MacroDef) => void;
  announce: string;
  /** The text is rich text: the cursor position marker is not offered. */
  rich?: boolean;
}) {
  const label = kind === "insert" ? "Insert macro" : "Add a macro";
  const [advanced] = useAdvanced();
  // Normal mode offers the everyday macros; the technical ones (commands, programs, forms) are Advanced.
  const list = (kind === "insert" ? MACROS : MACROS.filter((m) => m.make)).filter((m) => advanced || !m.advanced);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const uid = useId();

  useEffect(() => {
    if (open) wrap.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [open]);

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (!open) return;
    const items = [...(wrap.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(at + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(at - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Home") {
      e.preventDefault();
      items[0]?.focus();
    } else if (e.key === "End") {
      e.preventDefault();
      items[items.length - 1]?.focus();
    } else if (e.key === "Escape") {
      // Don't let the editor treat this as "cancel".
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      button.current?.focus();
    } else if (e.key === "Tab") {
      setOpen(false);
    }
  }

  return (
    <div
      className="macro-menu"
      ref={wrap}
      onKeyDown={onKeyDown}
      onBlur={(e) => {
        if (!wrap.current?.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <button
        type="button"
        ref={button}
        className="btn"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={`${uid}-menu`}
        title={kind === "insert" ? "Adds a macro and puts it in the text at the cursor" : "Adds a macro without putting it in the text"}
        onClick={() => setOpen((o) => !o)}
      >
        {kind === "insert" ? <Braces size={16} aria-hidden="true" /> : <Plus size={16} aria-hidden="true" />}
        {label}
        <ChevronDown size={16} aria-hidden="true" />
      </button>
      {open && (
        <div className={`macro-list${kind === "add" ? " macro-up" : ""}`} role="menu" id={`${uid}-menu`} aria-label={label}>
          {list.map((m) => {
            // espanso moves the cursor by counting characters in the HTML source, which is wrong
            // once markup is involved, so the cursor position is for plain text only.
            const off = rich && !m.make;
            return (
              <button
                key={m.id}
                type="button"
                role="menuitem"
                className={`macro-item${off ? " macro-off" : ""}`}
                aria-disabled={off || undefined}
                title={off ? "The cursor position does not work in rich text, so it is only offered in plain text." : undefined}
                onClick={() => {
                  if (off) return;
                  setOpen(false);
                  onPick(m);
                }}
              >
                {m.icon}
                <span className="macro-label">{m.label}</span>
                <span className="macro-hint">{off ? "Plain text only" : m.hint}</span>
              </button>
            );
          })}
        </div>
      )}
      <span className="sr-only" role="status" aria-live="polite">
        {announce}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// Section and cards
// ---------------------------------------------------------------------------------------

interface SectionProps {
  state: "loading" | "ready" | "error";
  loadError: string;
  items: Item[];
  errors: (string | null)[];
  open: boolean;
  onToggle: () => void;
  problemCount: number;
  warningCount: number;
  onUpdate: (key: number, fn: (it: Item) => Item) => void;
  onRename: (key: number, name: string) => string | null;
  onRemove: (key: number) => void;
  unusedKeys: Set<number>;
  notice: Notice | null;
  onUndo: () => void;
  onChangeType: (key: number, type: string) => void;
  registerCommit: (key: number, commit: (() => CommitResult) | null) => void;
  onDismissNotice: () => void;
  onFocusCapture: (el: HTMLElement | null) => void;
  /** The "Add a macro" menu. */
  addMenu: ReactNode;
  /** The card of this variable gets the focus when it appears (it was just added). */
  focusKey: number | null;
  /** The Insert macro menu is available above the text. */
  canInsert: boolean;
  /** Prefix for ids the hook needs to find (the Undo button). */
  sectionId: string;
}

/** The notice under the cards, with Undo when a removal can still be taken back. */
function NoticeBar({ p }: { p: SectionProps }) {
  const n = p.notice;
  return (
    <>
      {/* Always-mounted live regions, so the text is announced when it appears. */}
      <span className="sr-only" role="status">
        {n && n.tone === "info" ? `${n.text}${n.undo ? " Undo is available." : ""}` : ""}
      </span>
      <span className="sr-only" role="alert">
        {n && n.tone === "error" ? n.text : ""}
      </span>
      {n && (
        <div className={`var-notice${n.tone === "error" ? " var-notice-error" : ""}`}>
          {n.tone === "error" ? <AlertTriangle size={16} aria-hidden="true" /> : <Info size={16} aria-hidden="true" />}
          <span className="grow">{n.text}</span>
          {n.undo && (
            <button type="button" className="btn" id={`${p.sectionId}-undo`} onClick={p.onUndo}>
              <Undo2 size={16} aria-hidden="true" />
              Undo
            </button>
          )}
          <button
            type="button"
            className="btn btn-icon"
            aria-label="Dismiss this message"
            title="Dismiss"
            onClick={p.onDismissNotice}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      )}
    </>
  );
}

function VarsSection(p: SectionProps) {
  const uid = useId();
  const [advanced] = useAdvanced();
  const count = p.items.length;
  // Adding a macro without putting it in the text is for building macros from macros: Advanced.
  const addRow = advanced ? (
    <div className="var-add">
      {p.addMenu}
      <span className="var-hint">Adds it without putting it in the text.</span>
    </div>
  ) : null;

  if (p.state !== "ready") {
    return (
      <div className="editor-group vars-section" role="group" aria-labelledby={`${uid}-h`}>
        <span className="editor-legend" id={`${uid}-h`}>
          Macros<VarsAka />
        </span>
        {p.state === "loading" && <p className="editor-note">Loading macros…</p>}
        {p.state === "error" && (
          <p className="editor-note" role="alert">
            <AlertTriangle size={16} aria-hidden="true" />
            <span>
              These macros use a layout xpando can't edit safely, so they are left as they are. Edit them in
              Advanced mode or a text editor. ({p.loadError})
            </span>
          </p>
        )}
      </div>
    );
  }

  // No macros: normal mode shows nothing (Insert macro above the text adds one), Advanced one compact row.
  if (count === 0) {
    if (!advanced) return <NoticeBar p={p} />;
    return (
      <div className="editor-group vars-section vars-empty" role="group" aria-labelledby={`${uid}-h`}>
        <span className="editor-legend" id={`${uid}-h`}>
          Macros<VarsAka />
        </span>
        <p className="editor-note">
          <Info size={16} aria-hidden="true" />
          <span>
            None yet. A macro fills in something when the phrase expands, like today's date.
            {p.canInsert && " Use Insert macro above the text to put one in."}
          </span>
        </p>
        <NoticeBar p={p} />
        {addRow}
      </div>
    );
  }

  const issues = p.problemCount + p.warningCount;
  return (
    <AdvancedContext.Provider value={advanced}>
    <div className="editor-group vars-section" role="group" aria-labelledby={`${uid}-h`}>
      <h3 className="vars-heading" id={`${uid}-h`}>
        <button
          type="button"
          className="vars-toggle"
          aria-expanded={p.open}
          aria-controls={`${uid}-body`}
          aria-describedby={p.open ? undefined : `${uid}-sum`}
          onClick={p.onToggle}
        >
          <ChevronRight size={16} aria-hidden="true" className="vars-chevron" />
          Macros ({count})<VarsAka />
        </button>
      </h3>
      {!p.open && (
        <p className="vars-summary" id={`${uid}-sum`}>
          {p.items.map((it) => (
            <span className="chip mono" key={it.key}>
              {it.name || "(no name)"}
            </span>
          ))}
          {issues > 0 && (
            <span className={`vars-badge${p.problemCount > 0 ? " vars-badge-problem" : ""}`}>
              <AlertTriangle size={14} aria-hidden="true" />
              {p.problemCount > 0
                ? `${p.problemCount} ${p.problemCount === 1 ? "problem" : "problems"}`
                : `${p.warningCount} ${p.warningCount === 1 ? "warning" : "warnings"}`}
            </span>
          )}
        </p>
      )}
      <div
        id={`${uid}-body`}
        className="vars-body"
        hidden={!p.open}
        onFocusCapture={(e) => p.onFocusCapture(e.target as HTMLElement)}
      >
        {advanced && (
          <p className="var-hint">
            <Info size={16} aria-hidden="true" />
            Macros are applied in the right order automatically: one that uses another goes after it.
          </p>
        )}
        <ol className="var-cards">
          {p.items.map((it, i) => (
            <VarCard
              key={it.key}
              item={it}
              index={i}
              error={p.errors[i]}
              autoFocus={p.focusKey === it.key}
              onUpdate={p.onUpdate}
              onRename={p.onRename}
              onChangeType={p.onChangeType}
              registerCommit={p.registerCommit}
              onRemove={p.onRemove}
              unused={p.unusedKeys.has(it.key)}
            />
          ))}
        </ol>
        <NoticeBar p={p} />
        {addRow}
      </div>
    </div>
    </AdvancedContext.Provider>
  );
}

/**
 * The name box. The name is applied (and the text and other variables follow) when the user
 * leaves the box or presses Enter, not on every key, so `{{date}}` is never half-renamed.
 */
function NameField({
  id,
  item,
  invalid,
  onRename,
  registerCommit,
}: {
  id: string;
  item: Item;
  invalid: boolean;
  onRename: (key: number, name: string) => string | null;
  registerCommit: (key: number, commit: (() => CommitResult) | null) => void;
}) {
  const [draft, setDraft] = useState(item.name);
  const [why, setWhy] = useState("");
  useEffect(() => setDraft(item.name), [item.name]);
  function commit(): CommitResult {
    const trimmed = draft.trim();
    if (trimmed === item.name) {
      setDraft(item.name);
      setWhy("");
      return "none";
    }
    const problem = onRename(item.key, trimmed);
    if (problem) {
      setWhy(problem);
      setDraft(item.name);
      return "problem";
    }
    setWhy("");
    return "applied";
  }
  // Ctrl+S saves without the box losing focus: the hook asks every name box to apply its draft first.
  const commitRef = useRef(commit);
  commitRef.current = commit;
  useEffect(() => {
    registerCommit(item.key, () => commitRef.current());
    return () => registerCommit(item.key, null);
  }, [item.key, registerCommit]);
  return (
    <>
      <input
        id={id}
        className="field mono"
        value={draft}
        spellCheck={false}
        autoComplete="off"
        aria-invalid={invalid || why !== ""}
        aria-describedby={`${id}-help`}
        onChange={(e) => {
          setDraft(e.target.value);
          setWhy("");
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      <p className={why ? "var-error" : "var-hint"} id={`${id}-help`} role={why ? "alert" : undefined}>
        {why || `In the text: {{${item.name}}}`}
      </p>
    </>
  );
}

function VarCard({
  item,
  index,
  error,
  autoFocus,
  onUpdate,
  onRename,
  onChangeType,
  registerCommit,
  onRemove,
  unused,
}: {
  item: Item;
  index: number;
  error: string | null;
  autoFocus: boolean;
  onUpdate: (key: number, fn: (it: Item) => Item) => void;
  onRename: (key: number, name: string) => string | null;
  onChangeType: (key: number, type: string) => void;
  registerCommit: (key: number, commit: (() => CommitResult) | null) => void;
  onRemove: (key: number) => void;
  unused: boolean;
}) {
  const uid = useId();
  const advanced = useContext(AdvancedContext);
  const known = isKnown(item.type);
  const set = (fn: (it: Item) => Item) => onUpdate(item.key, fn);
  const label = item.name || `macro ${index + 1}`;
  const card = useRef<HTMLLIElement>(null);
  useEffect(() => {
    // A macro that was just added: go to its first setting (or its name), ready to fill in.
    if (!autoFocus || !card.current) return;
    const first =
      card.current.querySelector<HTMLElement>(".var-head ~ .var-field input:not([type=checkbox]), .var-head ~ .var-field textarea") ??
      card.current.querySelector<HTMLElement>(".var-name input");
    first?.focus();
    card.current.scrollIntoView({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <li className="var-card" aria-label={`Macro ${label}`} ref={card}>
      <div className="var-head">
        {!advanced && (
          // Normal mode: what it is and how it shows in the text. Renaming and retyping are Advanced.
          <p className="var-title">
            {MACROS.find((m) => m.id === item.type)?.icon ?? <Braces size={16} aria-hidden="true" />}
            <span className="var-title-label">{macroLabel(item.type)}</span>
            <code className="placeholder">{`{{${item.name}}}`}</code>
          </p>
        )}
        {advanced && (
        <div className="var-field var-name">
          <label htmlFor={`${uid}-name`}>Name</label>
          <NameField
            id={`${uid}-name`}
            item={item}
            invalid={error !== null && /name/i.test(error)}
            onRename={onRename}
            registerCommit={registerCommit}
          />
        </div>
        )}
        {advanced && (
        <div className="var-field var-type">
          <label htmlFor={`${uid}-type`}>What it fills in</label>
          <select
            id={`${uid}-type`}
            className="field"
            value={known ? item.type : "__other"}
            aria-describedby={item.params.length > 0 ? `${uid}-type-help` : undefined}
            onChange={(e) => onChangeType(item.key, e.target.value === "__other" ? "custom" : e.target.value)}
          >
            {KNOWN_TYPES.map((t) => (
              <option key={t} value={t}>
                {advanced ? `${TYPE_LABELS[t]} (${t})` : TYPE_LABELS[t]}
              </option>
            ))}
            {(advanced || !known) && <option value="__other">Other type…</option>}
          </select>
          {item.params.length > 0 && (
            <span className="var-hint" id={`${uid}-type-help`}>
              Changing this clears the settings below (Undo is offered).
            </span>
          )}
        </div>
        )}
        <div className="var-buttons">
          <button
            type="button"
            className="btn btn-icon"
            aria-label={`Remove ${label}`}
            title="Remove macro"
            onClick={() => onRemove(item.key)}
          >
            <Trash2 size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
      {!known && !advanced && (
        <p className="var-hint">
          <Info size={16} aria-hidden="true" />
          xpando doesn't know this kind of macro ({item.type || "no type"}), so its settings are shown as name and value
          pairs.
        </p>
      )}
      {!known && advanced && (
        <div className="var-field">
          <label htmlFor={`${uid}-custom`}>Type name</label>
          <input
            id={`${uid}-custom`}
            className="field mono"
            value={item.type}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => set((it) => ({ ...it, type: e.target.value }))}
          />
          <p className="editor-note">
            <Info size={16} aria-hidden="true" />
            xpando doesn't know this type, so its settings are shown as plain name and value pairs.
          </p>
        </div>
      )}
      {unused && (
        <p className="var-unused">
          <Info size={16} aria-hidden="true" />
          <span>Not in the text yet.</span>
          <button type="button" className="btn" onClick={() => onRemove(item.key)}>
            <Trash2 size={16} aria-hidden="true" />
            Remove it
          </button>
        </p>
      )}
      <TypeFields item={item} set={set} uid={uid} />
      <ExtraParams item={item} set={set} uid={uid} />
      {error && (
        <p className="var-error" role="alert">
          <AlertTriangle size={16} aria-hidden="true" />
          {error}
        </p>
      )}
    </li>
  );
}

type SetItem = (fn: (it: Item) => Item) => void;

function TextField({
  id,
  label,
  value,
  onChange,
  mono = true,
  multiline = false,
  rows = 3,
  placeholder,
  yamlKey,
  hint,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  mono?: boolean;
  multiline?: boolean;
  rows?: number;
  placeholder?: string;
  yamlKey?: string;
  hint?: ReactNode;
}) {
  const cls = `field${mono ? " mono" : ""}`;
  return (
    <div className="var-field">
      <label htmlFor={id}>
        {label}
        {yamlKey && <KeyTag name={yamlKey} />}
      </label>
      {multiline ? (
        <textarea
          id={id}
          className={cls}
          rows={rows}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <input
          id={id}
          className={cls}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => onChange(e.target.value)}
        />
      )}
      {hint && <p className="var-hint">{hint}</p>}
    </div>
  );
}

function CheckField({
  id,
  label,
  checked,
  yamlKey,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  yamlKey?: string;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="var-check">
      <input id={id} type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <label htmlFor={id}>
        {label}
        {yamlKey && <KeyTag name={yamlKey} />}
      </label>
    </div>
  );
}

function ListField({
  uid,
  label,
  values,
  onChange,
  addLabel,
  yamlKey,
  mono = false,
}: {
  uid: string;
  label: string;
  values: string[];
  onChange: (v: string[]) => void;
  addLabel: string;
  yamlKey?: string;
  mono?: boolean;
}) {
  return (
    <div className="var-field" role="group" aria-label={label}>
      <span className="var-label">
        {label}
        {yamlKey && <KeyTag name={yamlKey} />}
      </span>
      {values.map((v, i) => (
        <div className="trigger-row" key={i}>
          <input
            id={`${uid}-${i}`}
            className={`field${mono ? " mono" : ""}`}
            aria-label={`${label} ${i + 1}`}
            value={v}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => onChange(values.map((x, j) => (j === i ? e.target.value : x)))}
          />
          <button
            type="button"
            className="btn btn-icon"
            aria-label={`Remove ${label} ${i + 1}`}
            title="Remove"
            onClick={() => onChange(values.filter((_, j) => j !== i))}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      ))}
      <div>
        <button type="button" className="btn" onClick={() => onChange([...values, ""])}>
          <Plus size={16} aria-hidden="true" />
          {addLabel}
        </button>
      </div>
    </div>
  );
}

function RawNote({ what }: { what: string }) {
  return (
    <p className="var-hint">
      <Info size={16} aria-hidden="true" />
      {what} Edit it as YAML.
    </p>
  );
}

/** A list parameter that falls back to a YAML text box when the file holds objects. */
function ListOrRaw({
  item,
  set,
  uid,
  paramKey,
  label,
  addLabel,
  rawNote,
}: {
  item: Item;
  set: SetItem;
  uid: string;
  paramKey: string;
  label: string;
  addLabel: string;
  rawNote: string;
}) {
  const v = getParam(item, paramKey);
  if (v?.t === "raw") {
    return (
      <TextField
        id={`${uid}-${paramKey}`}
        label={label}
        yamlKey={paramKey}
        multiline
        rows={6}
        value={v.v}
        onChange={(t) => set((it) => withParam(it, paramKey, { t: "raw", v: t }))}
        hint={<RawNote what={rawNote} />}
      />
    );
  }
  // A single text value (espanso allows one value per line) is shown as one row per line.
  const values = v?.t === "text" ? v.v.split("\n").filter((x) => x !== "") : listOf(item, paramKey);
  return (
    <ListField
      uid={`${uid}-${paramKey}`}
      label={label}
      yamlKey={paramKey}
      values={values}
      addLabel={addLabel}
      onChange={(l) => set((it) => withParam(it, paramKey, { t: "list", v: l }))}
    />
  );
}

/** Common date formats, offered as one-click examples. */
const DATE_EXAMPLES = ["%Y-%m-%d", "%m/%d/%Y", "%d.%m.%Y", "%B %d, %Y", "%A", "%H:%M", "%Y-%m-%d %H:%M"];

/**
 * Settings that are rarely changed, behind one "More settings" disclosure. Closed in normal
 * mode; Advanced mode shows them open, since showing everything is what it is for.
 */
function MoreSettings({ children }: { children: ReactNode }) {
  const advanced = useContext(AdvancedContext);
  const [open, setOpen] = useState(advanced);
  useEffect(() => {
    if (advanced) setOpen(true);
  }, [advanced]);
  return (
    <details className="var-more" open={open} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>More settings</summary>
      <div className="var-more-body">{children}</div>
    </details>
  );
}

function DateFields({ item, set, uid }: { item: Item; set: SetItem; uid: string }) {
  const format = textOf(item, "format");
  const offset = textOf(item, "offset");
  const locale = textOf(item, "locale");
  const [now] = useState(() => new Date());
  const preview = useMemo(() => {
    const secs = /^-?\d+$/.test(offset.trim()) ? Number(offset) : 0;
    const when = new Date(now.getTime() + secs * 1000);
    return formatDate(format === "" ? "%c" : format, when, locale);
  }, [format, offset, locale, now]);
  const opt = (key: string, v: string) => set((it) => withParam(it, key, v === "" ? null : text(v)));
  return (
    <>
      <TextField
        id={`${uid}-format`}
        label="How the date is written"
        yamlKey="format"
        value={format}
        placeholder="%Y-%m-%d"
        onChange={(v) => opt("format", v)}
        hint={
          <>
            Looks like: <output className="date-preview">{preview || "(empty)"}</output>
          </>
        }
      />
      <div className="var-field" role="group" aria-label="Date format examples">
        <span className="var-label">Or pick an example</span>
        <div className="date-examples">
          {DATE_EXAMPLES.map((f) => {
            const example = formatDate(f, now, locale);
            return (
              <button
                type="button"
                key={f}
                className={`btn date-example${format === f ? " is-on" : ""}`}
                aria-pressed={format === f}
                title={`Uses the format ${f}`}
                onClick={() => opt("format", f)}
              >
                {example}
              </button>
            );
          })}
        </div>
      </div>
      <MoreSettings>
      <details className="var-help">
        <summary>Build your own: what the format codes mean</summary>
        <dl className="token-list">
          {DATE_TOKENS.map(([tok, what]) => (
            <div key={tok}>
              <dt>
                <code>{tok}</code>
              </dt>
              <dd>{what}</dd>
            </div>
          ))}
        </dl>
      </details>
      <div className="var-row">
        <TextField
          id={`${uid}-offset`}
          label="Shift by (seconds)"
          yamlKey="offset"
          value={offset}
          placeholder="0"
          onChange={(v) => opt("offset", v)}
          hint="For example 86400 is tomorrow, -3600 is an hour ago."
        />
        <TextField
          id={`${uid}-locale`}
          label="Language of month and day names"
          yamlKey="locale"
          value={locale}
          placeholder="en-US"
          onChange={(v) => opt("locale", v)}
          hint="Leave empty for English, or for example de-DE."
        />
        <TextField
          id={`${uid}-tz`}
          label="Time zone"
          yamlKey="tz"
          value={textOf(item, "tz")}
          placeholder="Europe/Berlin"
          onChange={(v) => opt("tz", v)}
          hint="Leave empty for this computer's time zone."
        />
      </div>
      <p className="var-hint">The preview is approximate; espanso writes the final result.</p>
      </MoreSettings>
    </>
  );
}

function ShellOptions({ item, set, uid, ignoreError }: { item: Item; set: SetItem; uid: string; ignoreError: boolean }) {
  const flag = (key: string) => textOf(item, key) === "true";
  return (
    <div className="var-checks">
      <CheckField
        id={`${uid}-trim`}
        label="Trim spaces and line breaks around the output"
        yamlKey="trim"
        checked={textOf(item, "trim") !== "false"}
        onChange={(c) => set((it) => withParam(it, "trim", c ? null : text("false")))}
      />
      <CheckField
        id={`${uid}-debug`}
        label="Write details to the log (for troubleshooting)"
        yamlKey="debug"
        checked={flag("debug")}
        onChange={(c) => set((it) => withParam(it, "debug", c ? text("true") : null))}
      />
      {ignoreError && (
        <CheckField
          id={`${uid}-ignore`}
          label="Carry on if the program fails"
          yamlKey="ignore_error"
          checked={flag("ignore_error")}
          onChange={(c) => set((it) => withParam(it, "ignore_error", c ? text("true") : null))}
        />
      )}
    </div>
  );
}

function TypeFields({ item, set, uid }: { item: Item; set: SetItem; uid: string }) {
  const opt = (key: string, v: string) => set((it) => withParam(it, key, v === "" ? null : text(v)));
  switch (item.type) {
    case "date":
      return <DateFields item={item} set={set} uid={uid} />;
    case "clipboard":
      return (
        <p className="var-hint">
          <Info size={16} aria-hidden="true" />
          Inserts whatever you last copied. Nothing to set.
        </p>
      );
    case "echo":
      return (
        <TextField
          id={`${uid}-echo`}
          label="Text to insert"
          yamlKey="echo"
          multiline
          mono={false}
          value={textOf(item, "echo")}
          onChange={(v) => set((it) => withParam(it, "echo", text(v)))}
          hint={<>Can use other macros, for example {"{{date}}"}_{"{{clipboard}}"}.</>}
        />
      );
    case "random":
      return (
        <ListOrRaw
          item={item}
          set={set}
          uid={uid}
          paramKey="choices"
          label="Option"
          addLabel="Add option"
          rawNote="This list holds more than plain text."
        />
      );
    case "choice":
      return (
        <ListOrRaw
          item={item}
          set={set}
          uid={uid}
          paramKey="values"
          label="Option"
          addLabel="Add option"
          rawNote="These values have an id and a label."
        />
      );
    case "shell": {
      const shell = textOf(item, "shell");
      return (
        <>
          <TextField
            id={`${uid}-cmd`}
            label="Command to run"
            yamlKey="cmd"
            multiline
            rows={3}
            value={textOf(item, "cmd")}
            onChange={(v) => set((it) => withParam(it, "cmd", text(v)))}
            hint="Its output replaces the macro. Can use other macros."
          />
          <MoreSettings>
          <div className="var-field">
            <label htmlFor={`${uid}-shell`}>
              Run it with
              <KeyTag name="shell" />
            </label>
            <select id={`${uid}-shell`} className="field" value={shell} onChange={(e) => opt("shell", e.target.value)}>
              <option value="">Default for this system</option>
              {SHELLS.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
              {shell !== "" && !SHELLS.includes(shell) && <option value={shell}>{shell}</option>}
            </select>
          </div>
          <ShellOptions item={item} set={set} uid={uid} ignoreError={false} />
          </MoreSettings>
        </>
      );
    }
    case "script": {
      const args = listOf(item, "args");
      return (
        <>
          <ListField
            uid={`${uid}-args`}
            label="Program or argument"
            yamlKey="args"
            values={args}
            addLabel="Add an argument"
            mono
            onChange={(l) => set((it) => withParam(it, "args", { t: "list", v: l }))}
          />
          <p className="var-hint">The first line is the program to run, the lines after it are passed to it.</p>
          <MoreSettings>
            <ShellOptions item={item} set={set} uid={uid} ignoreError />
          </MoreSettings>
        </>
      );
    }
    case "form": {
      const fields = getParam(item, "fields");
      return (
        <>
          <TextField
            id={`${uid}-layout`}
            label="What the form looks like"
            yamlKey="layout"
            multiline
            rows={4}
            mono={false}
            value={textOf(item, "layout")}
            onChange={(v) => set((it) => withParam(it, "layout", text(v)))}
            hint={<>Write the text of the form and put [[name]] where an answer goes.</>}
          />
          <TextField
            id={`${uid}-fields`}
            label="The questions (written as YAML)"
            yamlKey="fields"
            multiline
            rows={6}
            value={fields ? (fields.t === "list" ? fields.v.join("\n") : fields.v) : ""}
            onChange={(v) => set((it) => withParam(it, "fields", v.trim() === "" ? null : { t: "raw", v }))}
            hint={<>One entry per [[name]] in the layout. For example a line <code>name:</code> and below it, indented, <code>type: text</code>.</>}
          />
        </>
      );
    }
    case "set_clipboard":
      return (
        <TextField
          id={`${uid}-text`}
          label="Text to put on the clipboard"
          yamlKey="text"
          multiline
          mono={false}
          value={textOf(item, "text")}
          onChange={(v) => set((it) => withParam(it, "text", text(v)))}
          hint="The macro itself inserts nothing."
        />
      );
    default:
      return null;
  }
}

/** Parameters the type's own form does not show (all of them for unknown types). */
function ExtraParams({ item, set, uid }: { item: Item; set: SetItem; uid: string }) {
  const advanced = useContext(AdvancedContext);
  const handled = HANDLED[item.type] ?? [];
  const extras = item.params.map((p, idx) => ({ p, idx })).filter(({ p }) => !handled.includes(p.key));
  const known = isKnown(item.type);
  const setAt = (idx: number, fn: (p: VarParam) => VarParam) =>
    set((it) => ({ ...it, params: it.params.map((p, i) => (i === idx ? fn(p) : p)) }));
  // Settings xpando has no field for are a technical matter: Advanced mode, or an unknown type
  // (where this is the only way to edit it).
  if (known && (extras.length === 0 || !advanced)) return null;
  return (
    <div className="var-field" role="group" aria-label={known ? "Other settings" : "Settings"}>
      <span className="var-label">{known ? "Other settings" : "Settings"}</span>
      {extras.map(({ p, idx }, n) => (
        <div className="param-row" key={idx}>
          <input
            className="field mono"
            aria-label={`Setting name ${n + 1}`}
            value={p.key}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setAt(idx, (q) => ({ ...q, key: e.target.value }))}
          />
          {p.value.t === "text" ? (
            <input
              className="field mono"
              aria-label={`Value of ${p.key || `setting ${n + 1}`}`}
              value={p.value.v}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setAt(idx, (q) => ({ ...q, value: text(e.target.value) }))}
            />
          ) : (
            <textarea
              className="field mono"
              rows={3}
              aria-label={`Value of ${p.key || `setting ${n + 1}`} (${p.value.t === "list" ? "one per line" : "YAML"})`}
              value={p.value.t === "list" ? p.value.v.join("\n") : p.value.v}
              spellCheck={false}
              onChange={(e) =>
                setAt(idx, (q) => ({
                  ...q,
                  value:
                    q.value.t === "list"
                      ? { t: "list", v: e.target.value.split("\n") }
                      : { t: "raw", v: e.target.value },
                }))
              }
            />
          )}
          <button
            type="button"
            className="btn btn-icon"
            aria-label={`Remove setting ${p.key || n + 1}`}
            title="Remove setting"
            onClick={() => set((it) => ({ ...it, params: it.params.filter((_, i) => i !== idx) }))}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      ))}
      <div>
        <button
          type="button"
          className="btn"
          id={`${uid}-addparam`}
          onClick={() => set((it) => ({ ...it, params: [...it.params, { key: "", value: text("") }] }))}
        >
          <Plus size={16} aria-hidden="true" />
          Add setting
        </button>
      </div>
    </div>
  );
}
