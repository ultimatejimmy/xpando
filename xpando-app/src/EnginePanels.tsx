import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle } from "lucide-react";
import "./engine.css";

export type EngineKind = "stock" | "xpando" | "missing";

export interface EngineStatus {
  kind: EngineKind;
  path: string | null;
  version: string | null;
  bundled: boolean;
}

/** The engine xpando talks to, asked once. `null` while loading or when the backend lacks it. */
export function useEngine(): EngineStatus | null {
  const [status, setStatus] = useState<EngineStatus | null>(null);
  useEffect(() => {
    invoke<EngineStatus>("engine_status")
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);
  return status;
}

const KIND_TEXT: Record<EngineKind, string> = {
  xpando: "xpando engine",
  stock: "a different espanso build (not xpando's)",
  missing: "not found",
};

/** One line for the Advanced status bar. */
export function EngineLine({ engine }: { engine: EngineStatus | null }) {
  if (!engine) return null;
  return (
    <span title={engine.path ?? undefined}>
      Engine: {KIND_TEXT[engine.kind]}
      {engine.version ? ` ${engine.version}` : ""}
    </span>
  );
}

/** Shown (any mode) when a phrase uses Set clipboard but the engine would reject it. */
export function EngineWarning({ engine, usesSetClipboard }: { engine: EngineStatus | null; usesSetClipboard: boolean }) {
  if (!engine || !usesSetClipboard || engine.kind === "xpando") return null;
  return (
    <p className="eng-warning" role="alert">
      <AlertTriangle size={14} aria-hidden="true" />{" "}
      This phrase uses a Set clipboard macro, but xpando's engine was not found, so it will not run.
    </p>
  );
}

interface HackPreview {
  candidate: { position: number; name: string; text: string };
  before: string[];
  after: string[];
}

/**
 * Advanced mode only. Offers to replace the PowerShell clipboard trick with a Set clipboard
 * macro. Nothing changes until the user presses Convert, after seeing both versions.
 */
export function ClipboardHackOffer<F>({
  file,
  index,
  hash,
  onConverted,
}: {
  file: string;
  index: number;
  hash: string;
  onConverted: (folder: F) => void;
}) {
  const [preview, setPreview] = useState<HackPreview | null>(null);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    setOpen(false);
    setError(null);
    invoke<HackPreview | null>("preview_clipboard_hack", { file, index })
      .then((p) => live && setPreview(p))
      .catch(() => live && setPreview(null));
    return () => {
      live = false;
    };
  }, [file, index, hash]);

  if (!preview) return null;

  const convert = () => {
    setBusy(true);
    invoke<F>("convert_clipboard_hack", {
      file,
      index,
      name: preview.candidate.name,
      expectedHash: hash,
    })
      .then(onConverted)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="eng-offer">
      <p>
        The macro <span className="placeholder">{`{{${preview.candidate.name}}}`}</span> copies text with PowerShell. It can
        be a Set clipboard macro instead, which is faster and works on every system.
      </p>
      {!open ? (
        <button type="button" onClick={() => setOpen(true)}>
          Preview conversion
        </button>
      ) : (
        <>
          <div className="eng-diff">
            <div>
              <h4>Now</h4>
              <pre>{preview.before.join("\n")}</pre>
            </div>
            <div>
              <h4>After</h4>
              <pre>{preview.after.join("\n")}</pre>
            </div>
          </div>
          <div className="eng-actions">
            <button type="button" onClick={convert} disabled={busy}>
              Convert
            </button>
            <button type="button" onClick={() => setOpen(false)} disabled={busy}>
              Cancel
            </button>
          </div>
        </>
      )}
      {error && (
        <p className="eng-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

export interface EngineRun {
  running: boolean;
  stopped_by_user: boolean;
  at_login: boolean;
  problem: string | null;
}

/** Settings: is expansion on, start/stop it, and whether it starts when you sign in. */
export function ExpansionSection() {
  const [run, setRun] = useState<EngineRun | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<EngineRun>("engine_run_state")
      .then((r) => alive && setRun(r))
      .catch(() => alive && setRun(null));
    return () => {
      alive = false;
    };
  }, []);

  async function act(command: string, args?: Record<string, unknown>) {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<EngineRun>(command, args);
      setRun(next);
      if (next.problem) setError(next.problem);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!run) return null;
  return (
    <>
      <h3 className="df-h3">Expansion</h3>
      <p className="fm-help" role="status">
        {run.running
          ? "On. Your phrases expand as you type, in any program."
          : run.stopped_by_user
            ? "Off. You turned it off. Phrases do not expand until you turn it on."
            : "Off. Phrases are not expanding."}
      </p>
      <div className="fm-buttons">
        {run.running ? (
          <>
            <button type="button" className="btn" disabled={busy} onClick={() => act("engine_restart")}>
              Restart
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => act("engine_stop")}>
              Turn off
            </button>
          </>
        ) : (
          <button type="button" className="btn btn-primary" disabled={busy} onClick={() => act("engine_start")}>
            Turn on
          </button>
        )}
      </div>
      <label className="eng-check">
        <input
          type="checkbox"
          checked={run.at_login}
          disabled={busy}
          onChange={(e) => act("engine_set_at_login", { on: e.target.checked })}
        />
        Start expanding when I sign in to this computer
      </label>
      {error && (
        <p className="fm-problem" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
