import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Braces,
  Calendar,
  ChevronDown,
  ChevronRight,
  Clipboard,
  ClipboardCopy,
  Info,
  ListChecks,
  MessageSquare,
  Plus,
  Shuffle,
  Terminal,
  TextCursorInput,
  Trash2,
  X,
} from "lucide-react";
import { placeholders, renameInText, sortByDependency } from "./varorder";
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

const KNOWN_TYPES = ["date", "clipboard", "echo", "random", "choice", "shell", "script", "form"] as const;
const TYPE_LABELS: Record<string, string> = {
  date: "Date and time",
  clipboard: "Clipboard",
  echo: "Text (echo)",
  random: "Random choice",
  choice: "Choice list (asks when expanding)",
  shell: "Shell command",
  script: "Script",
  form: "Form",
};
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
    if (s.name === "") return "Give the variable a name.";
    if (!NAME_RE.test(s.name))
      return "Use letters, digits and _ in the name, and don't start with a digit.";
    if (items.some((o, j) => j !== i && o.name === s.name)) return "Another variable already has this name.";
    if (!/^[A-Za-z0-9_-]+$/.test(s.type)) return "The type needs letters, digits, - or _ only.";
    const keys = s.params.map((p) => p.key);
    if (keys.some((k) => !/^[A-Za-z0-9_-]+$/.test(k))) return "Every parameter needs a name (letters, digits, - or _).";
    if (new Set(keys).size !== keys.length) return "A parameter name is used twice.";
    const blank = (k: string) => textOf(s, k).trim() === "";
    const noList = (k: string) => listOf(s, k).every((x) => x.trim() === "") && getParam(s, k)?.t !== "raw";
    switch (s.type) {
      case "echo":
        return getParam(s, "echo") === undefined ? "Enter the text to output." : null;
      case "shell":
        return blank("cmd") ? "Enter the command to run." : null;
      case "random":
        return noList("choices") ? "Add at least one choice." : null;
      case "choice":
        return noList("values") ? "Add at least one value." : null;
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
        `The text uses {{${n}}}, but this phrase has no variable with that name. That only works if a global variable provides it.`,
      );
    }
  }
  for (const s of items) {
    const byVar = items.some(
      (o) =>
        o.name !== s.name &&
        o.params.some((p) => paramStrings(p.value).some((t) => placeholders(t).includes(s.name))),
    );
    if (!inBody.includes(s.name) && !byVar) {
      out.push(`The variable “${s.name}” is not used by the text or by another variable.`);
    }
  }
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
    label: "Shell command",
    hint: "Output of a command",
    icon: <Terminal size={16} aria-hidden="true" />,
    baseName: "output",
    make: (n) => spec(n, "shell", [{ key: "cmd", value: text("") }]),
  },
  {
    id: "random",
    label: "Random",
    hint: "One of several, picked at random",
    icon: <Shuffle size={16} aria-hidden="true" />,
    baseName: "random",
    make: (n) => spec(n, "random", [{ key: "choices", value: { t: "list", v: ["Option 1", "Option 2"] } }]),
  },
  {
    id: "choice",
    label: "Choice list",
    hint: "Asks you to pick when expanding",
    icon: <ListChecks size={16} aria-hidden="true" />,
    baseName: "choice",
    make: (n) => spec(n, "choice", [{ key: "values", value: { t: "list", v: ["Option 1", "Option 2"] } }]),
  },
  {
    id: "echo",
    label: "Echo",
    hint: "A fixed text, reusable in other variables",
    icon: <MessageSquare size={16} aria-hidden="true" />,
    baseName: "text",
    make: (n) => spec(n, "echo", [{ key: "echo", value: text("") }]),
  },
  {
    // Needs xpando's engine build: stock espanso rejects this variable type.
    id: "set_clipboard",
    label: "Set clipboard",
    hint: "Fills the clipboard after pasting (xpando only)",
    icon: <ClipboardCopy size={16} aria-hidden="true" />,
    baseName: "setclip",
    make: (n) => spec(n, "set_clipboard", [{ key: "text", value: text("") }]),
  },
  {
    id: "cursor",
    label: "Cursor position",
    hint: "Where the cursor lands ($|$)",
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

interface UseVarsOptions {
  file: string;
  /** Index of the phrase in its file, or null for a new phrase. */
  index: number | null;
  /** Whether the phrase has variables in the file now (they are loaded then). */
  hasVars: boolean;
  body: string;
  setBody: (body: string) => void;
  /** `id` of the body text area, used to insert at the cursor. */
  bodyId: string;
  /** False when the body is not plain text: macros can't be inserted. */
  bodyEditable: boolean;
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
  /** The complete list to send to the backend; null when not editable. */
  specs: VarSpec[] | null;
}

let nextKey = 1;

/** Whether the Variables section was left open; remembered until the app is closed. */
let sectionOpenMemory = false;

export function useVars(opts: UseVarsOptions): VarsApi {
  const { index, hasVars, body, setBody, bodyId, bodyEditable, file } = opts;
  const [items, setItems] = useState<Item[]>([]);
  const [original, setOriginal] = useState<VarSpec[]>([]);
  const [state, setState] = useState<"loading" | "ready" | "error">(index !== null && hasVars ? "loading" : "ready");
  const [loadError, setLoadError] = useState("");
  const [announce, setAnnounce] = useState("");
  const lastValid = useRef(new Map<number, string>());
  const [open, setOpenState] = useState(sectionOpenMemory);

  useEffect(() => {
    if (index === null || !hasVars) return;
    let cancelled = false;
    invoke<VarSpec[]>("get_phrase_vars", { file, index })
      .then((specs) => {
        if (cancelled) return;
        const loaded = specs.map((s) => ({ ...s, key: nextKey++ }));
        loaded.forEach((it) => lastValid.current.set(it.key, it.name));
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
    const own = errors.flatMap((e, i) => (e ? [`${shown[i].name || `Variable ${i + 1}`}: ${e}`] : []));
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

  const update = useCallback((key: number, fn: (it: Item) => Item) => {
    setItems((all) => all.map((it) => (it.key === key ? fn(it) : it)));
  }, []);

  const rename = useCallback(
    (key: number, name: string) => {
      const item = items.find((it) => it.key === key);
      if (!item) return;
      const prev = lastValid.current.get(key) ?? item.name;
      const ok = NAME_RE.test(name) && !items.some((o) => o.key !== key && o.name === name);
      if (!ok || name === prev) {
        update(key, (it) => ({ ...it, name }));
        return;
      }
      lastValid.current.set(key, name);
      setItems((all) =>
        all.map((it) => {
          const params = it.params.map((p) => ({ key: p.key, value: renameInValue(p.value, prev, name) }));
          return it.key === key ? { ...it, name, params } : { ...it, params };
        }),
      );
      setBody(renameInText(body, prev, name));
    },
    [items, body, setBody, update],
  );

  const addItem = useCallback((make: (name: string) => VarSpec, base: string): string => {
    const name = uniqueName(base, items);
    const item: Item = { ...make(name), key: nextKey++ };
    lastValid.current.set(item.key, name);
    setItems([...items, item]);
    setOpenState(true);
    return name;
  }, [items]);

  function insertAtCursor(snippet: string) {
    const el = document.getElementById(bodyId) as HTMLTextAreaElement | null;
    const start = el ? el.selectionStart : body.length;
    const end = el ? el.selectionEnd : body.length;
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
    if (!m.make) {
      insertAtCursor("$|$");
      setAnnounce("Inserted the cursor position marker.");
      return;
    }
    const name = addItem(m.make, m.baseName);
    insertAtCursor(`{{${name}}}`);
    setAnnounce(`Inserted {{${name}}} and added a ${m.label} variable named ${name}.`);
  }

  const menu =
    editable && bodyEditable ? <MacroMenu onPick={insertMacro} announce={announce} /> : null;

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
      onRename={rename}
      onRemove={(key) => setItems((all) => all.filter((it) => it.key !== key))}
      onAdd={(type) => {
        const m = MACROS.find((x) => x.id === type);
        const name = addItem(
          m?.make ?? ((n) => spec(n, type, [])),
          m?.baseName || (type === "script" ? "script" : type === "form" ? "form" : "var"),
        );
        setAnnounce(`Added a variable named ${name}.`);
      }}
    />
  );

  return { menu, section, dirty, editable, problems, warnings, specs: editable ? specs : null };
}

// ---------------------------------------------------------------------------------------
// Insert macro menu
// ---------------------------------------------------------------------------------------

function MacroMenu({ onPick, announce }: { onPick: (m: MacroDef) => void; announce: string }) {
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
        onClick={() => setOpen((o) => !o)}
      >
        <Braces size={16} aria-hidden="true" />
        Insert macro
        <ChevronDown size={16} aria-hidden="true" />
      </button>
      {open && (
        <div className="macro-list" role="menu" id={`${uid}-menu`} aria-label="Insert macro">
          {MACROS.map((m) => (
            <button
              key={m.id}
              type="button"
              role="menuitem"
              className="macro-item"
              onClick={() => {
                setOpen(false);
                onPick(m);
              }}
            >
              {m.icon}
              <span className="macro-label">{m.label}</span>
              <span className="macro-hint">{m.hint}</span>
            </button>
          ))}
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
  onRename: (key: number, name: string) => void;
  onRemove: (key: number) => void;
  onFocusCapture: (el: HTMLElement | null) => void;
  onAdd: (type: string) => void;
}

function VarsSection(p: SectionProps) {
  const uid = useId();
  const [newType, setNewType] = useState("echo");
  const count = p.items.length;
  const addRow = (
    <div className="var-add">
      <select
        className="field"
        aria-label="Type of the new variable"
        value={newType}
        onChange={(e) => setNewType(e.target.value)}
      >
        {KNOWN_TYPES.map((t) => (
          <option key={t} value={t}>
            {TYPE_LABELS[t]}
          </option>
        ))}
      </select>
      <button type="button" className="btn" onClick={() => p.onAdd(newType)}>
        <Plus size={16} aria-hidden="true" />
        Add variable
      </button>
    </div>
  );

  if (p.state !== "ready") {
    return (
      <div className="editor-group vars-section" role="group" aria-labelledby={`${uid}-h`}>
        <span className="editor-legend" id={`${uid}-h`}>
          Variables
        </span>
        {p.state === "loading" && <p className="editor-note">Loading variables…</p>}
        {p.state === "error" && (
          <p className="editor-note" role="alert">
            <AlertTriangle size={16} aria-hidden="true" />
            <span>
              These variables use a layout xpando can't edit safely, so they are left as they are. Edit them in
              Advanced mode or a text editor. ({p.loadError})
            </span>
          </p>
        )}
      </div>
    );
  }

  // No variables: one compact row instead of an empty section.
  if (count === 0) {
    return (
      <div className="editor-group vars-section vars-empty" role="group" aria-labelledby={`${uid}-h`}>
        <span className="editor-legend" id={`${uid}-h`}>
          Variables
        </span>
        <p className="editor-note">
          <Info size={16} aria-hidden="true" />
          None yet. A variable fills in something when the phrase expands, like today's date.
        </p>
        {addRow}
      </div>
    );
  }

  const issues = p.problemCount + p.warningCount;
  return (
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
          Variables ({count})
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
        <p className="var-hint">
          <Info size={16} aria-hidden="true" />
          Variables are applied in the right order automatically: one that uses another goes after it.
        </p>
        <ol className="var-cards">
          {p.items.map((it, i) => (
            <VarCard
              key={it.key}
              item={it}
              index={i}
              error={p.errors[i]}
              onUpdate={p.onUpdate}
              onRename={p.onRename}
              onRemove={p.onRemove}
            />
          ))}
        </ol>
        {addRow}
      </div>
    </div>
  );
}

function VarCard({
  item,
  index,
  error,
  onUpdate,
  onRename,
  onRemove,
}: {
  item: Item;
  index: number;
  error: string | null;
  onUpdate: (key: number, fn: (it: Item) => Item) => void;
  onRename: (key: number, name: string) => void;
  onRemove: (key: number) => void;
}) {
  const uid = useId();
  const known = isKnown(item.type);
  const set = (fn: (it: Item) => Item) => onUpdate(item.key, fn);
  const label = item.name || `variable ${index + 1}`;
  return (
    <li className="var-card" aria-label={`Variable ${label}`}>
      <div className="var-head">
        <div className="var-field var-name">
          <label htmlFor={`${uid}-name`}>Name</label>
          <input
            id={`${uid}-name`}
            className="field mono"
            value={item.name}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={error !== null && /name/i.test(error)}
            onChange={(e) => onRename(item.key, e.target.value)}
          />
        </div>
        <div className="var-field var-type">
          <label htmlFor={`${uid}-type`}>Type</label>
          <select
            id={`${uid}-type`}
            className="field"
            value={known ? item.type : "__other"}
            onChange={(e) => {
              const type = e.target.value === "__other" ? "custom" : e.target.value;
              set((it) => ({ ...it, type, params: [] }));
            }}
          >
            {KNOWN_TYPES.map((t) => (
              <option key={t} value={t}>
                {TYPE_LABELS[t]}
              </option>
            ))}
            <option value="__other">Other type…</option>
          </select>
        </div>
        <div className="var-buttons">
          <button
            type="button"
            className="btn btn-icon"
            aria-label={`Remove ${label}`}
            title="Remove variable"
            onClick={() => onRemove(item.key)}
          >
            <Trash2 size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
      {!known && (
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
            xpando doesn't know this type, so its parameters are edited as plain name and value pairs.
          </p>
        </div>
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
  hint?: ReactNode;
}) {
  const cls = `field${mono ? " mono" : ""}`;
  return (
    <div className="var-field">
      <label htmlFor={id}>{label}</label>
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
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="var-check">
      <input id={id} type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <label htmlFor={id}>{label}</label>
    </div>
  );
}

function ListField({
  uid,
  label,
  values,
  onChange,
  addLabel,
  mono = false,
}: {
  uid: string;
  label: string;
  values: string[];
  onChange: (v: string[]) => void;
  addLabel: string;
  mono?: boolean;
}) {
  return (
    <div className="var-field" role="group" aria-label={label}>
      <span className="var-label">{label}</span>
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
      values={values}
      addLabel={addLabel}
      onChange={(l) => set((it) => withParam(it, paramKey, { t: "list", v: l }))}
    />
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
        label="Format"
        value={format}
        placeholder="%Y-%m-%d"
        onChange={(v) => opt("format", v)}
        hint={
          <>
            Preview: <output className="date-preview">{preview || "(empty)"}</output>
          </>
        }
      />
      <details className="var-help">
        <summary>Format codes</summary>
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
          label="Offset in seconds"
          value={offset}
          placeholder="0"
          onChange={(v) => opt("offset", v)}
          hint="For example 86400 is tomorrow, -3600 is an hour ago."
        />
        <TextField
          id={`${uid}-locale`}
          label="Language (locale)"
          value={locale}
          placeholder="en-US"
          onChange={(v) => opt("locale", v)}
          hint="Month and day names, for example de-DE."
        />
        <TextField
          id={`${uid}-tz`}
          label="Time zone"
          value={textOf(item, "tz")}
          placeholder="Europe/Berlin"
          onChange={(v) => opt("tz", v)}
          hint="Leave empty for this computer's time zone."
        />
      </div>
      <p className="var-hint">The preview is approximate; espanso formats the final result.</p>
    </>
  );
}

function ShellOptions({ item, set, uid, ignoreError }: { item: Item; set: SetItem; uid: string; ignoreError: boolean }) {
  const flag = (key: string) => textOf(item, key) === "true";
  return (
    <div className="var-checks">
      <CheckField
        id={`${uid}-trim`}
        label="Trim whitespace around the output"
        checked={textOf(item, "trim") !== "false"}
        onChange={(c) => set((it) => withParam(it, "trim", c ? null : text("false")))}
      />
      <CheckField
        id={`${uid}-debug`}
        label="Write debug details to the log"
        checked={flag("debug")}
        onChange={(c) => set((it) => withParam(it, "debug", c ? text("true") : null))}
      />
      {ignoreError && (
        <CheckField
          id={`${uid}-ignore`}
          label="Ignore errors"
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
          Inserts whatever text is on the clipboard when the phrase expands. It has no settings.
        </p>
      );
    case "echo":
      return (
        <TextField
          id={`${uid}-echo`}
          label="Text"
          multiline
          mono={false}
          value={textOf(item, "echo")}
          onChange={(v) => set((it) => withParam(it, "echo", text(v)))}
          hint={<>Can use other variables, for example {"{{date}}"}_{"{{clipboard}}"}.</>}
        />
      );
    case "random":
      return (
        <ListOrRaw
          item={item}
          set={set}
          uid={uid}
          paramKey="choices"
          label="Choice"
          addLabel="Add choice"
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
          label="Value"
          addLabel="Add value"
          rawNote="These values have an id and a label."
        />
      );
    case "shell": {
      const shell = textOf(item, "shell");
      return (
        <>
          <TextField
            id={`${uid}-cmd`}
            label="Command"
            multiline
            rows={3}
            value={textOf(item, "cmd")}
            onChange={(v) => set((it) => withParam(it, "cmd", text(v)))}
            hint="Its output replaces the variable. Can use other variables."
          />
          <div className="var-field">
            <label htmlFor={`${uid}-shell`}>Shell</label>
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
        </>
      );
    }
    case "script": {
      const args = listOf(item, "args");
      return (
        <>
          <ListField
            uid={`${uid}-args`}
            label="Argument"
            values={args}
            addLabel="Add argument"
            mono
            onChange={(l) => set((it) => withParam(it, "args", { t: "list", v: l }))}
          />
          <p className="var-hint">The first argument is the program, the rest are passed to it.</p>
          <ShellOptions item={item} set={set} uid={uid} ignoreError />
        </>
      );
    }
    case "form": {
      const fields = getParam(item, "fields");
      return (
        <>
          <TextField
            id={`${uid}-layout`}
            label="Layout"
            multiline
            rows={4}
            mono={false}
            value={textOf(item, "layout")}
            onChange={(v) => set((it) => withParam(it, "layout", text(v)))}
            hint={<>Use [[name]] where a field goes.</>}
          />
          <TextField
            id={`${uid}-fields`}
            label="Fields (YAML)"
            multiline
            rows={6}
            value={fields ? (fields.t === "list" ? fields.v.join("\n") : fields.v) : ""}
            onChange={(v) => set((it) => withParam(it, "fields", v.trim() === "" ? null : { t: "raw", v }))}
            hint="Each field is a name with a type, for example: name: then type: text on the next line, indented."
          />
        </>
      );
    }
    default:
      return null;
  }
}

/** Parameters the type's own form does not show (all of them for unknown types). */
function ExtraParams({ item, set, uid }: { item: Item; set: SetItem; uid: string }) {
  const handled = HANDLED[item.type] ?? [];
  const extras = item.params.map((p, idx) => ({ p, idx })).filter(({ p }) => !handled.includes(p.key));
  const known = isKnown(item.type);
  const setAt = (idx: number, fn: (p: VarParam) => VarParam) =>
    set((it) => ({ ...it, params: it.params.map((p, i) => (i === idx ? fn(p) : p)) }));
  if (extras.length === 0 && known) return null;
  return (
    <div className="var-field" role="group" aria-label={known ? "Other parameters" : "Parameters"}>
      <span className="var-label">{known ? "Other parameters" : "Parameters"}</span>
      {extras.map(({ p, idx }, n) => (
        <div className="param-row" key={idx}>
          <input
            className="field mono"
            aria-label={`Parameter name ${n + 1}`}
            value={p.key}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setAt(idx, (q) => ({ ...q, key: e.target.value }))}
          />
          {p.value.t === "text" ? (
            <input
              className="field mono"
              aria-label={`Value of ${p.key || `parameter ${n + 1}`}`}
              value={p.value.v}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setAt(idx, (q) => ({ ...q, value: text(e.target.value) }))}
            />
          ) : (
            <textarea
              className="field mono"
              rows={3}
              aria-label={`Value of ${p.key || `parameter ${n + 1}`} (${p.value.t === "list" ? "one per line" : "YAML"})`}
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
            aria-label={`Remove parameter ${p.key || n + 1}`}
            title="Remove parameter"
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
          Add parameter
        </button>
      </div>
    </div>
  );
}
