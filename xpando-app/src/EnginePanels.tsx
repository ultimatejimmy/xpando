import { Fragment, useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle } from "lucide-react";
import { SettingsGroup, SettingsRow, SwitchRow } from "./SettingsParts";
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

/** The key name the engine uses for a pressed key, or null for keys it cannot take. */
function engineKey(e: KeyboardEvent): string | null {
  const c = e.code;
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit[0-9]$/.test(c)) return c.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(c)) return c;
  const named: Record<string, string> = {
    Space: "SPACE",
    Enter: "ENTER",
    Tab: "TAB",
    ArrowUp: "UP",
    ArrowDown: "DOWN",
    ArrowLeft: "LEFT",
    ArrowRight: "RIGHT",
    Home: "HOME",
    End: "END",
    PageUp: "PAGEUP",
    PageDown: "PAGEDOWN",
    Insert: "INSERT",
    Delete: "DELETE",
  };
  return named[c] ?? null;
}

function prettyKey(k: string): string {
  return k.length === 1 ? k : k.charAt(0) + k.slice(1).toLowerCase();
}

/** "Press the keys you want" row for the shortcut that opens the search box. */
function SearchShortcutRow() {
  const [value, setValue] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("search_shortcut_get")
      .then(setValue)
      .catch(() => setValue(null));
  }, []);

  const save = useCallback(async (next: string) => {
    setBusy(true);
    setError(null);
    try {
      setValue(await invoke<string>("search_shortcut_set", { value: next }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (["Control", "Alt", "Shift", "Meta", "AltGraph"].includes(e.key)) return;
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      const key = engineKey(e);
      const mods = [e.ctrlKey && "CTRL", e.altKey && "ALT", e.shiftKey && "SHIFT"].filter(Boolean) as string[];
      if (!key) {
        setError("That key cannot be used. Try a letter, number, F key or Space.");
        setRecording(false);
        return;
      }
      if (mods.length === 0) {
        setError("Hold Ctrl or Alt while you press the key.");
        setRecording(false);
        return;
      }
      setRecording(false);
      void save([...mods, key].join("+"));
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [recording, save]);

  if (value === null) return null;
  const parts = value === "OFF" ? [] : value.split("+");
  return (
    <SettingsRow
      title="Open the search box"
      help={value === "OFF" ? "Off. Choose a shortcut to turn it on." : "Press this anywhere to find a phrase."}
      below={
        error ? (
          <p className="st-notice" role="alert">
            {error}
          </p>
        ) : undefined
      }
    >
      {recording ? (
        <span className="st-keys" role="status">
          Press the keys you want…
        </span>
      ) : (
        <span className="st-keys">
          {parts.map((p, i) => (
            <Fragment key={p + i}>
              {i > 0 && "+"}
              <kbd className="st-cap">{prettyKey(p)}</kbd>
            </Fragment>
          ))}
        </span>
      )}
      <button
        type="button"
        className="btn st-key"
        disabled={busy}
        onClick={() => {
          setError(null);
          setRecording((r) => !r);
        }}
      >
        {recording ? "Cancel" : busy ? "Saving..." : "Change"}
      </button>
    </SettingsRow>
  );
}

/** Settings: is expansion on, whether it starts at sign-in, and the search box shortcut. */
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
    <SettingsGroup title="Expansion and search">
      {!run.running && (
        <SettingsRow
          title="Expansion is off"
          help={run.stopped_by_user ? "You turned it off. Phrases do not expand until you turn it on." : "Phrases are not expanding."}
        >
          <button type="button" className="btn btn-primary" disabled={busy} onClick={() => void act("engine_start")}>
            Turn on
          </button>
        </SettingsRow>
      )}
      <SwitchRow
        title="Start when I sign in to this computer"
        checked={run.at_login}
        disabled={busy}
        onChange={(on) => void act("engine_set_at_login", { on })}
      />
      <SearchShortcutRow />
      {error && (
        <div className="st-row">
          <p className="fm-problem" role="alert" style={{ margin: 0 }}>
            {error}
          </p>
        </div>
      )}
    </SettingsGroup>
  );
}
