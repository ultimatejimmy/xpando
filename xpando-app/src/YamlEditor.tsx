import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, Loader2, RefreshCw, Save } from "lucide-react";

/** Prefix the backend puts on the error for a file that changed since it was loaded. */
const CONFLICT_PREFIX = "file-changed:";
const DEBOUNCE_MS = 300;
const INDENT = "  ";

interface YamlError {
  kind: string;
  line: number;
  column: number;
  message: string;
  lost: string[];
}

interface Validation {
  ok: boolean;
  error: YamlError | null;
  indent_shift: number;
  notice: string | null;
}

interface SaveFailure {
  kind: string;
  message: string;
  error: YamlError | null;
}

export interface SavedFolder {
  file: string;
  hash: string;
}

type Mode = "entry" | "file";

/** Unsaved text survives switching to another phrase and back (not an app restart). */
interface Draft {
  mode: Mode;
  entry: string;
  file: string | null;
}
const drafts = new Map<string, Draft>();

export function draftKey(file: string, index: number): string {
  return `${file}#${index}`;
}

export function hasDraft(file: string, index: number): boolean {
  return drafts.has(draftKey(file, index));
}

function describeFailure(e: unknown): SaveFailure {
  if (e && typeof e === "object" && "kind" in e && "message" in e) return e as SaveFailure;
  return { kind: "other", message: String(e), error: null };
}

/** Start offsets of each line of `text`. */
function lineStarts(text: string): number[] {
  const starts = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") starts.push(i + 1);
  return starts;
}

function lineIndexAt(starts: number[], pos: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Replaces `[start, end)` through the browser's editing path, so undo keeps working. */
function replaceRange(
  ta: HTMLTextAreaElement,
  start: number,
  end: number,
  replacement: string,
  selStart: number,
  selEnd: number,
  setText: (t: string) => void,
) {
  ta.focus();
  ta.setSelectionRange(start, end);
  // execCommand is deprecated but is the only way to edit a textarea and keep Ctrl+Z.
  if (!document.execCommand("insertText", false, replacement)) {
    ta.setRangeText(replacement, start, end, "end");
    setText(ta.value);
  }
  ta.setSelectionRange(selStart, selEnd);
}

/** Tab and Shift+Tab: two spaces in or out, for the caret's line or every selected line. */
function indentSelection(ta: HTMLTextAreaElement, dedent: boolean, setText: (t: string) => void) {
  const { value, selectionStart: s, selectionEnd: e } = ta;
  if (!dedent && s === e) {
    replaceRange(ta, s, e, INDENT, s + INDENT.length, s + INDENT.length, setText);
    return;
  }
  const starts = lineStarts(value);
  const first = lineIndexAt(starts, s);
  let last = lineIndexAt(starts, e);
  if (e > s && starts[last] === e) last -= 1; // a selection ending at a line start excludes it
  last = Math.max(last, first);
  const blockStart = starts[first];
  const blockEnd = last + 1 < starts.length ? starts[last + 1] - 1 : value.length;
  const block = value.slice(blockStart, blockEnd).split("\n");
  let firstDelta = 0;
  const out = block.map((line, i) => {
    let next: string;
    if (dedent) {
      const lead = line.length - line.trimStart().length;
      const k = Math.min(INDENT.length, lead);
      next = line.slice(k);
      if (i === 0) firstDelta = -k;
    } else {
      next = line.length > 0 ? INDENT + line : line;
      if (i === 0 && line.length > 0) firstDelta = INDENT.length;
    }
    return next;
  });
  const joined = out.join("\n");
  const delta = joined.length - block.join("\n").length;
  const newS = Math.max(blockStart, s + firstDelta);
  const newE = s === e ? newS : e + delta;
  replaceRange(ta, blockStart, blockEnd, joined, newS, newE, setText);
}

/** A native modal dialog; closing it (Esc, backdrop-less) means "keep editing". */
function ConfirmDialog({
  title,
  children,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onClose,
}: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);
  return (
    <dialog ref={ref} className="confirm" aria-labelledby={titleId} onClose={onClose}>
      <h2 className="confirm-title" id={titleId}>
        {title}
      </h2>
      {children}
      <div className="editor-buttons">
        <button type="button" className="btn btn-danger" onClick={onConfirm}>
          {confirmLabel}
        </button>
        <button type="button" className="btn" autoFocus onClick={onClose}>
          {cancelLabel}
        </button>
      </div>
    </dialog>
  );
}

interface Props {
  file: string;
  index: number;
  /** The entry's current source, and the file line of its first line. */
  entryBase: string;
  firstLine: number;
  /** The folder's content hash; saves are refused when the file no longer has it. */
  hash: string;
  initialMode?: Mode;
  /** Called with the reloaded folder after a successful save. */
  onSaved: (folder: SavedFolder) => void;
  /** Leaves the editor without saving. */
  onClose: () => void;
  /** Reloads the folders from disk (the draft is kept). */
  onReload: () => Promise<void>;
}

export function YamlEditor({
  file,
  index,
  entryBase,
  firstLine,
  hash,
  initialMode,
  onSaved,
  onClose,
  onReload,
}: Props) {
  const key = draftKey(file, index);
  const saved = drafts.get(key);
  const [mode, setMode] = useState<Mode>(initialMode ?? saved?.mode ?? "entry");
  const [entryText, setEntryText] = useState(saved?.entry ?? entryBase);
  const [fileBase, setFileBase] = useState<string | null>(null);
  const [fileText, setFileText] = useState<string | null>(saved?.file ?? null);
  const [validation, setValidation] = useState<Validation | null>(null);
  const [pending, setPending] = useState(true);
  const [callError, setCallError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmedLoss, setConfirmedLoss] = useState<string | null>(null);
  const [askDiscard, setAskDiscard] = useState<null | "close" | Mode>(null);
  const [reloaded, setReloaded] = useState(false);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const mirrorRef = useRef<HTMLDivElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const seq = useRef(0);
  const finished = useRef(false);
  const prevEntryBase = useRef(entryBase);
  const statusId = useId();
  const [heights, setHeights] = useState<number[]>([]);

  // When the file is reloaded the entry's base text changes; follow it unless the user typed.
  useEffect(() => {
    setEntryText((t) => (t === prevEntryBase.current ? entryBase : t));
    prevEntryBase.current = entryBase;
  }, [entryBase]);

  // Whole-file mode needs the file's text; fetch it (again after a reload).
  useEffect(() => {
    if (mode !== "file") return;
    let current = true;
    invoke<string>("get_file_source", { file, expectedHash: hash })
      .then((t) => {
        if (!current) return;
        setFileText((cur) => (cur === null || cur === fileBase ? t : cur));
        setFileBase(t);
      })
      .catch((e) => current && setCallError(String(e)));
    return () => {
      current = false;
    };
    // fileBase is read only to decide whether the user has typed; it must not retrigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, file, hash]);

  const text = mode === "entry" ? entryText : (fileText ?? "");
  const base = mode === "entry" ? entryBase : (fileBase ?? "");
  const setText = (t: string) => (mode === "entry" ? setEntryText(t) : setFileText(t));
  const textReady = mode === "entry" || fileText !== null;
  const changed = text !== base;
  const entryDirty = entryText !== entryBase;
  const fileDirty = fileText !== null && fileBase !== null && fileText !== fileBase;
  const dirty = mode === "entry" ? entryDirty : fileDirty;
  const firstNumber = mode === "entry" ? firstLine : 1;

  // Remember unsaved text so it survives selecting another phrase and coming back.
  useEffect(() => {
    if (finished.current) return;
    if (entryDirty || fileDirty) drafts.set(key, { mode, entry: entryText, file: fileText });
    else drafts.delete(key);
  }, [key, mode, entryText, fileText, entryDirty, fileDirty]);

  // Live validation, debounced.
  useEffect(() => {
    if (!textReady) return;
    setPending(true);
    const mine = ++seq.current;
    const timer = setTimeout(
      async () => {
        try {
          const v =
            mode === "entry"
              ? await invoke<Validation>("validate_entry_yaml", {
                  file,
                  index,
                  text,
                  expectedHash: hash,
                })
              : await invoke<Validation>("validate_file_yaml", {
                  file,
                  text,
                  confirmLoss: false,
                  expectedHash: hash,
                });
          if (mine !== seq.current) return;
          setValidation(v);
          setCallError(null);
        } catch (e) {
          if (mine !== seq.current) return;
          setValidation(null);
          setCallError(String(e));
        }
        setPending(false);
      },
      validation === null ? 0 : DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
    // `validation` only picks the first (immediate) run.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, mode, file, index, hash, textReady]);

  // Line heights: a hidden copy of the text wraps exactly as the textarea does, so each line
  // number can be as tall as its wrapped line and the textarea can grow to fit (no inner scroll).
  const lines = useMemo(() => text.split("\n"), [text]);
  const measure = () => {
    const m = mirrorRef.current;
    if (!m) return;
    const hs = Array.from(m.children, (c) => (c as HTMLElement).offsetHeight);
    setHeights((prev) => (prev.length === hs.length && prev.every((h, i) => h === hs[i]) ? prev : hs));
  };
  useLayoutEffect(measure, [lines]);
  useEffect(() => {
    const w = wrapRef.current;
    if (!w) return;
    const ro = new ResizeObserver(measure);
    ro.observe(w);
    return () => ro.disconnect();
  }, []);
  const total = heights.reduce((a, b) => a + b, 0);

  const err = validation?.error ?? null;
  const lostKey = err?.kind === "loses_phrases" ? err.lost.join("\n") : null;
  const lossConfirmed = lostKey !== null && confirmedLoss === lostKey;
  const canSave =
    changed &&
    !pending &&
    !saving &&
    textReady &&
    validation !== null &&
    (validation.ok || (mode === "file" && lossConfirmed));
  const errLine = err && err.kind !== "loses_phrases" ? Math.min(err.line, lines.length) : null;
  const errTop = errLine ? heights.slice(0, errLine - 1).reduce((a, b) => a + b, 0) : 0;

  function goTo(line: number, column: number) {
    const ta = taRef.current;
    if (!ta) return;
    const starts = lineStarts(ta.value);
    const l = Math.min(Math.max(line, 1), starts.length) - 1;
    const pos = Math.min(starts[l] + Math.max(column - 1, 0), ta.value.length);
    ta.focus();
    ta.setSelectionRange(pos, pos);
  }

  async function save() {
    if (!canSave) return;
    setSaving(true);
    setCallError(null);
    try {
      const folder =
        mode === "entry"
          ? await invoke<SavedFolder>("save_entry_yaml", { file, index, text, expectedHash: hash })
          : await invoke<SavedFolder>("save_file_yaml", {
              file,
              text,
              confirmLoss: lossConfirmed,
              expectedHash: hash,
            });
      finished.current = true;
      drafts.delete(key);
      onSaved(folder);
    } catch (e) {
      const f = describeFailure(e);
      if (f.error) {
        setValidation({ ok: false, error: f.error, indent_shift: 0, notice: null });
      } else {
        setCallError(f.message);
      }
      setSaving(false);
    }
  }

  function leave() {
    finished.current = true;
    drafts.delete(key);
    onClose();
  }

  function requestClose() {
    if (entryDirty || fileDirty) setAskDiscard("close");
    else leave();
  }

  function requestMode(next: Mode) {
    if (next === mode) return;
    if (dirty) setAskDiscard(next);
    else setMode(next);
  }

  function discardAndContinue() {
    const target = askDiscard;
    setAskDiscard(null);
    if (target === "close") {
      leave();
    } else if (target) {
      if (mode === "entry") setEntryText(entryBase);
      else setFileText(fileBase);
      setMode(target);
    }
  }

  async function reload() {
    setCallError(null);
    await onReload();
    setReloaded(true);
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.nativeEvent.isComposing) return;
    const ta = e.currentTarget;
    if (e.key === "Escape") {
      e.preventDefault();
      requestClose();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
    } else if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      indentSelection(ta, e.shiftKey, setText);
    } else if (e.key === "Enter" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      const starts = lineStarts(ta.value);
      const li = lineIndexAt(starts, ta.selectionStart);
      const before = ta.value.slice(starts[li], ta.selectionStart);
      const lead = before.length - before.trimStart().length;
      if (lead > 0) {
        e.preventDefault();
        const ins = "\n" + " ".repeat(lead);
        const pos = ta.selectionStart + ins.length;
        replaceRange(ta, ta.selectionStart, ta.selectionEnd, ins, pos, pos, setText);
      }
    }
  }

  const conflict = callError?.startsWith(CONFLICT_PREFIX) ?? false;
  const shown = validation?.indent_shift
    ? `Valid. Indentation will be ${validation.indent_shift > 0 ? "increased" : "reduced"} by ${Math.abs(validation.indent_shift)} to fit the file.`
    : "Valid";

  return (
    <section className="ye" aria-label="Edit YAML">
      <div className="ye-head">
        <span className="ye-where">
          {file}
          {mode === "entry" ? ` · from line ${firstLine}` : " · whole file"}
        </span>
        <label className="adv-switch">
          <input
            type="checkbox"
            role="switch"
            checked={mode === "file"}
            onChange={(e) => requestMode(e.target.checked ? "file" : "entry")}
          />
          <span className="adv-track" aria-hidden="true" />
          <span>Whole file</span>
        </label>
      </div>
      {mode === "file" && (
        <p className="ye-note">
          You are editing every phrase and top-level key of this file. A phrase that disappears must
          be confirmed.
        </p>
      )}

      <div className="ye-box">
        <div className="ye-gutter" aria-hidden="true">
          {lines.map((_, i) => (
            <div
              key={i}
              className={errLine === i + 1 ? "ye-ln ye-ln-bad" : "ye-ln"}
              style={heights[i] ? { height: heights[i] } : undefined}
            >
              {firstNumber + i}
            </div>
          ))}
        </div>
        <div className="ye-text" ref={wrapRef}>
          {errLine && heights[errLine - 1] ? (
            <div
              className="ye-errband"
              aria-hidden="true"
              style={{ top: errTop, height: heights[errLine - 1] }}
            />
          ) : null}
          <div className="ye-mirror" ref={mirrorRef} aria-hidden="true">
            {lines.map((l, i) => (
              <div key={i}>{l === "" ? "​" : l}</div>
            ))}
          </div>
          <textarea
            ref={taRef}
            className="ye-area"
            value={textReady ? text : ""}
            readOnly={!textReady}
            autoFocus
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            wrap="soft"
            aria-label={mode === "entry" ? "YAML of this phrase" : "YAML of the whole file"}
            aria-describedby={statusId}
            aria-invalid={err ? true : undefined}
            style={total > 0 ? { height: total + 16 } : undefined}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
          />
        </div>
      </div>

      <div className="ye-status" id={statusId} role="status" aria-live="polite">
        {callError ? (
          <span className="adv-status adv-bad">
            <AlertTriangle size={14} aria-hidden="true" />
            <span>{conflict ? callError.slice(CONFLICT_PREFIX.length).trim() : callError}</span>
            {conflict && (
              <button type="button" className="btn" onClick={() => void reload()}>
                <RefreshCw size={16} aria-hidden="true" />
                Reload
              </button>
            )}
          </span>
        ) : !textReady ? (
          <span>Loading the file…</span>
        ) : pending || validation === null ? (
          <span className="ye-checking">
            <Loader2 size={14} aria-hidden="true" />
            Checking…
          </span>
        ) : err ? (
          <span className="adv-status adv-bad">
            <AlertTriangle size={14} aria-hidden="true" />
            <span>
              {err.kind === "loses_phrases"
                ? err.message
                : `Line ${firstNumber + err.line - 1}, column ${err.column}: ${err.message}`}
            </span>
            {err.kind !== "loses_phrases" && (
              <button type="button" className="ye-link" onClick={() => goTo(err.line, err.column)}>
                Go to
              </button>
            )}
          </span>
        ) : (
          <span className="adv-status adv-ok">
            <CheckCircle2 size={14} aria-hidden="true" />
            <span>{changed ? shown : "Valid, no changes yet"}</span>
          </span>
        )}
        {validation?.notice && !err && !callError && <p className="ye-sub">{validation.notice}</p>}
        {reloaded && !callError && (
          <p className="ye-sub">The file was reloaded. Your edit is kept; it was checked against the new version.</p>
        )}
      </div>

      {lostKey !== null && err && (
        <div className="ye-loss">
          <p>
            These phrases are in the file now but not in your text:
          </p>
          <ul>
            {err.lost.map((l, i) => (
              <li key={i}>{l}</li>
            ))}
          </ul>
          <label className="ye-check">
            <input
              type="checkbox"
              checked={lossConfirmed}
              onChange={(e) => setConfirmedLoss(e.target.checked ? lostKey : null)}
            />
            <span>Remove these phrases from the file</span>
          </label>
        </div>
      )}

      <p className="ye-hint">
        Tab adds two spaces, Shift+Tab removes them. Ctrl+S saves. Esc leaves the editor.
      </p>

      <div className="editor-buttons">
        <button type="button" className="btn btn-primary" disabled={!canSave} onClick={() => void save()}>
          <Save size={16} aria-hidden="true" />
          {saving ? "Saving…" : "Save"}
        </button>
        <button type="button" className="btn" onClick={requestClose} disabled={saving}>
          Cancel
        </button>
      </div>

      {askDiscard && (
        <ConfirmDialog
          title="Discard your changes?"
          confirmLabel="Discard"
          cancelLabel="Keep editing"
          onConfirm={discardAndContinue}
          onClose={() => {
            setAskDiscard(null);
            taRef.current?.focus();
          }}
        >
          <p>
            {askDiscard === "close"
              ? "The text you changed has not been saved."
              : "Switching between this phrase and the whole file discards the text you changed."}
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
