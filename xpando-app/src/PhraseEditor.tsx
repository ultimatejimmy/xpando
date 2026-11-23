import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Check,
  ChevronRight,
  Copy,
  Info,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import "./editor.css";
import { useVars } from "./VarsEditor"; // xpando:vars
import { useAdvanced } from "./AdvancedPanels";
import { HelpTip } from "./HelpTip";
import { BodyModeSwitch, RichBodyEditor } from "./RichLazy"; // xpando:richtext
import type { BodyMode } from "./RichLazy"; // xpando:richtext
import { isBlankHtml, listWords } from "./richtextUtil"; // xpando:richtext
import type { RichHandle } from "./richtextUtil"; // xpando:richtext

/** espanso's per-match options as the backend sends them (`null` = not set in the file). */
export interface EditorOptions {
  word: boolean | null;
  left_word: boolean | null;
  right_word: boolean | null;
  propagate_case: boolean | null;
  uppercase_style: string | null;
  force_mode: string | null;
  search_terms: string[];
}

export const NO_OPTIONS: EditorOptions = {
  word: null,
  left_word: null,
  right_word: null,
  propagate_case: null,
  uppercase_style: null,
  force_mode: null,
  search_terms: [],
};

/** Tolerates a backend that predates `options`. */
export function normalizeOptions(o: Partial<EditorOptions> | null | undefined): EditorOptions {
  return { ...NO_OPTIONS, ...(o ?? {}), search_terms: o?.search_terms ?? [] };
}

function optionsKey(o: EditorOptions): string {
  return JSON.stringify([
    o.word,
    o.left_word,
    o.right_word,
    o.propagate_case,
    o.uppercase_style,
    o.force_mode,
    o.search_terms,
  ]);
}

/** The parts of the browser's phrase and folder types that editing needs. */
export interface EditorPhrase {
  triggers: string[];
  is_regex: boolean;
  label: string | null;
  kind: string;
  content: string;
  preview: string;
  vars: { name: string; kind: string; params: [string, string][] }[];
  comment: string | null;
  index: number;
  line: number | null;
  options: EditorOptions;
}

export interface EditorFolder {
  name: string;
  file: string;
  hash: string;
  warnings: string[];
  phrases: EditorPhrase[];
}

/** Prefix the backend puts on the error for a file that changed since it was loaded. */
const CONFLICT_PREFIX = "file-changed:";

function isConflict(message: string): boolean {
  return message.startsWith(CONFLICT_PREFIX);
}

function errorText(e: unknown): string {
  return String(e);
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function phraseName(p: EditorPhrase): string {
  return p.label || p.triggers[0] || "(untitled)";
}

/** Whether two reads of a phrase are the same entry content (the line may move). */
function samePhrase(a: EditorPhrase, b: EditorPhrase): boolean {
  return (
    sameList(a.triggers, b.triggers) &&
    a.is_regex === b.is_regex &&
    a.label === b.label &&
    a.kind === b.kind &&
    a.content === b.content &&
    a.comment === b.comment &&
    JSON.stringify(a.vars) === JSON.stringify(b.vars) &&
    optionsKey(normalizeOptions(a.options)) === optionsKey(normalizeOptions(b.options))
  );
}

// ---------------------------------------------------------------------------------------
// Option descriptions
// ---------------------------------------------------------------------------------------

type FlagKey = "word" | "left_word" | "right_word" | "propagate_case";

const FLAGS: { key: FlagKey; label: string; help: string; advanced?: boolean }[] = [
  {
    key: "word",
    label: "Only expand when I type it as a whole word",
    help: "The trigger only expands when it has a space or punctuation on both sides, so it does not fire inside another word.",
  },
  {
    key: "propagate_case",
    label: "Match the capitalization I type",
    help: "Typing the trigger capitalised or in capitals gives a capitalised or all-capitals result.",
  },
  {
    key: "left_word",
    label: "Only expand at the start of a word",
    help: "Only expand when the trigger starts a word, so it does not fire in the middle of one.",
    advanced: true,
  },
  {
    key: "right_word",
    label: "Only expand at the end of a word",
    help: "Only expand when the trigger ends a word, so it does not fire before more letters.",
    advanced: true,
  },
];

/** The file's own name of an option, shown small next to the plain label in Advanced mode. */
function KeyName({ name }: { name: string }) {
  return <code className="opt-key">{name}</code>;
}

const UPPERCASE_STYLES: [string, string][] = [
  ["", "Default (all capitals)"],
  ["uppercase", "All capitals"],
  ["capitalize", "First letter only"],
  ["capitalize_words", "First letter of each word"],
];

const FORCE_MODES: [string, string][] = [
  ["", "Automatic"],
  ["clipboard", "Paste from the clipboard"],
  ["keys", "Type with the keyboard"],
];

function optionLabel(list: [string, string][], value: string): string {
  return list.find(([v]) => v === value)?.[1] ?? value;
}

/** Short names for the options that are set, for chips in the detail pane. */
export function optionChipTexts(raw: EditorOptions | null | undefined): string[] {
  const o = normalizeOptions(raw);
  const out: string[] = [];
  if (o.word) out.push("Whole word");
  if (o.left_word) out.push("Word start");
  if (o.right_word) out.push("Word end");
  if (o.propagate_case) {
    out.push(
      o.uppercase_style
        ? `Match case: ${optionLabel(UPPERCASE_STYLES, o.uppercase_style).toLowerCase()}`
        : "Match case",
    );
  }
  if (o.force_mode) out.push(o.force_mode === "clipboard" ? "Pastes" : o.force_mode === "keys" ? "Types" : `Mode: ${o.force_mode}`);
  if (o.search_terms.length > 0) {
    out.push(`${o.search_terms.length} search ${o.search_terms.length === 1 ? "term" : "terms"}`);
  }
  return out;
}

/** The active options as small chips; nothing when none are set. */
export function OptionChips({ options }: { options: EditorOptions | null | undefined }) {
  const texts = optionChipTexts(options);
  if (texts.length === 0) return null;
  return (
    <>
      <p className="detail-meta">Options</p>
      <div className="chips">
        {texts.map((t) => (
          <span className="chip" key={t}>
            {t}
          </span>
        ))}
      </div>
    </>
  );
}

/** Error text, plus a Reload button when the file changed on disk. */
function ErrorMessage({ message, onReload }: { message: string; onReload: () => void }) {
  const conflict = isConflict(message);
  return (
    <div className="editor-error" role="alert">
      <AlertTriangle size={16} aria-hidden="true" />
      <span className="grow">{conflict ? message.slice(CONFLICT_PREFIX.length).trim() : message}</span>
      {conflict && (
        <button type="button" className="btn" onClick={onReload}>
          <RefreshCw size={16} aria-hidden="true" />
          Reload
        </button>
      )}
    </div>
  );
}

export function NewPhraseButton({ onClick, disabled }: { onClick: () => void; disabled?: boolean }) {
  return (
    <button type="button" className="btn btn-primary new-phrase" onClick={onClick} disabled={disabled}>
      <Plus size={16} aria-hidden="true" />
      New phrase
    </button>
  );
}

/** Modal shown when the user tries to leave an editor that has unsaved changes. */
export function UnsavedChangesDialog({ onKeep, onDiscard }: { onKeep: () => void; onDiscard: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const discarding = useRef(false);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  return (
    <dialog
      ref={ref}
      className="confirm"
      aria-labelledby={titleId}
      onClose={() => {
        if (!discarding.current) onKeep();
      }}
    >
      <h2 id={titleId} className="confirm-title">
        Discard your changes?
      </h2>
      <p>You have changes to this phrase that are not saved yet. If you leave now they are lost.</p>
      <div className="editor-buttons">
        <button type="button" className="btn btn-primary" autoFocus onClick={onKeep}>
          Keep editing
        </button>
        <button
          type="button"
          className="btn btn-danger"
          onClick={() => {
            discarding.current = true;
            ref.current?.close();
            onDiscard();
          }}
        >
          Discard changes
        </button>
      </div>
    </dialog>
  );
}

// ---------------------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------------------

interface EditorProps {
  folders: EditorFolder[];
  /** The file the phrase lives in, or the file a new phrase is added to. */
  file: string;
  /** The phrase as it was when editing started, or null to create one. Keep it stable. */
  phrase: EditorPhrase | null;
  /** Called with the reloaded folder and the index of the phrase to select. */
  onSaved: (folder: EditorFolder, select: number) => void;
  /** Leave the editor. The parent decides whether to ask first (see `onDirtyChange`). */
  onCancel: () => void;
  /** Reloads every folder from disk and returns them (null when that failed). */
  onReload: () => Promise<EditorFolder[] | null>;
  /** Reports whether the editor holds unsaved changes. */
  onDirtyChange: (dirty: boolean) => void;
}

/** Plain text of a draft, for the copy button. */
function draftText(
  triggers: string[],
  label: string,
  description: string,
  body: string,
  o: EditorOptions,
): string {
  const lines = [`Trigger${triggers.length > 1 ? "s" : ""}: ${triggers.join(", ")}`];
  if (label) lines.push(`Name: ${label}`);
  if (description) lines.push(`Description: ${description.replace(/\n/g, "\n  ")}`);
  const opts = optionChipTexts(o);
  if (opts.length > 0) lines.push(`Options: ${opts.join(", ")}`);
  if (o.search_terms.length > 0) lines.push(`Search terms: ${o.search_terms.join(", ")}`);
  lines.push("Text:", body);
  return lines.join("\n");
}

export function PhraseEditor({ folders, file, phrase, onSaved, onCancel, onReload, onDirtyChange }: EditorProps) {
  const folder = folders.find((f) => f.file === file);
  const baseOptions = useMemo(() => normalizeOptions(phrase?.options), [phrase]);
  const [triggers, setTriggers] = useState<string[]>(phrase ? phrase.triggers : [""]);
  const [label, setLabel] = useState(phrase?.label ?? "");
  const [description, setDescription] = useState(phrase?.comment ?? "");
  const [body, setBody] = useState(phrase?.content ?? "");
  // xpando:richtext — `body` holds HTML in rich mode and plain text in plain mode.
  const originalMode: BodyMode = phrase?.kind === "html" ? "rich" : "plain";
  const [mode, setMode] = useState<BodyMode>(originalMode);
  const [switches, setSwitches] = useState(0);
  const [opts, setOpts] = useState<EditorOptions>(baseOptions);
  const [termDraft, setTermDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Which Save button was used last, so the save error appears next to it.
  const [saveFrom, setSaveFrom] = useState<"top" | "bottom">("top");
  const [stale, setStale] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const firstTrigger = useRef<HTMLInputElement>(null);
  const [advanced] = useAdvanced();
  // "More options" is closed until asked for; Advanced mode opens it, since that is what it is for.
  const [moreOpen, setMoreOpen] = useState(false);
  useEffect(() => {
    if (advanced) setMoreOpen(true);
  }, [advanced]);
  const uid = useId();
  const richRef = useRef<RichHandle | null>(null); // xpando:richtext — insert at the cursor

  useEffect(() => {
    firstTrigger.current?.focus();
  }, []);

  const triggersEditable = !phrase || !phrase.is_regex;
  const bodyEditable = !phrase || phrase.kind === "replace" || phrase.kind === "html"; // xpando:vars, xpando:richtext
  const bodyNote = !phrase
    ? null
    : phrase.kind !== "replace"
      ? `This phrase's text is stored as ${phrase.kind}. It can't be edited here yet; the text is shown read-only.`
      : null;
  // xpando:vars — the Variables editor and Insert macro menu (see VarsEditor.tsx).
  const vars = useVars({
    file,
    index: phrase ? phrase.index : null,
    hasVars: (phrase?.vars.length ?? 0) > 0,
    body,
    setBody,
    bodyId: `${uid}-body`,
    richRef,
    bodyEditable,
    bodyIsHtml: mode === "rich",
    onBodyReplaced: () => setSwitches((n) => n + 1), // the rich text editor starts again from the new body
  });

  const { problems, warnings } = useMemo(() => {
    const problems: string[] = [];
    const warnings: string[] = [];
    if (triggersEditable) {
      if (triggers.some((t) => t.trim() === "")) problems.push("Every trigger needs text.");
      const seen = new Set<string>();
      for (const t of triggers) {
        if (t.trim() === "") continue;
        if (seen.has(t)) problems.push(`The trigger “${t}” is listed twice.`);
        seen.add(t);
        const inFile = folder?.phrases.some(
          (p) => !p.is_regex && p.index !== phrase?.index && p.triggers.includes(t),
        );
        if (inFile) problems.push(`The trigger “${t}” is already used by another phrase in this file.`);
        const elsewhere = folders
          .filter((f) => f.file !== file && f.phrases.some((p) => !p.is_regex && p.triggers.includes(t)))
          .map((f) => f.name);
        if (elsewhere.length > 0) {
          warnings.push(
            `“${t}” is also used in ${elsewhere.join(", ")}. espanso may expand either one.`,
          );
        }
      }
    }
    if (bodyEditable && (mode === "rich" ? isBlankHtml(body) : body.trim() === "")) {
      problems.push("Enter the text to insert."); // xpando:richtext
    }
    if (termDraft.trim() !== "") {
      problems.push("Press Add to keep the search term you typed, or clear it.");
    }
    // xpando:vars
    return {
      problems: [...problems, ...vars.problems],
      warnings: [...warnings, ...vars.warnings],
    };
  }, [
    triggers,
    body,
    bodyEditable,
    triggersEditable,
    folder,
    folders,
    file,
    phrase,
    termDraft,
    mode,
    vars.problems,
    vars.warnings,
  ]);

  // What is set under "More options", so a closed disclosure still says what is inside.
  const moreSummary = [
    description.trim() !== "" ? "description" : null,
    opts.search_terms.length > 0
      ? `${opts.search_terms.length} search ${opts.search_terms.length === 1 ? "term" : "terms"}`
      : null,
  ]
    .filter(Boolean)
    .join(", ");
  // Options that only Advanced mode shows: still kept and saved, so say they exist.
  const hiddenOptions = [
    opts.left_word ? "start-of-word matching" : null,
    opts.right_word ? "end-of-word matching" : null,
    opts.uppercase_style ? "a capitals style" : null,
    opts.force_mode ? "a chosen way of inserting the text" : null,
  ].filter((x): x is string => x !== null);

  const optionsChanged = optionsKey(opts) !== optionsKey(baseOptions);
  const descriptionChanged = description !== (phrase?.comment ?? "");
  const dirty = phrase
    ? !sameList(triggers, phrase.triggers) ||
      label !== (phrase.label ?? "") ||
      (bodyEditable && body !== phrase.content) ||
      descriptionChanged ||
      optionsChanged ||
      mode !== originalMode || // xpando:richtext
      vars.dirty // xpando:vars
    : triggers.length > 1 ||
      triggers.some((t) => t !== "") ||
      label !== "" ||
      body !== "" ||
      description !== "" ||
      optionsChanged ||
      mode !== originalMode || // xpando:richtext
      vars.dirty; // xpando:vars

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(() => () => onDirtyChange(false), [onDirtyChange]);

  // Close the window or reload the page: the browser asks first while there is a draft.
  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);

  function setTrigger(i: number, value: string) {
    setTriggers((ts) => ts.map((t, j) => (j === i ? value : t)));
  }

  function patchOptions(p: Partial<EditorOptions>) {
    setOpts((o) => ({ ...o, ...p }));
  }

  function addTerm() {
    const t = termDraft.trim();
    if (t === "") return;
    if (!opts.search_terms.includes(t)) patchOptions({ search_terms: [...opts.search_terms, t] });
    setTermDraft("");
  }

  async function save(from: "top" | "bottom" = "top") {
    if (!folder || problems.length > 0 || saving || stale) return;
    setSaveFrom(from);
    setSaving(true);
    setError(null);
    const labelOut = label === (phrase?.label ?? "") ? (phrase?.label ?? null) : label.trim() || null;
    try {
      const rich = mode === "rich"; // xpando:richtext
      const comment = !phrase || !descriptionChanged
        ? "Keep"
        : description.trim() === ""
          ? "Clear"
          : { Set: description };
      // xpando:richtext — editing an html phrase, or switching between plain and rich text.
      if (phrase && (originalMode === "rich" || mode !== originalMode)) {
        const switched = mode !== originalMode;
        let saved = await invoke<EditorFolder>("save_rich_phrase", {
          file,
          index: phrase.index,
          edit: {
            triggers,
            label: labelOut,
            body: rich ? null : body,
            comment,
            options: optionsChanged ? opts : null,
          },
          convert: switched ? (rich ? "html" : "plain") : null,
          html: rich && (switched || body !== phrase.content) ? body : null,
          expectedHash: folder.hash,
        });
        // The variables go in a second write (the rich-text command does not carry them);
        // it starts from the hash the first write returned.
        if (vars.dirty) {
          saved = await invoke<EditorFolder>("save_phrase_vars", {
            file,
            index: phrase.index,
            change: { vars: vars.specs, body: null },
            edit: { triggers, label: labelOut, body: null, comment: "Keep", options: null },
            expectedHash: saved.hash,
          });
        }
        onSaved(saved, phrase.index);
      } else if (!phrase && rich) {
        let saved = await invoke<EditorFolder>("create_rich_phrase", {
          file,
          phrase: {
            triggers,
            label: labelOut,
            body: "",
            comment: description.trim() === "" ? null : description,
            options: opts,
          },
          html: body,
          expectedHash: folder.hash,
        });
        const newIndex = Math.max(...saved.phrases.map((p) => p.index));
        if (vars.dirty && vars.specs) {
          saved = await invoke<EditorFolder>("save_phrase_vars", {
            file,
            index: newIndex,
            change: { vars: vars.specs, body: null },
            edit: { triggers, label: labelOut, body: null, comment: "Keep", options: null },
            expectedHash: saved.hash,
          });
        }
        onSaved(saved, newIndex);
      } else if (phrase) {
        const edit = {
          triggers,
          label: labelOut,
          body: bodyEditable ? body : null,
          comment,
          options: optionsChanged ? opts : null,
        };
        // xpando:vars — when the variables changed, text and variables go in one write.
        const saved = vars.dirty
          ? await invoke<EditorFolder>("save_phrase_vars", {
              file,
              index: phrase.index,
              change: { vars: vars.specs, body: bodyEditable ? body : null },
              edit: { ...edit, body: null },
              expectedHash: folder.hash,
            })
          : await invoke<EditorFolder>("save_phrase", {
              file,
              index: phrase.index,
              edit,
              expectedHash: folder.hash,
            });
        onSaved(saved, phrase.index);
      } else {
        const newPhrase = {
          triggers,
          label: labelOut,
          body,
          comment: description.trim() === "" ? null : description,
          options: opts,
        };
        const saved =
          vars.dirty && vars.specs
            ? await invoke<EditorFolder>("create_phrase_with_vars", {
                file,
                phrase: newPhrase,
                variables: vars.specs,
                expectedHash: folder.hash,
              })
            : await invoke<EditorFolder>("create_phrase", {
                file,
                phrase: newPhrase,
                expectedHash: folder.hash,
              });
        onSaved(saved, Math.max(...saved.phrases.map((p) => p.index)));
      }
    } catch (err) {
      setError(errorText(err));
      setSaving(false);
    }
  }

  // Ctrl+S saves from anywhere in the window while no dialog is open.
  const saveRef = useRef(save);
  saveRef.current = save;
  const varsRef = useRef(vars);
  varsRef.current = vars;
  // A macro name still being typed is applied first; the save then waits for the next render,
  // so it sees the renamed macro and text.
  const [saveSoon, setSaveSoon] = useState(false);
  useEffect(() => {
    if (!saveSoon) return;
    setSaveSoon(false);
    void saveRef.current();
  }, [saveSoon]);
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (document.querySelector("dialog[open]")) return;
        const name = varsRef.current.commitPendingName();
        if (name === "problem") return; // the name box says why; nothing is saved
        if (name === "applied") setSaveSoon(true);
        else void saveRef.current();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  /** Reload after a conflict, keeping the draft. */
  async function reload() {
    const fresh = await onReload();
    if (!fresh) return;
    setError(null);
    setSaving(false);
    const f = fresh.find((x) => x.file === file);
    if (!f) {
      setStale("The file this phrase belongs to is gone.");
      return;
    }
    if (phrase) {
      const live = f.phrases.find((p) => p.index === phrase.index);
      if (!live) {
        setStale("This phrase is no longer in the file.");
      } else if (!samePhrase(live, phrase)) {
        setStale("This phrase was changed in the file after you started editing.");
      }
    }
    // Otherwise the entry is unchanged on disk: the draft stays and can be saved again.
  }

  async function copyDraft() {
    try {
      await navigator.clipboard.writeText(draftText(triggers, label, description, body, opts));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setError("Could not copy. Select the text above and copy it by hand.");
    }
  }

  function onFormKeyDown(e: KeyboardEvent<HTMLFormElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
      // Enter in a single-line field never saves; Ctrl+S or the Save button does.
      e.preventDefault();
    }
  }

  /** One checkbox of "When it expands", with its longer help one click away. */
  function flagRow({ key, label: text, help }: (typeof FLAGS)[number]) {
    return (
      <div className="opt-row" key={key}>
        <input
          type="checkbox"
          id={`${uid}-${key}`}
          checked={opts[key] === true}
          onChange={(e) => {
            const on = e.target.checked;
            const off = baseOptions[key] === false ? false : null;
            if (key === "propagate_case") {
              patchOptions({ propagate_case: on ? true : off, ...(on ? {} : { uppercase_style: null }) });
            } else {
              patchOptions({ [key]: on ? true : off });
            }
          }}
        />
        <label htmlFor={`${uid}-${key}`}>
          {text}
          {advanced && <KeyName name={key} />}
        </label>
        <HelpTip label={text}>{help}</HelpTip>
      </div>
    );
  }

  const staleView = stale !== null;
  const saveDisabled = saving || problems.length > 0 || (!!phrase && !dirty);

  if (staleView) {
    return (
      <div className="editor" role="region" aria-label="Unsaved draft">
        <h2 className="detail-title">{phrase ? `Draft for “${phraseName(phrase)}”` : "Draft for a new phrase"}</h2>
        <div className="editor-error" role="alert">
          <AlertTriangle size={16} aria-hidden="true" />
          <span className="grow">
            {stale} Your unsaved draft is kept below, read-only, so nothing you typed is lost. Copy it, then close
            this to see the current version.
          </span>
        </div>
        <div className="draft-copy">
          <pre aria-label="Your draft">{draftText(triggers, label, description, body, opts)}</pre>
          <div className="editor-buttons">
            <button type="button" className="btn btn-primary" onClick={() => void copyDraft()}>
              {copied ? <Check size={16} aria-hidden="true" /> : <Copy size={16} aria-hidden="true" />}
              {copied ? "Copied" : "Copy draft"}
            </button>
            <button type="button" className="btn" onClick={onCancel}>
              Close draft
            </button>
          </div>
          {error && (
            <p className="editor-note" role="alert">
              <Info size={16} aria-hidden="true" />
              {error}
            </p>
          )}
        </div>
      </div>
    );
  }

  return (
    <form
      className="editor"
      onSubmit={(e) => {
        e.preventDefault();
        const submitter = (e.nativeEvent as SubmitEvent).submitter as HTMLElement | null;
        void save(submitter?.dataset.where === "bottom" ? "bottom" : "top");
      }}
      onKeyDown={onFormKeyDown}
      aria-label={phrase ? "Edit phrase" : "New phrase"}
    >
      <h2 className="detail-title">{phrase ? `Edit “${phraseName(phrase)}”` : "New phrase"}</h2>
      <p className="detail-meta editor-where">
        {phrase
          ? `In ${advanced ? (folder?.file ?? file) : (folder?.name ?? file)}`
          : `Will be added to ${advanced ? (folder?.file ?? file) : (folder?.name ?? file)}`}
      </p>

      <div className="editor-topbar" role="toolbar" aria-label="Save or cancel this phrase">
        <div className="editor-buttons">
          <button
            type="submit"
            className="btn btn-primary"
            data-where="top"
            disabled={saveDisabled}
            aria-describedby={problems.length > 0 ? `${uid}-topbar-note` : undefined}
            title="Save (Ctrl+S)"
          >
            <Save size={16} aria-hidden="true" />
            {saving ? "Saving…" : "Save"}
          </button>
          <button type="button" className="btn" onClick={onCancel} disabled={saving} title="Cancel (Esc)">
            Cancel
          </button>
          {dirty && (
            <span className="editor-unsaved">
              <span className="editor-unsaved-dot" aria-hidden="true" />
              Unsaved changes
            </span>
          )}
        </div>
        {problems.length > 0 && (
          <p className="editor-topbar-note" id={`${uid}-topbar-note`}>
            <AlertTriangle size={16} aria-hidden="true" />
            <span>
              Can't save yet: {problems[0]}
              {problems.length > 1 ? ` (and ${problems.length - 1} more below)` : ""}
            </span>
          </p>
        )}
        {error && saveFrom === "top" && <ErrorMessage message={error} onReload={() => void reload()} />}
      </div>

      <div className="editor-panel">
        <div className="editor-group" role="group" aria-labelledby={`${uid}-triggers`}>
          <span className="editor-legend" id={`${uid}-triggers`}>
            {phrase?.is_regex ? "Regex trigger" : triggers.length > 1 ? "Triggers" : "Trigger"}
          </span>
          {triggers.map((t, i) => (
            <div className="trigger-row" key={i}>
              <input
                ref={i === 0 ? firstTrigger : undefined}
                className="field mono"
                aria-label={`Trigger ${i + 1}`}
                value={t}
                readOnly={!triggersEditable}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => setTrigger(i, e.target.value)}
              />
              {triggersEditable && (
                <button
                  type="button"
                  className="btn btn-icon"
                  aria-label={`Remove trigger ${i + 1}`}
                  title="Remove trigger"
                  disabled={triggers.length === 1}
                  onClick={() => setTriggers((ts) => ts.filter((_, j) => j !== i))}
                >
                  <X size={16} aria-hidden="true" />
                </button>
              )}
            </div>
          ))}
          {triggersEditable ? (
            <div>
              <button type="button" className="btn btn-quiet" onClick={() => setTriggers((ts) => [...ts, ""])}>
                <Plus size={16} aria-hidden="true" />
                Add another trigger
              </button>
            </div>
          ) : (
            <p className="editor-note">
              <Info size={16} aria-hidden="true" />
              Regex triggers can't be edited here yet.
            </p>
          )}
        </div>

        <div className="editor-group">
          <label htmlFor={`${uid}-label`}>
            Name <span className="editor-optional">(optional)</span>
          </label>
          <input
            id={`${uid}-label`}
            className="field"
            value={label}
            autoComplete="off"
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>

        <div className="editor-group">
          <div className="rt-labelrow">
            <label htmlFor={`${uid}-body`} id={`${uid}-body-label`}>
              Text
            </label>
            {bodyEditable && (
              <BodyModeSwitch
                mode={mode}
                body={body}
                onSwitch={(m, b) => {
                  setMode(m);
                  setBody(b);
                  setSwitches((n) => n + 1);
                }}
              />
            )}
          </div>
          {vars.menu /* xpando:vars (inserts into whichever editor is showing) */}
          {mode === "rich" ? (
            <RichBodyEditor
              // A new editor after each switch, so it starts from the converted body.
              key={switches}
              html={body}
              onChange={setBody}
              labelledBy={`${uid}-body-label`}
              handleRef={richRef}
            />
          ) : bodyEditable ? (
            <textarea
              id={`${uid}-body`}
              className="field mono"
              rows={8}
              value={body}
              spellCheck={false}
              onChange={(e) => setBody(e.target.value)}
            />
          ) : (
            <>
              <pre className="content-text" id={`${uid}-body`}>
                {phrase?.content}
              </pre>
              {bodyNote && (
                <p className="editor-note">
                  <Info size={16} aria-hidden="true" />
                  {bodyNote}
                </p>
              )}
            </>
          )}
        </div>

        {vars.section /* xpando:vars */}

        <div className="editor-group" role="group" aria-labelledby={`${uid}-when`}>
          <span className="editor-legend" id={`${uid}-when`}>
            When it expands
          </span>
          {FLAGS.filter((f) => advanced || !f.advanced).map(flagRow)}
          {advanced && (
            <div className="opt-select opt-indent">
              <label htmlFor={`${uid}-upper`}>
                Capitals style <KeyName name="uppercase_style" />
              </label>
              <select
                id={`${uid}-upper`}
                className="field"
                value={opts.uppercase_style ?? ""}
                disabled={opts.propagate_case !== true}
                aria-describedby={`${uid}-upper-help`}
                onChange={(e) => patchOptions({ uppercase_style: e.target.value || null })}
              >
                {opts.uppercase_style && !UPPERCASE_STYLES.some(([v]) => v === opts.uppercase_style) && (
                  <option value={opts.uppercase_style}>{opts.uppercase_style} (not recognised)</option>
                )}
                {UPPERCASE_STYLES.map(([v, text]) => (
                  <option key={v} value={v}>
                    {text}
                  </option>
                ))}
              </select>
              <p className="opt-help" id={`${uid}-upper-help`}>
                How the text is capitalised when you type the trigger in capitals. Needs “Match the capitalization I
                type”.
              </p>
            </div>
          )}
        </div>

        <details
          className="editor-more"
          open={moreOpen}
          onToggle={(e) => setMoreOpen(e.currentTarget.open)}
        >
          <summary>
            <ChevronRight size={16} aria-hidden="true" className="editor-more-chev" />
            More options
            {moreSummary && <span className="editor-more-sum">{moreSummary}</span>}
          </summary>
          <div className="editor-more-body">
            <div className="editor-group">
              <label htmlFor={`${uid}-desc`}>Description</label>
              <textarea
                id={`${uid}-desc`}
                className="field"
                rows={2}
                value={description}
                aria-describedby={`${uid}-desc-help`}
                onChange={(e) => setDescription(e.target.value)}
              />
              <p className="editor-note" id={`${uid}-desc-help`}>
                Notes for yourself.{advanced && " Saved as a # comment above the phrase in the file."}
              </p>
            </div>

            <div className="editor-group" role="group" aria-labelledby={`${uid}-terms`}>
              <span className="editor-legend" id={`${uid}-terms`}>
                Search terms
              </span>
              <p className="editor-note">Extra words that find this phrase when you search. They don't change expansion.</p>
              {opts.search_terms.length > 0 && (
                <ul className="term-list">
                  {opts.search_terms.map((t) => (
                    <li key={t}>
                      <span className="chip grow">{t}</span>
                      <button
                        type="button"
                        className="btn btn-icon"
                        aria-label={`Remove search term ${t}`}
                        title="Remove search term"
                        onClick={() => patchOptions({ search_terms: opts.search_terms.filter((x) => x !== t) })}
                      >
                        <X size={16} aria-hidden="true" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="term-row">
                <input
                  className="field"
                  aria-label="New search term"
                  value={termDraft}
                  autoComplete="off"
                  onChange={(e) => setTermDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      addTerm();
                    }
                  }}
                />
                <button type="button" className="btn" onClick={addTerm} disabled={termDraft.trim() === ""}>
                  <Plus size={16} aria-hidden="true" />
                  Add
                </button>
              </div>
            </div>

            {!advanced && hiddenOptions.length > 0 && (
              <p className="editor-note">
                <Info size={16} aria-hidden="true" />
                This phrase also uses {listWords(hiddenOptions)}. Turn on Advanced to change{" "}
                {hiddenOptions.length === 1 ? "it" : "them"}.
              </p>
            )}

            {advanced && (
              <div className="editor-group editor-adv" role="group" aria-labelledby={`${uid}-adv`}>
                <span className="editor-legend" id={`${uid}-adv`}>
                  Insertion
                </span>
                <div className="opt-select">
                  <label htmlFor={`${uid}-mode`}>
                    How the text is inserted <KeyName name="force_mode" />
                  </label>
                  <select
                    id={`${uid}-mode`}
                    className="field"
                    value={opts.force_mode ?? ""}
                    aria-describedby={`${uid}-mode-help`}
                    onChange={(e) => patchOptions({ force_mode: e.target.value || null })}
                  >
                    {opts.force_mode && !FORCE_MODES.some(([v]) => v === opts.force_mode) && (
                      <option value={opts.force_mode}>{opts.force_mode} (not recognised)</option>
                    )}
                    {FORCE_MODES.map(([v, text]) => (
                      <option key={v} value={v}>
                        {text}
                      </option>
                    ))}
                  </select>
                  <p className="opt-help" id={`${uid}-mode-help`}>
                    Automatic is best. Choose paste if typing drops characters, or typing if an app blocks paste.
                  </p>
                </div>
              </div>
            )}
          </div>
        </details>
      </div>

      {problems.length > 1 && (
        <ul className="editor-problems" aria-live="polite">
          {problems.map((p, i) => (
            <li key={i}>{p}</li>
          ))}
        </ul>
      )}
      {warnings.length > 0 && (
        <ul className="editor-warnings" aria-live="polite">
          {warnings.map((w, i) => (
            <li key={i}>
              <AlertTriangle size={16} aria-hidden="true" />
              {w}
            </li>
          ))}
        </ul>
      )}
      {error && saveFrom === "bottom" && <ErrorMessage message={error} onReload={() => void reload()} />}

      {advanced && (
        <div className="editor-buttons" role="group" aria-label="Save or cancel at the end of the form">
          <button
            type="submit"
            className="btn btn-primary"
            data-where="bottom"
            disabled={saveDisabled}
            title="Save (Ctrl+S)"
          >
            <Save size={16} aria-hidden="true" />
            {saving ? "Saving…" : "Save"}
          </button>
          <button type="button" className="btn" onClick={onCancel} disabled={saving} title="Cancel (Esc)">
            Cancel
          </button>
        </div>
      )}
    </form>
  );
}

interface ActionsProps {
  folder: EditorFolder;
  phrase: EditorPhrase;
  onEdit: () => void;
  /** Called with the reloaded folder after the phrase was deleted. */
  onDeleted: (folder: EditorFolder) => void;
  onReload: () => Promise<unknown>;
}

/** Edit and Delete buttons for the selected phrase, and the delete confirmation. */
export function PhraseActions({ folder, phrase, onEdit, onDeleted, onReload }: ActionsProps) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="detail-actions">
      <button type="button" className="btn btn-primary" onClick={onEdit}>
        <Pencil size={16} aria-hidden="true" />
        Edit
      </button>
      <button type="button" className="btn" onClick={() => setConfirming(true)}>
        <Trash2 size={16} aria-hidden="true" />
        Delete
      </button>
      {confirming && (
        <ConfirmDelete
          folder={folder}
          phrase={phrase}
          onClose={() => setConfirming(false)}
          onDeleted={onDeleted}
          onReload={onReload}
        />
      )}
    </div>
  );
}

function ConfirmDelete({
  folder,
  phrase,
  onClose,
  onDeleted,
  onReload,
}: {
  folder: EditorFolder;
  phrase: EditorPhrase;
  onClose: () => void;
  onDeleted: (folder: EditorFolder) => void;
  onReload: () => Promise<unknown>;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const titleId = useId();
  const [advanced] = useAdvanced();

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  async function remove() {
    setBusy(true);
    setError(null);
    try {
      const saved = await invoke<EditorFolder>("delete_phrase", {
        file: folder.file,
        index: phrase.index,
        expectedHash: folder.hash,
      });
      onDeleted(saved);
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  }

  return (
    <dialog ref={ref} className="confirm" aria-labelledby={titleId} onClose={onClose}>
      <h2 id={titleId} className="confirm-title">
        Delete “{phraseName(phrase)}”?
      </h2>
      <p>
        This removes the phrase{" "}
        {phrase.triggers.map((t) => (
          <span className="chip" key={t}>
            {t}
          </span>
        ))}{" "}
        from “{folder.name}”. A backup copy of the file is kept.
      </p>
      {advanced && (
        <ul className="confirm-list">
          <li>File: {folder.file}</li>
          {phrase.line !== null && <li>The entry starting at line {phrase.line}.</li>}
          {phrase.comment && (
            <li>
              The comment above it: <q className="confirm-comment">{phrase.comment}</q>
            </li>
          )}
          <li>Nothing else in the file changes. The one-time backup is {folder.file}.bak.</li>
        </ul>
      )}
      {error && <ErrorMessage message={error} onReload={() => void onReload().then(onClose)} />}
      <div className="editor-buttons">
        <button type="button" className="btn btn-danger" onClick={remove} disabled={busy}>
          <Trash2 size={16} aria-hidden="true" />
          {busy ? "Deleting…" : "Delete phrase"}
        </button>
        <button type="button" className="btn" autoFocus onClick={() => ref.current?.close()} disabled={busy}>
          Cancel
        </button>
      </div>
    </dialog>
  );
}
