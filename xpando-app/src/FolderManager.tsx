import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, FolderPlus, Pencil, Plus, Trash2, X } from "lucide-react";

/** Mirrors `xpando_core::folders`. */
export type ScopeKind = "all" | "only" | "except" | "nowhere";

export interface AppScope {
  config: string;
  name: string;
  label: string | null;
  exe_names: string[];
  exec_pattern: string | null;
  exec_simple: boolean;
  title_pattern: string | null;
  title_text: string | null;
  class_pattern: string | null;
  os: string | null;
}

export interface FolderInfo {
  file: string;
  name: string;
  scope: { kind: ScopeKind; apps: AppScope[] };
  locked: string | null;
  scope_locked: string | null;
  configs: string[];
  imported_by: string[];
}

interface Change {
  file: string;
  action: "create" | "edit" | "rename" | "delete";
  detail: string;
  backup: string | null;
}

interface Outcome {
  changes: Change[];
  file: string | null;
}

type TitleChange = { kind: "keep" } | { kind: "clear" } | { kind: "contains"; text: string };
type ScopeSpec = { kind: "all" } | { kind: "apps"; exes: string[] | null; title: TitleChange };

/** The part of a folder the UI already has: its file, name, hash and phrase count. */
export interface FolderRef {
  name: string;
  file: string;
  hash: string;
  phrases: number;
}

const CONFLICT_PREFIX = "file-changed:";

function errorText(e: unknown): string {
  const s = String(e);
  return s.startsWith(CONFLICT_PREFIX) ? s.slice(CONFLICT_PREFIX.length).trim() : s;
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function appName(a: AppScope): string {
  return a.label || capitalize(a.name);
}

function joinNames(apps: AppScope[]): string {
  const names = apps.map(appName);
  return names.length <= 2 ? names.join(", ") : `${names.slice(0, 2).join(", ")} +${names.length - 2}`;
}

/** Short plain-language scope: "All apps", "Word only", "All apps except Word", "Not used". */
export function scopeLabel(info: FolderInfo | undefined): string {
  if (!info) return "";
  const { kind, apps } = info.scope;
  if (kind === "all") return "All apps";
  if (kind === "only") return `${joinNames(apps)} only`;
  if (kind === "except") return `All apps except ${joinNames(apps)}`;
  return "Not used";
}

/** Loads scope information for every folder again whenever `version` changes. */
export function useFolderInfo(version: unknown): Record<string, FolderInfo> {
  const [infos, setInfos] = useState<Record<string, FolderInfo>>({});
  useEffect(() => {
    let live = true;
    invoke<FolderInfo[]>("list_folder_info")
      .then((list) => {
        if (live) setInfos(Object.fromEntries(list.map((i) => [i.file, i])));
      })
      .catch(() => {
        if (live) setInfos({});
      });
    return () => {
      live = false;
    };
  }, [version]);
  return infos;
}

/** The small scope text under a folder's name in the sidebar. */
export function ScopeLabel({ info }: { info: FolderInfo | undefined }) {
  if (!info) return null;
  const warn = info.scope.kind === "nowhere";
  return (
    <span
      className={warn ? "fm-scope fm-scope-warn" : "fm-scope"}
      title={warn ? "No settings file loads this folder, so its phrases never expand." : undefined}
    >
      {scopeLabel(info)}
    </span>
  );
}

// ---------------------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------------------

function useDialog(onClose: () => void) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);
  return { ref, close: () => ref.current?.close(), onClose };
}

/** Runs `check` shortly after `deps` change; `null` result means "no preview available". */
function usePreview(check: () => Promise<Change[]> | null, deps: unknown[]) {
  const [changes, setChanges] = useState<Change[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const run = check();
    if (!run) {
      setChanges(null);
      setError(null);
      return;
    }
    const timer = setTimeout(() => {
      run
        .then((c) => {
          if (live) {
            setChanges(c);
            setError(null);
          }
        })
        .catch((e) => {
          if (live) {
            setChanges(null);
            setError(errorText(e));
          }
        });
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { changes, error };
}

function ChangeList({ changes }: { changes: Change[] }) {
  return (
    <ul className="fm-changes">
      {changes.map((c, i) => (
        <li key={`${c.file}-${i}`}>
          <code>{c.file}</code> <span className="fm-action">{ACTION_TEXT[c.action]}</span>
          {c.backup && (
            <>
              {" "}
              Backup: <code>{c.backup}</code>
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

const ACTION_TEXT: Record<Change["action"], string> = {
  create: "will be created.",
  edit: "will be edited.",
  rename: "will be renamed.",
  delete: "will be deleted.",
};

function Problem({ message, onReload }: { message: string; onReload?: () => void }) {
  return (
    <p className="fm-problem" role="alert">
      {message}
      {onReload && (
        <>
          {" "}
          <button type="button" className="btn" onClick={onReload}>
            Reload
          </button>
        </>
      )}
    </p>
  );
}

function Dialog({
  title,
  labelId,
  dialog,
  children,
}: {
  title: string;
  labelId: string;
  dialog: ReturnType<typeof useDialog>;
  children: ReactNode;
}) {
  return (
    <dialog ref={dialog.ref} className="fm-dialog" aria-labelledby={labelId} onClose={dialog.onClose}>
      <h2 id={labelId} className="fm-title">
        {title}
      </h2>
      {children}
    </dialog>
  );
}

// ---------------------------------------------------------------------------------------
// New folder
// ---------------------------------------------------------------------------------------

export function NewFolderButton({ onDone }: { onDone: (file: string | null) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="btn fm-new" onClick={() => setOpen(true)}>
        <FolderPlus size={16} aria-hidden="true" />
        New folder
      </button>
      {open && <NewFolderDialog onClose={() => setOpen(false)} onDone={onDone} />}
    </>
  );
}

function NewFolderDialog({ onClose, onDone }: { onClose: () => void; onDone: (file: string | null) => void }) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const fieldId = useId();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const out = await invoke<Outcome>("create_folder", { name });
      onDone(out.file);
      dialog.close();
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  }

  return (
    <Dialog title="New folder" labelId={titleId} dialog={dialog}>
      <form onSubmit={submit} className="fm-form">
        <label htmlFor={fieldId}>Folder name</label>
        <input
          id={fieldId}
          className="field"
          value={name}
          autoFocus
          autoComplete="off"
          onChange={(e) => setName(e.target.value)}
          aria-describedby={`${fieldId}-help`}
        />
        <p id={`${fieldId}-help`} className="fm-help">
          Letters, digits, spaces, “-” and “_”. The folder starts empty and applies to all apps; use “Apps…” to
          limit it.
        </p>
        {error && <Problem message={error} />}
        <div className="fm-buttons">
          <button type="submit" className="btn btn-primary" disabled={busy || !name.trim()}>
            <Plus size={16} aria-hidden="true" />
            Create folder
          </button>
          <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
            Cancel
          </button>
        </div>
      </form>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------------------
// Folder toolbar: scope, Rename, Delete, Apps…
// ---------------------------------------------------------------------------------------

type Which = "rename" | "delete" | "apps" | null;

export function FolderToolbar({
  folder,
  info,
  advanced,
  onDone,
}: {
  folder: FolderRef;
  info: FolderInfo | undefined;
  advanced: boolean;
  onDone: (file: string | null) => void;
}) {
  const [which, setWhich] = useState<Which>(null);
  const close = () => setWhich(null);
  const lockNote = info?.locked ?? null;
  const scopeNote = info?.scope_locked ?? null;
  return (
    <div className="fm-bar">
      <p className="fm-bar-scope">
        <AppWindow size={16} aria-hidden="true" />
        <span>
          Applies to: <strong>{info ? scopeLabel(info) : "…"}</strong>
        </span>
      </p>
      <div className="fm-bar-buttons" role="group" aria-label={`Folder ${folder.name}`}>
        <button type="button" className="btn" onClick={() => setWhich("apps")} disabled={!info || !!scopeNote}>
          <AppWindow size={16} aria-hidden="true" />
          Apps…
        </button>
        <button type="button" className="btn" onClick={() => setWhich("rename")} disabled={!info || !!lockNote}>
          <Pencil size={16} aria-hidden="true" />
          Rename
        </button>
        <button type="button" className="btn" onClick={() => setWhich("delete")} disabled={!info || !!lockNote}>
          <Trash2 size={16} aria-hidden="true" />
          Delete
        </button>
      </div>
      {(lockNote || scopeNote) && (
        <p className="fm-note">
          {lockNote ? `This folder cannot be renamed or deleted here. ${lockNote}` : `Its apps cannot be changed here. ${scopeNote}`}
        </p>
      )}
      {which === "rename" && <RenameDialog folder={folder} onClose={close} onDone={onDone} advanced={advanced} />}
      {which === "delete" && <DeleteDialog folder={folder} onClose={close} onDone={onDone} />}
      {which === "apps" && info && (
        <AppsDialog folder={folder} info={info} advanced={advanced} onClose={close} onDone={onDone} />
      )}
    </div>
  );
}

// ---- Rename ----------------------------------------------------------------------------

function RenameDialog({
  folder,
  advanced,
  onClose,
  onDone,
}: {
  folder: FolderRef;
  advanced: boolean;
  onClose: () => void;
  onDone: (file: string | null) => void;
}) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const fieldId = useId();
  const [name, setName] = useState(folder.name);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(false);
  const [busy, setBusy] = useState(false);
  const unchanged = name.trim() === folder.name;
  const preview = usePreview(
    () => (unchanged || !name.trim() ? null : invoke<Change[]>("preview_rename_folder", { file: folder.file, newName: name })),
    [name],
  );

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const out = await invoke<Outcome>("rename_folder", {
        file: folder.file,
        newName: name,
        expectedHash: folder.hash,
      });
      onDone(out.file);
      dialog.close();
    } catch (err) {
      const text = String(err);
      setReload(text.startsWith(CONFLICT_PREFIX));
      setError(errorText(err));
      setBusy(false);
    }
  }

  const problem = error ?? preview.error;
  return (
    <Dialog title={`Rename “${folder.name}”`} labelId={titleId} dialog={dialog}>
      <form onSubmit={submit} className="fm-form">
        <label htmlFor={fieldId}>New name</label>
        <input
          id={fieldId}
          className="field"
          value={name}
          autoFocus
          autoComplete="off"
          onChange={(e) => setName(e.target.value)}
        />
        <p className="fm-help">
          Phrases and the apps this folder applies to stay as they are. Any settings that point to the
          folder are updated for you.
        </p>
        {advanced && preview.changes && (
          <>
            <p className="fm-help">Files that will change:</p>
            <ChangeList changes={preview.changes} />
          </>
        )}
        {problem && (
          <Problem
            message={problem}
            onReload={
              reload
                ? () => {
                    onDone(folder.file);
                    dialog.close();
                  }
                : undefined
            }
          />
        )}
        <div className="fm-buttons">
          <button
            type="submit"
            className="btn btn-primary"
            disabled={busy || unchanged || !name.trim() || !!preview.error}
          >
            Rename
          </button>
          <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
            Cancel
          </button>
        </div>
      </form>
    </Dialog>
  );
}

// ---- Delete ----------------------------------------------------------------------------

function DeleteDialog({
  folder,
  onClose,
  onDone,
}: {
  folder: FolderRef;
  onClose: () => void;
  onDone: (file: string | null) => void;
}) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(false);
  const [busy, setBusy] = useState(false);
  const preview = usePreview(() => invoke<Change[]>("preview_delete_folder", { file: folder.file }), []);

  async function remove() {
    setBusy(true);
    setError(null);
    try {
      await invoke<Outcome>("delete_folder", { file: folder.file, confirm: true, expectedHash: folder.hash });
      onDone(null);
      dialog.close();
    } catch (err) {
      setReload(String(err).startsWith(CONFLICT_PREFIX));
      setError(errorText(err));
      setBusy(false);
    }
  }

  const problem = error ?? preview.error;
  return (
    <Dialog title={`Delete folder “${folder.name}”?`} labelId={titleId} dialog={dialog}>
      <p>
        This removes the folder and its {folder.phrases} {folder.phrases === 1 ? "phrase" : "phrases"} from
        espanso. These files will change:
      </p>
      {preview.changes ? <ChangeList changes={preview.changes} /> : !problem && <p className="fm-help">Checking…</p>}
      <p className="fm-help">
        Every file that is deleted is first copied to the backup named above, so the folder can be restored by
        copying the backup back. Nothing else in your settings changes.
      </p>
      {problem && (
        <Problem
          message={problem}
          onReload={
            reload
              ? () => {
                  onDone(folder.file);
                  dialog.close();
                }
              : undefined
          }
        />
      )}
      <div className="fm-buttons">
        <button type="button" className="btn btn-danger" onClick={remove} disabled={busy || !preview.changes}>
          <Trash2 size={16} aria-hidden="true" />
          {busy ? "Deleting…" : "Delete folder"}
        </button>
        <button type="button" className="btn" autoFocus onClick={dialog.close} disabled={busy}>
          Cancel
        </button>
      </div>
    </Dialog>
  );
}

// ---- Apps ------------------------------------------------------------------------------

function AppsDialog({
  folder,
  info,
  advanced,
  onClose,
  onDone,
}: {
  folder: FolderRef;
  info: FolderInfo;
  advanced: boolean;
  onClose: () => void;
  onDone: (file: string | null) => void;
}) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const listId = useId();
  const apps = info.scope.kind === "only" ? info.scope.apps : [];
  const customExec = apps.some((a) => a.exec_pattern && !a.exec_simple);
  const customTitle = apps.some((a) => a.title_pattern && !a.title_text);
  const [mode, setMode] = useState<"all" | "only">(info.scope.kind === "only" ? "only" : "all");
  const [exes, setExes] = useState<string[]>(() => [...new Set(apps.flatMap((a) => a.exe_names))]);
  const [draft, setDraft] = useState("");
  const [title, setTitle] = useState(apps.find((a) => a.title_text)?.title_text ?? "");
  const [dropCustomExec, setDropCustomExec] = useState(false);
  const [dropCustomTitle, setDropCustomTitle] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(false);
  const [busy, setBusy] = useState(false);

  const spec: ScopeSpec =
    mode === "all"
      ? { kind: "all" }
      : {
          kind: "apps",
          exes: exes.length === 0 && customExec && !dropCustomExec ? null : exes,
          title: title.trim()
            ? { kind: "contains", text: title.trim() }
            : customTitle && !dropCustomTitle
              ? { kind: "keep" }
              : { kind: "clear" },
        };
  const alreadyAll = mode === "all" && info.scope.kind === "all";
  const preview = usePreview(
    () => (alreadyAll ? null : invoke<Change[]>("preview_folder_scope", { file: folder.file, spec })),
    [JSON.stringify(spec)],
  );

  function addDraft() {
    const parts = draft
      .split(/[,;\n]/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (parts.length === 0) return;
    setExes((cur) => [...cur, ...parts.filter((p) => !cur.some((c) => c.toLowerCase() === p.toLowerCase()))]);
    setDraft("");
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const out = await invoke<Outcome>("set_folder_scope", { file: folder.file, spec, expectedHash: folder.hash });
      onDone(out.file);
      dialog.close();
    } catch (err) {
      setReload(String(err).startsWith(CONFLICT_PREFIX));
      setError(errorText(err));
      setBusy(false);
    }
  }

  const problem = error ?? preview.error;
  return (
    <Dialog title={`Apps for “${folder.name}”`} labelId={titleId} dialog={dialog}>
      <p className="fm-help">
        Choose where the phrases in this folder expand. Phrases in “All apps” folders work everywhere; phrases
        in an app-specific folder work only while one of its apps is in front.
      </p>
      <fieldset className="fm-radios">
        <legend className="fm-legend">Where should this folder work?</legend>
        <label className="fm-radio">
          <input type="radio" name={`${listId}-mode`} checked={mode === "all"} onChange={() => setMode("all")} />
          All apps
        </label>
        <label className="fm-radio">
          <input type="radio" name={`${listId}-mode`} checked={mode === "only"} onChange={() => setMode("only")} />
          Only these apps
        </label>
      </fieldset>

      {mode === "only" && (
        <div className="fm-apps">
          <label htmlFor={`${listId}-exe`}>Programs</label>
          <ul className="fm-chips" aria-label="Chosen programs">
            {exes.map((x) => (
              <li key={x} className="fm-chip">
                {x}
                <button
                  type="button"
                  className="fm-chip-x"
                  aria-label={`Remove ${x}`}
                  onClick={() => setExes((cur) => cur.filter((c) => c !== x))}
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </li>
            ))}
            {exes.length === 0 && <li className="fm-help">No programs added.</li>}
          </ul>
          <div className="fm-add">
            <input
              id={`${listId}-exe`}
              className="field"
              value={draft}
              placeholder="winword.exe"
              autoComplete="off"
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addDraft();
                }
              }}
              aria-describedby={`${listId}-exe-help`}
            />
            <button type="button" className="btn" onClick={addDraft} disabled={!draft.trim()}>
              <Plus size={16} aria-hidden="true" />
              Add
            </button>
          </div>
          <p id={`${listId}-exe-help`} className="fm-help">
            Type the program’s file name, for example winword.exe or firefox. On Windows, open Task Manager, go
            to the Details tab and look in the Name column while the program is running.
          </p>
          {customExec && (
            <label className="fm-check">
              <input type="checkbox" checked={dropCustomExec} onChange={(e) => setDropCustomExec(e.target.checked)} />
              Replace the program filter written by hand
              {advanced && (
                <>
                  {" "}
                  (<code>{apps.find((a) => a.exec_pattern && !a.exec_simple)?.exec_pattern}</code>)
                </>
              )}
            </label>
          )}

          <label htmlFor={`${listId}-title`}>Window title contains (optional)</label>
          <input
            id={`${listId}-title`}
            className="field"
            value={title}
            autoComplete="off"
            placeholder="for example: Inbox"
            onChange={(e) => setTitle(e.target.value)}
            aria-describedby={`${listId}-title-help`}
          />
          <p id={`${listId}-title-help`} className="fm-help">
            Narrows the folder to windows whose title has this text, such as one website in a browser. Capital
            letters do not matter. When you also choose programs, both must match.
          </p>
          {customTitle && (
            <label className="fm-check">
              <input type="checkbox" checked={dropCustomTitle} onChange={(e) => setDropCustomTitle(e.target.checked)} />
              Remove the window-title filter written by hand
              {advanced && (
                <>
                  {" "}
                  (<code>{apps.find((a) => a.title_pattern && !a.title_text)?.title_pattern}</code>)
                </>
              )}
            </label>
          )}
          {advanced && apps.length > 0 && (
            <p className="fm-help">
              Settings {apps.length > 1 ? "files" : "file"}:{" "}
              {apps.map((a, i) => (
                <span key={a.config}>
                  {i > 0 && ", "}
                  <code>config/{a.config}</code>
                </span>
              ))}
            </p>
          )}
        </div>
      )}

      {alreadyAll && <p className="fm-help">This folder already applies to all apps.</p>}
      {advanced && preview.changes && preview.changes.length > 0 && (
        <>
          <p className="fm-help">Files that will change:</p>
          <ChangeList changes={preview.changes} />
        </>
      )}
      {!advanced && preview.changes && preview.changes.length > 0 && (
        <p className="fm-help">xpando will update the settings that decide which apps use this folder.</p>
      )}
      {problem && (
        <Problem
          message={problem}
          onReload={
            reload
              ? () => {
                  onDone(folder.file);
                  dialog.close();
                }
              : undefined
          }
        />
      )}
      <div className="fm-buttons">
        <button
          type="button"
          className="btn btn-primary"
          onClick={save}
          disabled={busy || alreadyAll || !!preview.error || !preview.changes || preview.changes.length === 0}
        >
          Save
        </button>
        <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
          Cancel
        </button>
      </div>
    </Dialog>
  );
}
