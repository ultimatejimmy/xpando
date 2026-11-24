import { useEffect, useId, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, MoveRight, Undo2, X } from "lucide-react";
import { AppPicker } from "./AppPicker";
import { CONFLICT_PREFIX, ChangeList, Dialog, Problem, errorText, useDialog, type Change, type FolderInfo } from "./FolderManager";
import { allAppsFolders, rememberOrigin, returnFolder, worksIn, worksInLabel, type Origins } from "./worksInLogic";

/** Mirrors `xpando_core::folders::{MovePreview, MoveOutcome}`. */
interface MoveConflict {
  trigger: string;
  message: string;
  blocking: boolean;
}

interface MovePreview {
  from_file: string;
  from_name: string;
  to_file: string | null;
  to_name: string;
  creates_folder: boolean;
  unchanged: boolean;
  notice: string;
  warnings: string[];
  conflicts: MoveConflict[];
  blocked: string | null;
  changes: Change[];
}

export interface MoveOutcome {
  from_file: string;
  from_name: string;
  to_file: string;
  to_name: string;
  to_index: number;
  created_folder: boolean;
  changes: Change[];
}

type Place =
  | { kind: "all"; folder: string | null }
  | { kind: "apps"; exes: string[]; names: string[] }
  | { kind: "folder"; file: string };

const ORIGINS_KEY = "xpando.phrase-origins";

function loadOrigins(): Origins {
  try {
    const raw = window.localStorage.getItem(ORIGINS_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    return parsed && typeof parsed === "object" ? (parsed as Origins) : {};
  } catch {
    return {};
  }
}

function saveOrigins(o: Origins) {
  try {
    window.localStorage.setItem(ORIGINS_KEY, JSON.stringify(o));
  } catch {
    /* A private window or blocked storage: the phrase then goes back to the default folder. */
  }
}

export interface WorksInFolder {
  file: string;
  name: string;
  hash: string;
}

export interface WorksInPhrase {
  index: number;
  triggers: string[];
  name: string;
}

/**
 * The "Works in" line of a phrase and the dialog that changes it. Changing it moves the phrase
 * to the folder that has those apps (xpando creates one if needed), because espanso limits apps
 * per folder, not per phrase.
 */
export function WorksIn({
  folder,
  phrase,
  infos,
  folders,
  advanced,
  onMoved,
  onReload,
  compact,
  blocked,
}: {
  folder: WorksInFolder;
  phrase: WorksInPhrase;
  infos: Record<string, FolderInfo>;
  folders: { file: string; name: string }[];
  advanced: boolean;
  onMoved: (outcome: MoveOutcome, phraseName: string) => void;
  onReload: () => void;
  /** One line for a header (the editor), without the section heading and notes. */
  compact?: boolean;
  /** Why Change is unavailable right now, when it is. */
  blocked?: string;
}) {
  const [open, setOpen] = useState(false);
  const info = infos[folder.file];
  const current = worksIn(info);
  const label = info ? worksInLabel(current) : "…";
  const change = (
    <button
      type="button"
      className="wi-change"
      onClick={() => setOpen(true)}
      disabled={!info || !!blocked}
      title={blocked}
    >
      Change
    </button>
  );
  const dialog = open && info && (
    <WorksInDialog
      folder={folder}
      phrase={phrase}
      infos={infos}
      folders={folders}
      advanced={advanced}
      onClose={() => setOpen(false)}
      onMoved={onMoved}
      onReload={onReload}
    />
  );
  if (compact) {
    return (
      <div className="wi wi-compact">
        <div className="wi-box">
          <span className="wi-box-text">{label}</span>
          {change}
        </div>
        {blocked && <p className="wi-note">Save or cancel your changes to change this.</p>}
        {dialog}
      </div>
    );
  }
  return (
    <div className="wi">
      <h3 className="detail-meta">Works in</h3>
      <p className="wi-line">
        {label}
        {change}
      </p>
      {current.kind === "custom" && (
        <p className="wi-note">
          Its folder “{folder.name}” is limited in a way set up by hand (for example by a window title). Changing this
          moves the phrase to another folder.
        </p>
      )}
      {current.kind === "nowhere" && (
        <p className="wi-note">No setting loads the folder “{folder.name}”, so this phrase does not expand.</p>
      )}
      {current.kind === "except" && (
        <p className="wi-note">The folder “{folder.name}” works everywhere except in some apps.</p>
      )}
      {advanced && info && (
        <p className="wi-note">
          Stored in the folder <code>match/{folder.file}</code>
          {info.configs.length > 0 && (
            <>
              ; loaded by {info.configs.map((c, i) => <span key={c}>{i > 0 && ", "}<code>config/{c}</code></span>)}
            </>
          )}
          .
        </p>
      )}
      {dialog}
    </div>
  );
}

function WorksInDialog({
  folder,
  phrase,
  infos,
  folders,
  advanced,
  onClose,
  onMoved,
  onReload,
}: {
  folder: WorksInFolder;
  phrase: WorksInPhrase;
  infos: Record<string, FolderInfo>;
  folders: { file: string; name: string }[];
  advanced: boolean;
  onClose: () => void;
  onMoved: (outcome: MoveOutcome, phraseName: string) => void;
  onReload: () => void;
}) {
  const dialog = useDialog(onClose);
  const uid = useId();
  const current = worksIn(infos[folder.file]);
  const origins = useMemo(loadOrigins, []);
  const allFolders = useMemo(() => allAppsFolders(folders, infos), [folders, infos]);
  const [mode, setMode] = useState<"all" | "apps">(current.kind === "apps" ? "apps" : current.kind === "all" ? "apps" : "all");
  const [exes, setExes] = useState<string[]>(current.kind === "apps" ? current.exes : []);
  const [names, setNames] = useState<string[]>([]);
  const [target, setTarget] = useState<string>(
    () => returnFolder(origins, folder.file, phrase.triggers[0] ?? "", allFolders) ?? "",
  );
  const [preview, setPreview] = useState<MovePreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(false);
  const [busy, setBusy] = useState(false);

  const place: Place | null =
    mode === "all"
      ? current.kind === "all"
        ? null
        : { kind: "all", folder: target || null }
      : exes.length === 0
        ? null
        : { kind: "apps", exes, names };

  const placeKey = JSON.stringify(place);
  useEffect(() => {
    setPreview(null);
    setPreviewError(null);
    if (!place) return;
    let live = true;
    const timer = setTimeout(() => {
      invoke<MovePreview>("preview_phrase_where", { file: folder.file, index: phrase.index, place })
        .then((p) => live && setPreview(p))
        .catch((e) => live && setPreviewError(errorText(e)));
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placeKey, folder.file, folder.hash, phrase.index]);

  async function move() {
    if (!place) return;
    setBusy(true);
    setError(null);
    try {
      const out = await invoke<MoveOutcome>("set_phrase_where", {
        file: folder.file,
        index: phrase.index,
        place,
        expectedHash: folder.hash,
      });
      // Remember where an all-apps phrase came from, so "All apps" can put it back there.
      if (current.kind === "all" && place.kind === "apps") {
        saveOrigins(rememberOrigin(loadOrigins(), out.to_file, phrase.triggers[0] ?? "", folder.file));
      }
      onMoved(out, phrase.name);
      dialog.close();
    } catch (e) {
      setReload(String(e).startsWith(CONFLICT_PREFIX));
      setError(errorText(e));
      setBusy(false);
    }
  }

  const blockingConflicts = preview?.conflicts.filter((c) => c.blocking) ?? [];
  const softConflicts = preview?.conflicts.filter((c) => !c.blocking) ?? [];
  const blockedText = preview?.blocked ?? null;
  const canMove = !!place && !!preview && !preview.unchanged && !preview.blocked && !busy;
  const problem = error ?? previewError;

  return (
    <Dialog title={`Where should “${phrase.name}” work?`} labelId={`${uid}-title`} dialog={dialog}>
      <fieldset className="fm-radios">
        <legend className="fm-legend">Works in</legend>
        <label className="fm-radio">
          <input type="radio" name={`${uid}-mode`} checked={mode === "all"} onChange={() => setMode("all")} />
          All apps
        </label>
        <label className="fm-radio">
          <input type="radio" name={`${uid}-mode`} checked={mode === "apps"} onChange={() => setMode("apps")} />
          Only these apps
        </label>
      </fieldset>

      {mode === "apps" && (
        <AppPicker
          value={exes}
          advanced={advanced}
          onChange={(e, n) => {
            setExes(e);
            setNames(n);
          }}
        />
      )}

      {mode === "all" && current.kind === "all" && (
        <p className="fm-help">This phrase already works in all apps.</p>
      )}
      {mode === "all" && current.kind !== "all" && (
        <div className="fm-form">
          <label htmlFor={`${uid}-target`}>Put it in this folder</label>
          <select
            id={`${uid}-target`}
            className="field wi-select"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
          >
            {allFolders.length === 0 && <option value="">(no folder works in all apps)</option>}
            {allFolders.map((f) => (
              <option key={f.file} value={f.file}>
                {f.name}
              </option>
            ))}
          </select>
        </div>
      )}

      <div className="wi-result" aria-live="polite">
        {place && !preview && !problem && <p className="fm-help">Checking…</p>}
        {preview && !preview.unchanged && !blockedText && <p>{preview.notice}</p>}
        {preview?.unchanged && <p className="fm-help">{preview.notice}</p>}
        {preview && preview.warnings.length > 0 && (
          <ul className="wi-list">
            {preview.warnings.map((w) => (
              <li key={w} className="wi-warn">
                <AlertTriangle size={14} aria-hidden="true" /> {w}
              </li>
            ))}
          </ul>
        )}
        {softConflicts.length > 0 && (
          <ul className="wi-list">
            {softConflicts.map((c, i) => (
              <li key={i} className="wi-warn">
                <AlertTriangle size={14} aria-hidden="true" /> {c.message}
              </li>
            ))}
          </ul>
        )}
        {advanced && preview && preview.changes.length > 0 && !blockedText && (
          <>
            <p className="fm-help">Files that will change (a backup copy is kept the first time):</p>
            <ChangeList changes={preview.changes} />
          </>
        )}
      </div>
      {(blockedText || blockingConflicts.length > 0) && <Problem message={blockedText ?? blockingConflicts[0].message} />}
      {problem && (
        <Problem
          message={problem}
          onReload={
            reload
              ? () => {
                  onReload();
                  dialog.close();
                }
              : undefined
          }
        />
      )}
      <div className="fm-buttons">
        <button type="button" className="btn btn-primary" onClick={() => void move()} disabled={!canMove}>
          <MoveRight size={16} aria-hidden="true" />
          {busy ? "Moving…" : "Move phrase"}
        </button>
        <button type="button" className="btn" onClick={dialog.close} disabled={busy}>
          Cancel
        </button>
      </div>
    </Dialog>
  );
}

/** The message after a move, with a button that puts the phrase back. */
export function MovedNotice({
  outcome,
  phraseName,
  busy,
  error,
  onUndo,
  onClose,
}: {
  outcome: MoveOutcome;
  phraseName: string;
  busy: boolean;
  error: string | null;
  onUndo: () => void;
  onClose: () => void;
}) {
  return (
    <div className="wi-toast" role="status">
      <span>
        Moved “{phraseName}” to the folder “{outcome.to_name}”.
        {outcome.created_folder && " xpando made that folder for these apps."}
      </span>
      <button type="button" className="btn" onClick={onUndo} disabled={busy}>
        <Undo2 size={16} aria-hidden="true" />
        {busy ? "Undoing…" : "Undo"}
      </button>
      <button type="button" className="btn" onClick={onClose} aria-label="Dismiss this message">
        <X size={16} aria-hidden="true" />
      </button>
      {error && <Problem message={error} />}
    </div>
  );
}
