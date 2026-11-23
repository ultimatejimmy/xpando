import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Info, Pencil, Plus, RefreshCw, Save, Trash2, X } from "lucide-react";
import "./editor.css";

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

interface EditorProps {
  folders: EditorFolder[];
  /** The file the phrase lives in, or the file a new phrase is added to. */
  file: string;
  /** The phrase being edited, or null to create one. */
  phrase: EditorPhrase | null;
  /** Called with the reloaded folder and the index of the phrase to select. */
  onSaved: (folder: EditorFolder, select: number) => void;
  onCancel: () => void;
  /** Reloads every folder from disk; the editor is closed afterwards. */
  onReload: () => Promise<void>;
}

export function PhraseEditor({ folders, file, phrase, onSaved, onCancel, onReload }: EditorProps) {
  const folder = folders.find((f) => f.file === file);
  const [triggers, setTriggers] = useState<string[]>(phrase ? phrase.triggers : [""]);
  const [label, setLabel] = useState(phrase?.label ?? "");
  const [body, setBody] = useState(phrase?.content ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const firstTrigger = useRef<HTMLInputElement>(null);
  const uid = useId();

  useEffect(() => {
    firstTrigger.current?.focus();
  }, []);

  const triggersEditable = !phrase || !phrase.is_regex;
  const bodyEditable = !phrase || (phrase.kind === "replace" && phrase.vars.length === 0);
  const bodyNote = !phrase
    ? null
    : phrase.kind !== "replace"
      ? `This phrase's text is stored as ${phrase.kind}. It can't be edited here yet; the text is shown read-only.`
      : phrase.vars.length > 0
        ? "This phrase uses variables. Its text can't be edited here yet, so the variables stay intact."
        : null;

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
    if (bodyEditable && body.trim() === "") problems.push("Enter the text to insert.");
    return { problems, warnings };
  }, [triggers, body, bodyEditable, triggersEditable, folder, folders, file, phrase]);

  const changed =
    !phrase ||
    !sameList(triggers, phrase.triggers) ||
    label !== (phrase.label ?? "") ||
    (bodyEditable && body !== phrase.content);

  function setTrigger(i: number, value: string) {
    setTriggers((ts) => ts.map((t, j) => (j === i ? value : t)));
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    if (!folder || problems.length > 0 || saving) return;
    setSaving(true);
    setError(null);
    const labelOut = label === (phrase?.label ?? "") ? (phrase?.label ?? null) : label.trim() || null;
    try {
      if (phrase) {
        const saved = await invoke<EditorFolder>("save_phrase", {
          file,
          index: phrase.index,
          edit: { triggers, label: labelOut, body: bodyEditable ? body : null },
          expectedHash: folder.hash,
        });
        onSaved(saved, phrase.index);
      } else {
        const saved = await invoke<EditorFolder>("create_phrase", {
          file,
          phrase: { triggers, label: labelOut, body },
          expectedHash: folder.hash,
        });
        onSaved(saved, Math.max(...saved.phrases.map((p) => p.index)));
      }
    } catch (err) {
      setError(errorText(err));
      setSaving(false);
    }
  }

  async function reload() {
    await onReload();
    onCancel();
  }

  return (
    <form
      className="editor"
      onSubmit={save}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      aria-label={phrase ? "Edit phrase" : "New phrase"}
    >
      <h2 className="detail-title">{phrase ? `Edit “${phraseName(phrase)}”` : "New phrase"}</h2>
      <p className="detail-meta editor-where">
        {phrase ? `In ${folder?.file ?? file}` : `Will be added to the end of ${folder?.file ?? file}`}
      </p>

      <fieldset className="editor-group">
        <legend>{phrase?.is_regex ? "Regex trigger" : triggers.length > 1 ? "Triggers" : "Trigger"}</legend>
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
          <button type="button" className="btn" onClick={() => setTriggers((ts) => [...ts, ""])}>
            <Plus size={16} aria-hidden="true" />
            Add trigger
          </button>
        ) : (
          <p className="editor-note">
            <Info size={16} aria-hidden="true" />
            Regex triggers can't be edited here yet.
          </p>
        )}
      </fieldset>

      <div className="editor-group">
        <label htmlFor={`${uid}-label`}>Name (label)</label>
        <input
          id={`${uid}-label`}
          className="field"
          value={label}
          autoComplete="off"
          onChange={(e) => setLabel(e.target.value)}
        />
      </div>

      <div className="editor-group">
        <label htmlFor={`${uid}-body`}>Text</label>
        {bodyEditable ? (
          <textarea
            id={`${uid}-body`}
            className="field mono"
            rows={10}
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

      {phrase?.comment && (
        <p className="editor-note">
          <Info size={16} aria-hidden="true" />
          The comment above this phrase in the file is kept as it is.
        </p>
      )}

      {problems.length > 0 && (
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
      {error && <ErrorMessage message={error} onReload={reload} />}

      <div className="editor-buttons">
        <button type="submit" className="btn btn-primary" disabled={saving || problems.length > 0 || !changed}>
          <Save size={16} aria-hidden="true" />
          {saving ? "Saving…" : "Save"}
        </button>
        <button type="button" className="btn" onClick={onCancel} disabled={saving}>
          Cancel
        </button>
      </div>
    </form>
  );
}

interface ActionsProps {
  folder: EditorFolder;
  phrase: EditorPhrase;
  onEdit: () => void;
  /** Called with the reloaded folder after the phrase was deleted. */
  onDeleted: (folder: EditorFolder) => void;
  onReload: () => Promise<void>;
}

/** Edit and Delete buttons for the selected phrase, and the delete confirmation. */
export function PhraseActions({ folder, phrase, onEdit, onDeleted, onReload }: ActionsProps) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="detail-actions">
      <button type="button" className="btn" onClick={onEdit}>
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
  onReload: () => Promise<void>;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const titleId = useId();

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
      <p>This removes the phrase from {folder.file}:</p>
      <ul className="confirm-list">
        <li>
          {phrase.triggers.length > 1 ? "Triggers" : "Trigger"}:{" "}
          {phrase.triggers.map((t) => (
            <span className="chip" key={t}>
              {t}
            </span>
          ))}
        </li>
        {phrase.line !== null && <li>The entry starting at line {phrase.line}.</li>}
        {phrase.comment && (
          <li>
            The comment above it: <q className="confirm-comment">{phrase.comment}</q>
          </li>
        )}
      </ul>
      <p>
        Nothing else in the file changes. xpando keeps a one-time backup of the file as {folder.file}.bak.
      </p>
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
