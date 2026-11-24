import { useCallback, useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpCircle, X } from "lucide-react";
import "./updates.css";

// ---------------------------------------------------------------------------------------
// Types (the backend's JSON; see src-tauri/src/update.rs)
// ---------------------------------------------------------------------------------------

export interface UpdateInfo {
  /** The human version, e.g. `2026.11.23` or `2026.11.23-beta2`. */
  version: string;
  beta: boolean;
  installer: { name: string; size: number };
}

interface UpdateState {
  current: string;
  dev_build: boolean;
  beta: boolean;
  auto_check: boolean;
  last_check: number | null;
  skipped: string | null;
  note: string | null;
  can_install: boolean;
}

type CheckOutcome =
  | { status: "not_due" | "dev_build" | "up_to_date" }
  | { status: "available"; update: UpdateInfo };

type InstallOutcome = { kind: "launched" } | { kind: "revealed"; path: string };

export type InstallState =
  | { phase: "idle" }
  | { phase: "downloading" | "verifying" | "starting"; done: number; total: number }
  | { phase: "launched" }
  | { phase: "revealed"; path: string }
  | { phase: "error"; message: string };

export interface UpdatesApi {
  state: UpdateState | null;
  /** The update to offer, if one was found. */
  available: UpdateInfo | null;
  /** "Later" was chosen for this version (until xpando restarts or the next manual check). */
  later: boolean;
  checking: boolean;
  /** What the last manual check found, in plain words. */
  message: string | null;
  install: InstallState;
  check: (manual: boolean) => Promise<void>;
  setPrefs: (autoCheck: boolean, beta: boolean) => Promise<void>;
  skip: () => Promise<void>;
  postpone: () => void;
  startInstall: () => Promise<void>;
  closeInstall: () => void;
}

/** How often the window asks whether the daily check is due; the backend decides. */
const DUE_POLL_MS = 30 * 60 * 1000;

function describe(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

// ---------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------

/**
 * Looks for updates once at start (when a day has passed since the last check) and every half
 * hour asks again, so a window left open for days still checks daily. Automatic checks fail
 * silently. Nothing is ever installed without the user pressing "Update now".
 */
export function useUpdates(): UpdatesApi {
  const [state, setState] = useState<UpdateState | null>(null);
  const [available, setAvailable] = useState<UpdateInfo | null>(null);
  const [later, setLater] = useState(false);
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [install, setInstall] = useState<InstallState>({ phase: "idle" });
  const busy = useRef(false);
  /** The version the banner was last shown for, so Later survives the next automatic check. */
  const shown = useRef<string | null>(null);

  const refresh = useCallback(
    () =>
      invoke<UpdateState>("update_state")
        .then(setState)
        .catch(() => undefined),
    [],
  );

  const check = useCallback(
    async (manual: boolean) => {
      if (busy.current) return;
      busy.current = true;
      if (manual) {
        setChecking(true);
        setMessage(null);
      }
      try {
        const r = await invoke<CheckOutcome>("check_for_update", { manual });
        if (r.status === "available") {
          if (manual || shown.current !== r.update.version) setLater(false);
          shown.current = r.update.version;
          setAvailable(r.update);
          if (manual) setMessage(`Version ${r.update.version}${r.update.beta ? " (beta)" : ""} is available.`);
        } else if (r.status === "up_to_date") {
          shown.current = null;
          setAvailable(null);
          if (manual) setMessage("You have the newest version.");
        } else if (r.status === "dev_build" && manual) {
          setMessage("This is a development build, which does not check for updates.");
        }
      } catch (e) {
        // Silent unless the user asked.
        if (manual) setMessage(`Could not check for updates. ${describe(e)}`);
      } finally {
        busy.current = false;
        if (manual) setChecking(false);
        void refresh();
      }
    },
    [refresh],
  );

  useEffect(() => {
    void refresh();
    void check(false);
    const timer = window.setInterval(() => void check(false), DUE_POLL_MS);
    return () => window.clearInterval(timer);
  }, [check, refresh]);

  useEffect(() => {
    const un = listen<{ phase: "downloading" | "verifying" | "starting"; done: number; total: number }>(
      "update-progress",
      (e) => setInstall({ phase: e.payload.phase, done: e.payload.done, total: e.payload.total }),
    );
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const setPrefs = useCallback(
    (autoCheck: boolean, beta: boolean) =>
      invoke<UpdateState>("set_update_prefs", { autoCheck, beta })
        .then((s) => {
          setState(s);
          // A different channel can change what is on offer: forget the old answer.
          shown.current = null;
          setAvailable(null);
          setMessage(null);
          if (autoCheck) void check(false);
        })
        .catch((e) => setMessage(describe(e))),
    [check],
  );

  const skip = useCallback(async () => {
    if (!available) return;
    await invoke("skip_update", { version: available.version }).catch(() => undefined);
    shown.current = null;
    setAvailable(null);
    void refresh();
  }, [available, refresh]);

  const startInstall = useCallback(async () => {
    if (!available || install.phase === "downloading") return;
    setInstall({ phase: "downloading", done: 0, total: available.installer.size });
    try {
      const r = await invoke<InstallOutcome>("install_update", { version: available.version });
      setInstall(r.kind === "launched" ? { phase: "launched" } : { phase: "revealed", path: r.path });
    } catch (e) {
      setInstall({ phase: "error", message: describe(e) });
    }
  }, [available, install.phase]);

  return {
    state,
    available,
    later,
    checking,
    message,
    install,
    check,
    setPrefs,
    skip,
    postpone: () => setLater(true),
    startInstall,
    closeInstall: () => setInstall({ phase: "idle" }),
  };
}

// ---------------------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------------------

function percent(done: number, total: number): number | null {
  return total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : null;
}

/** What is happening with an install, in plain words, with a progress bar while downloading. */
function InstallStatus({ u }: { u: UpdatesApi }) {
  const { install, available } = u;
  const version = available?.version ?? "";
  if (install.phase === "idle") return null;
  if (install.phase === "error") {
    return (
      <p className="fm-problem" role="alert">
        {install.message}
      </p>
    );
  }
  if (install.phase === "revealed") {
    return (
      <div role="status">
        <p>Version {version} was downloaded and checked. Open the file to install it:</p>
        <p className="up-path">
          <code>{install.path}</code>
        </p>
        <p className="fm-help">xpando cannot install itself on this system yet, so the last step is yours.</p>
      </div>
    );
  }
  if (install.phase === "launched" || install.phase === "starting") {
    return <p role="status">The installer is starting. xpando will close so the update can finish.</p>;
  }
  const pct = install.phase === "downloading" ? percent(install.done, install.total) : null;
  return (
    <div role="status">
      <p>
        {install.phase === "downloading"
          ? `Downloading version ${version}${pct === null ? "..." : `... ${pct}%`}`
          : "Checking that the download is complete and genuine..."}
      </p>
      {install.phase === "downloading" && <progress className="up-progress" max={100} value={pct ?? undefined} />}
    </div>
  );
}

/** The small banner under the header when a newer version exists. */
export function UpdateBanner({ u }: { u: UpdatesApi }) {
  const { available, install } = u;
  const installing = install.phase !== "idle";
  if (!available || (u.later && !installing)) return null;
  const can = u.state?.can_install ?? false;
  return (
    <div className="df-banner up-banner" role="region" aria-label="Update available">
      <ArrowUpCircle size={18} aria-hidden="true" />
      <div className="df-banner-body">
        {!installing && (
          <>
            <strong>
              Version {available.version} is available
              {available.beta && <span className="up-tag">Beta</span>}
            </strong>
            {available.beta && (
              <p>This is an early version. It may have problems that finished versions do not.</p>
            )}
          </>
        )}
        <InstallStatus u={u} />
      </div>
      {!installing && (
        <div className="df-banner-actions">
          <button type="button" className="btn btn-primary" onClick={() => void u.startInstall()}>
            {can ? "Update now" : "Download"}
          </button>
          <button type="button" className="btn" onClick={u.postpone}>
            Later
          </button>
          <button type="button" className="btn btn-quiet" onClick={() => void u.skip()}>
            Skip this version
          </button>
        </div>
      )}
      {(install.phase === "error" || install.phase === "revealed") && (
        <button type="button" className="btn btn-icon" aria-label="Close this message" onClick={u.closeInstall}>
          <X size={16} aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

/** The "Updates" part of the Settings dialog. */
export function UpdatesSection({ u }: { u: UpdatesApi }) {
  const autoId = useId();
  const betaId = useId();
  const s = u.state;
  const installing = u.install.phase !== "idle" && u.install.phase !== "error";
  return (
    <>
      <h3 className="df-h3">Updates</h3>
      <p className="fm-help">
        You have version {s?.current ?? "..."}
        {s?.dev_build ? " (a development build)" : ""}.
      </p>
      <div className="fm-buttons">
        <button type="button" className="btn" disabled={u.checking || installing} onClick={() => void u.check(true)}>
          {u.checking ? "Checking..." : "Check for updates"}
        </button>
        {u.available && u.install.phase === "idle" && (
          <button type="button" className="btn btn-primary" onClick={() => void u.startInstall()}>
            {s?.can_install ? "Update now" : "Download"}
          </button>
        )}
      </div>
      {u.message && (
        <p className="fm-help" role="status">
          {u.message}
        </p>
      )}
      <InstallStatus u={u} />
      <div className="fm-check up-check">
        <input
          id={autoId}
          type="checkbox"
          checked={s?.auto_check ?? true}
          disabled={!s}
          onChange={(e) => void u.setPrefs(e.target.checked, s?.beta ?? false)}
        />
        <label htmlFor={autoId}>Check automatically every day</label>
      </div>
      <div className="fm-check up-check">
        <input
          id={betaId}
          type="checkbox"
          checked={s?.beta ?? false}
          disabled={!s}
          onChange={(e) => void u.setPrefs(s?.auto_check ?? true, e.target.checked)}
        />
        <label htmlFor={betaId}>Get beta versions</label>
      </div>
      <p className="fm-help up-help">
        Betas are early versions that may have problems. Leave this off to get only finished versions.
      </p>
      {s?.note && <p className="fm-help">{s.note}</p>}
    </>
  );
}
