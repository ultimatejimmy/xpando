import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderPlus, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { Dialog, Problem, errorText, useDialog } from "./FolderManager";
import { sameFolder } from "./folderNames";

/** What a folder dialog needs to know about the folders that exist. */
interface Existing {
  names: string[];
}

function nameProblem(name: string, existing: string[], current: string | null): string | null {
  const n = name.trim();
  if (n === "") return "Enter a name for the folder.";
  if ([...n].length > 60) return "A folder name can have at most 60 characters.";
  if (/[\u0000-\u001f]/.test(n)) return "A folder name cannot contain control characters.";
  if (!sameFolder(n, current) && existing.some((e) => sameFolder(e, n))) return `There is already a folder called “${n}”.`;
  return null;
}

/** The "New folder" button. A new folder is just a name; it is kept even while it is empty. */
export function NewFolderLabelButton({ names, onCreate }: Existing & { onCreate: (name: string) => Promise<void> }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="btn fm-new" onClick={() => setOpen(true)}>
        <FolderPlus size={16} aria-hidden="true" />
        New folder
      </button>
      {open && (
        <NameDialog
          title="New folder"
          action="Create folder"
          initial=""
          current={null}
          names={names}
          onClose={() => setOpen(false)}
          onSubmit={onCreate}
        />
      )}
    </>
  );
}

function NameDialog({
  title,
  action,
  initial,
  current,
  names,
  onClose,
  onSubmit,
}: Existing & {
  title: string;
  action: string;
  initial: string;
  current: string | null;
  onClose: () => void;
  onSubmit: (name: string) => Promise<void>;
}) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const fieldId = useId();
  const [name, setName] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    const bad = nameProblem(name, names, current);
    if (bad) return setError(bad);
    setBusy(true);
    setError(null);
    try {
      await onSubmit(name.trim());
      dialog.close();
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  }

  return (
    <Dialog title={title} labelId={titleId} dialog={dialog}>
      <form onSubmit={submit} className="fm-form">
        <label htmlFor={fieldId}>Folder name</label>
        <input
          id={fieldId}
          className="field"
          value={name}
          autoFocus
          autoComplete="off"
          onChange={(e) => setName(e.target.value)}
        />
        {error && <Problem message={error} />}
        <div className="fm-buttons">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "Saving…" : action}
          </button>
          <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
            Cancel
          </button>
        </div>
      </form>
    </Dialog>
  );
}

/** The "..." menu of a folder: rename it, or delete it (its phrases are kept, in no folder). */
export function FolderLabelMenu({
  name,
  count,
  names,
  onRename,
  onDelete,
}: Existing & {
  name: string;
  count: number;
  onRename: (to: string) => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [which, setWhich] = useState<"rename" | "delete" | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const uid = useId();
  useEffect(() => {
    if (open) wrap.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [open]);

  return (
    <div
      className="fm-menu"
      ref={wrap}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
        }
      }}
      onBlur={(e) => {
        if (!wrap.current?.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <button
        type="button"
        className="btn btn-icon"
        aria-label="Folder options"
        title="Folder options"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={`${uid}-menu`}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal size={18} aria-hidden="true" />
      </button>
      {open && (
        <div className="fm-menu-list" role="menu" id={`${uid}-menu`} aria-label="Folder options">
          <button
            type="button"
            role="menuitem"
            className="fm-menu-item"
            onClick={() => {
              setOpen(false);
              setWhich("rename");
            }}
          >
            <Pencil size={16} aria-hidden="true" />
            Rename folder
          </button>
          <button
            type="button"
            role="menuitem"
            className="fm-menu-item"
            onClick={() => {
              setOpen(false);
              setWhich("delete");
            }}
          >
            <Trash2 size={16} aria-hidden="true" />
            Delete folder
          </button>
        </div>
      )}
      {which === "rename" && (
        <NameDialog
          title="Rename folder"
          action="Rename"
          initial={name}
          current={name}
          names={names}
          onClose={() => setWhich(null)}
          onSubmit={onRename}
        />
      )}
      {which === "delete" && <DeleteDialog name={name} count={count} onClose={() => setWhich(null)} onDelete={onDelete} />}
    </div>
  );
}

function DeleteDialog({
  name,
  count,
  onClose,
  onDelete,
}: {
  name: string;
  count: number;
  onClose: () => void;
  onDelete: () => Promise<void>;
}) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function go() {
    setBusy(true);
    setError(null);
    try {
      await onDelete();
      dialog.close();
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  }
  return (
    <Dialog title={`Delete the folder “${name}”?`} labelId={titleId} dialog={dialog}>
      <p>
        {count === 0
          ? "The folder is empty."
          : `Its ${count} ${count === 1 ? "phrase is" : "phrases are"} not deleted. ${count === 1 ? "It stays" : "They stay"} in your list, in no folder.`}
      </p>
      {error && <Problem message={error} />}
      <div className="fm-buttons">
        <button type="button" className="btn btn-danger" onClick={() => void go()} disabled={busy}>
          {busy ? "Deleting…" : "Delete folder"}
        </button>
        <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
          Cancel
        </button>
      </div>
    </Dialog>
  );
}

/** Files phrases in a folder (`""` is no folder). */
export async function fileInFolder(items: { file: string; index: number }[], folder: string | null): Promise<void> {
  await invoke("set_phrases_folder", { items, folder });
}
