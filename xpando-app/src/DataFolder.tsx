import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, FolderInput, FolderOpen, RefreshCw, Settings as SettingsIcon, X } from "lucide-react";
import { UpdatesSection, type UpdatesApi } from "./Updates";
import "./data.css";

/** A pasted path: surrounding spaces and quotes ("Copy as path" adds quotes) are removed. */
export function cleanPath(raw: string): string {
  return raw.trim().replace(/^["'“‘]+|["'”’]+$/g, "").trim();
}

// ---------------------------------------------------------------------------------------
// Types (the backend's JSON)
// ---------------------------------------------------------------------------------------

export interface ConflictedCopy {
  file: string;
  original: string | null;
  original_exists: boolean;
}

export interface DataInfo {
  path: string;
  is_default: boolean;
  default_path: string | null;
  status: { kind: "ok" | "missing" | "not_a_directory" | "unreadable"; detail?: string };
  problem: string | null;
  conflicts: ConflictedCopy[];
}

interface Problem {
  file: string;
  reason: string;
}

interface CopyReport {
  copied: string[];
  already_there: string[];
  skipped: Problem[];
  failed: Problem[];
}

interface ImportPreview {
  source: string;
  files: { file: string; size: number; exists_in_destination: boolean }[];
  skipped: Problem[];
  unreadable: Problem[];
}

interface PathCheck {
  path: string;
  ok: boolean;
  problem: string | null;
  will_create: boolean;
  has_data: boolean;
}

interface ChangeResult {
  path: string;
  switched: boolean;
  report: CopyReport | null;
}

// ---------------------------------------------------------------------------------------
// Data folder state
// ---------------------------------------------------------------------------------------

/** Where the data is and how it is doing; `refresh` asks again (also whenever the window regains focus). */
export function useDataInfo(): { info: DataInfo | null; error: string | null; refresh: () => Promise<DataInfo | null> } {
  const [info, setInfo] = useState<DataInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(
    () =>
      invoke<DataInfo>("data_info")
        .then((i) => {
          setInfo(i);
          setError(null);
          return i;
        })
        .catch((e) => {
          setError(String(e));
          return null;
        }),
    [],
  );
  useEffect(() => {
    void refresh();
  }, [refresh]);
  return { info, error, refresh };
}

function useDialog(onClose: () => void) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);
  return { ref, onClose };
}

function Dialog({
  title,
  dialog,
  children,
}: {
  title: string;
  dialog: ReturnType<typeof useDialog>;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <dialog ref={dialog.ref} className="fm-dialog df-dialog" aria-labelledby={id} onClose={dialog.onClose}>
      <h2 id={id} className="fm-title">
        {title}
      </h2>
      {children}
    </dialog>
  );
}

function fileLabel(file: string): string {
  return file.replace(/^match\//, "").replace(/^config\//, "App settings: ");
}

function Summary({ report }: { report: CopyReport }) {
  return (
    <>
      <p>
        Copied {report.copied.length} {report.copied.length === 1 ? "file" : "files"}.
        {report.already_there.length > 0 && ` ${report.already_there.length} already there and kept as they were.`}
      </p>
      {report.skipped.length > 0 && (
        <details className="df-details">
          <summary>{report.skipped.length} not copied</summary>
          <ul>
            {report.skipped.map((s) => (
              <li key={s.file}>
                <code>{s.file}</code>: {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
      {report.failed.length > 0 && (
        <div className="fm-problem" role="alert">
          {report.failed.length} could not be copied:
          <ul>
            {report.failed.map((s) => (
              <li key={s.file}>
                <code>{s.file}</code>: {s.reason}
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------------------
// Banners: unreachable folder and sync conflict copies
// ---------------------------------------------------------------------------------------

export function DataBanners({
  info,
  error,
  onRetry,
  onOpenSettings,
}: {
  info: DataInfo | null;
  error: string | null;
  onRetry: () => void;
  onOpenSettings: () => void;
}) {
  const [hidden, setHidden] = useState<string>("");
  const problem = info?.problem ?? error;
  const conflicts = info?.conflicts ?? [];
  const conflictKey = conflicts.map((c) => c.file).join("|");
  return (
    <>
      {problem && (
        <div className="df-banner df-banner-warn" role="alert">
          <AlertTriangle size={18} aria-hidden="true" />
          <div className="df-banner-body">
            <strong>xpando cannot reach your data folder.</strong>
            <p>{problem}</p>
          </div>
          <div className="df-banner-actions">
            <button type="button" className="btn" onClick={onRetry}>
              <RefreshCw size={16} aria-hidden="true" />
              Try again
            </button>
            <button type="button" className="btn" onClick={onOpenSettings}>
              <SettingsIcon size={16} aria-hidden="true" />
              Settings
            </button>
          </div>
        </div>
      )}
      {!problem && conflicts.length > 0 && hidden !== conflictKey && (
        <div className="df-banner" role="status">
          <AlertTriangle size={18} aria-hidden="true" />
          <div className="df-banner-body">
            <strong>
              {conflicts.length === 1 ? "1 file looks" : `${conflicts.length} files look`} like a copy made by your sync
              service.
            </strong>
            <p>
              xpando loads these like any other file, so a phrase can expand twice or an older version can win. Open the
              data folder, keep the version you want and delete the other.
            </p>
            <ul>
              {conflicts.map((c) => (
                <li key={c.file}>
                  <code>{c.file}</code>
                  {c.original && (
                    <>
                      {" "}
                      (copy of <code>{c.original}</code>
                      {c.original_exists ? "" : ", which is not here"})
                    </>
                  )}
                </li>
              ))}
            </ul>
          </div>
          <button type="button" className="btn btn-icon" aria-label="Hide this message" onClick={() => setHidden(conflictKey)}>
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------------------
// Import from espanso
// ---------------------------------------------------------------------------------------

/** Asks for the import preview; `force` is the Settings button (ignores "already answered"). */
export function useImportOffer(ready: boolean) {
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  useEffect(() => {
    if (!ready) return;
    invoke<ImportPreview | null>("import_offer", { force: false })
      .then(setPreview)
      .catch(() => setPreview(null));
  }, [ready]);
  return { preview, close: () => setPreview(null), open: setPreview };
}

export function ImportDialog({
  preview,
  onClose,
  onImported,
}: {
  preview: ImportPreview;
  onClose: () => void;
  onImported: () => void;
}) {
  const dialog = useDialog(onClose);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<CopyReport | null>(null);
  const toCopy = preview.files.filter((f) => !f.exists_in_destination).length;

  const run = () => {
    setBusy(true);
    setError(null);
    invoke<CopyReport>("import_espanso", { source: preview.source })
      .then((r) => {
        setReport(r);
        onImported();
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };
  const never = () => {
    invoke("skip_import")
      .catch(() => undefined)
      .finally(() => dialog.ref.current?.close());
  };

  return (
    <Dialog title={report ? "Import finished" : "Bring your espanso phrases to xpando?"} dialog={dialog}>
      {report ? (
        <>
          <Summary report={report} />
          <div className="fm-buttons">
            <button type="button" className="btn btn-primary" autoFocus onClick={() => dialog.ref.current?.close()}>
              Done
            </button>
          </div>
        </>
      ) : (
        <>
          <p>
            xpando found your espanso setup in <code>{preview.source}</code>. It can copy your phrases and app settings
            into its own folder. Your espanso files are only read: they are not changed, moved or deleted.
          </p>
          <p className="fm-help">
            {toCopy} {toCopy === 1 ? "file" : "files"} will be copied.
            {preview.files.length > toCopy && ` ${preview.files.length - toCopy} already exist in xpando and are kept.`}
          </p>
          <ul className="df-files" aria-label="Files to copy">
            {preview.files.map((f) => (
              <li key={f.file}>
                <code>{fileLabel(f.file)}</code>
                {f.exists_in_destination && <span className="df-note"> already in xpando, kept</span>}
              </li>
            ))}
          </ul>
          {(preview.skipped.length > 0 || preview.unreadable.length > 0) && (
            <details className="df-details">
              <summary>{preview.skipped.length + preview.unreadable.length} will not be copied</summary>
              <ul>
                {[...preview.skipped, ...preview.unreadable].map((s) => (
                  <li key={s.file}>
                    <code>{s.file}</code>: {s.reason}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {error && (
            <p className="fm-problem" role="alert">
              {error}
            </p>
          )}
          <div className="fm-buttons">
            <button type="button" className="btn btn-primary" autoFocus disabled={busy} onClick={run}>
              <FolderInput size={16} aria-hidden="true" />
              Import
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => dialog.ref.current?.close()}>
              Not now
            </button>
            <button type="button" className="btn btn-quiet" disabled={busy} onClick={never}>
              Don't ask again
            </button>
          </div>
        </>
      )}
    </Dialog>
  );
}

// ---------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------

export function SettingsButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="btn btn-icon" aria-label="Settings" title="Settings" onClick={onClick}>
      <SettingsIcon size={18} aria-hidden="true" />
    </button>
  );
}

type Step =
  | { kind: "view" }
  | { kind: "choose" }
  | { kind: "ask"; check: PathCheck }
  | { kind: "done"; result: ChangeResult };

export function SettingsDialog({
  info,
  onClose,
  onChanged,
  onOffer,
  updates,
}: {
  info: DataInfo | null;
  updates: UpdatesApi;
  onClose: () => void;
  /** The data folder changed (or an import ran): reload everything. */
  onChanged: () => void;
  onOffer: (preview: ImportPreview) => void;
}) {
  const dialog = useDialog(onClose);
  const [step, setStep] = useState<Step>({ kind: "view" });
  const [text, setText] = useState("");
  const [check, setCheck] = useState<PathCheck | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const inputId = useId();

  const validate = (path: string) => {
    if (!path.trim()) {
      setCheck(null);
      return;
    }
    invoke<PathCheck>("check_data_dir", { path })
      .then(setCheck)
      .catch((e) => setError(String(e)));
  };

  const browse = () => {
    setError(null);
    invoke<string | null>("pick_folder", { start: info?.path ?? null })
      .then((p) => {
        if (p) {
          const clean = cleanPath(p);
          setText(clean);
          validate(clean);
        }
      })
      .catch((e) =>
        setError(String(e) || "The folder dialog could not be opened. Type the folder path in the box instead."),
      );
  };

  const apply = (path: string | null, copy: boolean) => {
    setBusy(true);
    setError(null);
    invoke<ChangeResult>("change_data_dir", { path, copy })
      .then((result) => {
        setStep({ kind: "done", result });
        if (result.switched) onChanged();
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  const findEspanso = () => {
    setNote(null);
    setError(null);
    invoke<ImportPreview | null>("import_offer", { force: true })
      .then((p) => {
        if (p) {
          dialog.ref.current?.close();
          onOffer(p);
        } else {
          setNote("No espanso phrases were found on this computer.");
        }
      })
      .catch((e) => setError(String(e)));
  };

  const close = () => dialog.ref.current?.close();

  return (
    <Dialog title="Settings" dialog={dialog}>
      {step.kind === "view" && (
        <>
          <h3 className="df-h3">Data folder</h3>
          <p className="fm-help">
            Your phrases are saved here. {info?.is_default ? "This is the default folder." : "You chose this folder."}
          </p>
          <p className="df-path">
            <FolderOpen size={16} aria-hidden="true" />
            <code>{info?.path ?? "..."}</code>
          </p>
          {info?.problem && (
            <p className="fm-problem" role="alert">
              {info.problem}
            </p>
          )}
          <div className="fm-buttons">
            <button
              type="button"
              className="btn"
              onClick={() => {
                setStep({ kind: "choose" });
                setText("");
                setCheck(null);
                setError(null);
              }}
            >
              Change...
            </button>
            {info && !info.is_default && (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setError(null);
                  apply(null, false);
                }}
              >
                Use the default folder
              </button>
            )}
          </div>
          <p className="fm-help df-tip">
            Tip: pick a folder in OneDrive, Google Drive or Dropbox to back your phrases up or use them on another computer.
          </p>

          <h3 className="df-h3">Import from espanso</h3>
          <p className="fm-help">Copy phrases from an espanso setup on this computer. Your espanso files are not changed.</p>
          <div className="fm-buttons">
            <button type="button" className="btn" onClick={findEspanso}>
              <FolderInput size={16} aria-hidden="true" />
              Find espanso phrases
            </button>
          </div>
          {note && <p className="fm-help">{note}</p>}

          <UpdatesSection u={updates} />
          {error && (
            <p className="fm-problem" role="alert">
              {error}
            </p>
          )}
          <div className="fm-buttons">
            <button type="button" className="btn btn-primary" autoFocus onClick={close}>
              Close
            </button>
          </div>
        </>
      )}

      {step.kind === "choose" && (
        <>
          <p className="fm-help">Choose the folder where xpando should keep your phrases.</p>
          <div className="fm-form">
            <label htmlFor={inputId}>Folder</label>
            <div className="df-row">
              <input
                id={inputId}
                className="field"
                type="text"
                value={text}
                autoFocus
                spellCheck={false}
                placeholder="For example C:\Users\you\OneDrive\xpando"
                onChange={(e) => {
                  setText(e.target.value);
                  setCheck(null);
                }}
                onBlur={() => {
                  const clean = cleanPath(text);
                  if (clean !== text) setText(clean);
                  validate(clean);
                }}
              />
              <button type="button" className="btn" onClick={browse}>
                <FolderOpen size={16} aria-hidden="true" /> Browse…
              </button>
            </div>
            {check && !check.ok && (
              <p className="fm-problem" role="alert">
                {check.problem}
              </p>
            )}
            {check?.ok && (
              <p className="fm-help">
                {check.will_create
                  ? "This folder does not exist yet. xpando will create it."
                  : check.has_data
                    ? "This folder already has phrases in it. xpando will use them; nothing there is overwritten."
                    : "This folder is ready."}
              </p>
            )}
          </div>
          {error && (
            <p className="fm-problem" role="alert">
              {error}
            </p>
          )}
          <div className="fm-buttons">
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy || !text.trim()}
              onClick={() =>
                invoke<PathCheck>("check_data_dir", { path: cleanPath(text) })
                  .then((c) => {
                    setCheck(c);
                    if (c.ok) setStep({ kind: "ask", check: c });
                  })
                  .catch((e) => setError(String(e)))
              }
            >
              Next
            </button>
            <button type="button" className="btn" onClick={() => setStep({ kind: "view" })}>
              Back
            </button>
          </div>
        </>
      )}

      {step.kind === "ask" && (
        <>
          <p>
            Copy your existing phrases to <code>{step.check.path}</code>?
          </p>
          <p className="fm-help">
            Copying keeps everything. The old folder stays where it is, and nothing already in the new folder is replaced.
          </p>
          {error && (
            <p className="fm-problem" role="alert">
              {error}
            </p>
          )}
          <div className="fm-buttons">
            <button type="button" className="btn btn-primary" autoFocus disabled={busy} onClick={() => apply(step.check.path, true)}>
              Copy my phrases
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => apply(step.check.path, false)}>
              {step.check.has_data ? "Just use this folder" : "Start empty"}
            </button>
            <button type="button" className="btn btn-quiet" disabled={busy} onClick={() => setStep({ kind: "choose" })}>
              Back
            </button>
          </div>
        </>
      )}

      {step.kind === "done" && (
        <>
          {step.result.switched ? (
            <p>
              xpando now uses <code>{step.result.path}</code>.
            </p>
          ) : (
            <p className="fm-problem" role="alert">
              Some files could not be copied, so xpando kept using the old folder.
            </p>
          )}
          {step.result.report && <Summary report={step.result.report} />}
          <div className="fm-buttons">
            <button type="button" className="btn btn-primary" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      )}
    </Dialog>
  );
}
