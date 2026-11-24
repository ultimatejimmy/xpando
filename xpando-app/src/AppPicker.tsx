import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, FolderOpen, Plus, RefreshCw, Search, X } from "lucide-react";
import {
  addExes,
  chipsFor,
  filterRows,
  isWindows,
  parseTypedExes,
  programRows,
  removeExes,
  toggleRow,
  rowSelected,
  type Program,
  type Row,
} from "./knownApps";
import { friendlyNames } from "./worksInLogic";

type Source = "open" | "installed";

const MAX_ROWS = 150;

function errorText(e: unknown): string {
  return String(e);
}

/**
 * Chooses programs by name: a search box over two lists (programs installed on this computer and
 * programs that are open now), a Browse button for a program file, and
 * removable chips for what is chosen. Used for a folder's apps and for a phrase's "Works in".
 * In Advanced mode there is also a field for typing a file name such as winword.exe.
 *
 * `value` is the list of program file names. `onChange` gets the new list and a friendly name
 * for each distinct program (used to name a new folder).
 */
export function AppPicker({
  value,
  onChange,
  advanced,
}: {
  value: string[];
  onChange: (exes: string[], names: string[]) => void;
  advanced: boolean;
}) {
  const uid = useId();
  const windows = useMemo(() => typeof navigator !== "undefined" && isWindows(navigator.userAgent), []);
  const sources: { id: Source; label: string }[] = windows
    ? [
        { id: "installed", label: "Installed" },
        { id: "open", label: "Open now" },
      ]
    : [{ id: "open", label: "Open now" }];
  const [source, setSource] = useState<Source>(sources[0].id);
  const [query, setQuery] = useState("");
  const [running, setRunning] = useState<Program[] | null>(null);
  const [installed, setInstalled] = useState<Program[] | null>(null);
  const [loading, setLoading] = useState<Source | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [typed, setTyped] = useState("");
  const [typedProblem, setTypedProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const seen = useMemo(() => [...(running ?? []), ...(installed ?? [])], [running, installed]);

  const change = useCallback(
    (next: string[]) => onChange(next, friendlyNames(next, seen)),
    [onChange, seen],
  );

  const load = useCallback(async (which: "open" | "installed", refresh: boolean) => {
    setLoading(which);
    setProblem(null);
    try {
      if (which === "open") setRunning(await invoke<Program[]>("list_running_programs"));
      else setInstalled(await invoke<Program[]>("list_installed_programs", { refresh }));
    } catch (e) {
      setProblem(errorText(e));
      if (which === "open") setRunning((r) => r ?? []);
      else setInstalled((r) => r ?? []);
    } finally {
      setLoading(null);
    }
  }, []);

  // The two system lists are fetched when their tab is first shown.
  useEffect(() => {
    if (source === "open" && running === null) void load("open", false);
    if (source === "installed" && installed === null) void load("installed", false);
  }, [source, running, installed, load]);

  const rows: Row[] = useMemo(() => {
    const all = programRows(source === "open" ? (running ?? []) : (installed ?? []));
    return filterRows(all, query);
  }, [source, running, installed, query]);
  const shown = rows.slice(0, MAX_ROWS);

  useEffect(() => setActive(0), [source, query]);

  function onListKey(e: KeyboardEvent<HTMLUListElement>) {
    const keys: Record<string, number> = {
      ArrowDown: Math.min(active + 1, shown.length - 1),
      ArrowUp: Math.max(active - 1, 0),
      Home: 0,
      End: shown.length - 1,
    };
    if (!(e.key in keys) || shown.length === 0) return;
    e.preventDefault();
    const next = keys[e.key];
    setActive(next);
    listRef.current?.querySelectorAll<HTMLElement>("button")[next]?.focus();
  }

  function onTabKey(e: KeyboardEvent<HTMLDivElement>) {
    const at = sources.findIndex((s) => s.id === source);
    const move = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!move) return;
    e.preventDefault();
    const next = sources[(at + move + sources.length) % sources.length];
    setSource(next.id);
    document.getElementById(`${uid}-tab-${next.id}`)?.focus();
  }

  async function browse() {
    setNote(null);
    try {
      const p = await invoke<Program | null>("pick_program");
      if (p) {
        change(addExes(value, [p.exe]));
        setRunning((r) => (r && !r.some((x) => x.exe.toLowerCase() === p.exe.toLowerCase()) ? [...r, p] : r));
        setNote(`Added ${p.name}.`);
      }
    } catch (e) {
      setProblem(errorText(e));
    }
  }

  function addTyped() {
    const { exes, rejected } = parseTypedExes(typed);
    setTypedProblem(
      rejected.length > 0
        ? `Not a program file name: ${rejected.join(", ")}. Use only the file name, for example winword.exe.`
        : null,
    );
    if (exes.length > 0) {
      change(addExes(value, exes));
      if (rejected.length === 0) setTyped("");
    }
  }

  const chips = chipsFor(value, seen);
  const emptyText =
    source === "open"
      ? "No open programs were found. Open the program first, or use Browse."
      : source === "installed"
        ? "No installed programs were found."
        : "No program matches.";

  return (
    <div className="ap">
      <p className="ap-label" id={`${uid}-chosen`}>
        Chosen programs
      </p>
      <ul className="fm-chips" aria-labelledby={`${uid}-chosen`}>
        {chips.map((c) => (
          <li key={c.exes.join("|")} className="fm-chip">
            {c.label}
            <button
              type="button"
              className="fm-chip-x"
              aria-label={`Remove ${c.label}`}
              onClick={() => change(removeExes(value, c.exes))}
            >
              <X size={14} aria-hidden="true" />
            </button>
          </li>
        ))}
        {chips.length === 0 && <li className="fm-help">None yet. Pick from the list below.</li>}
      </ul>

      <div className="ap-search">
        <Search size={16} aria-hidden="true" />
        <input
          className="field"
          type="search"
          aria-label="Search programs"
          placeholder="Search programs"
          autoComplete="off"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="button" className="btn" onClick={() => void browse()}>
          <FolderOpen size={16} aria-hidden="true" />
          Browse…
        </button>
      </div>

      <div className="ap-tabs" role="tablist" aria-label="Where to look for programs" onKeyDown={onTabKey}>
        {sources.map((s) => (
          <button
            key={s.id}
            type="button"
            role="tab"
            id={`${uid}-tab-${s.id}`}
            className="ap-tab"
            aria-selected={source === s.id}
            aria-controls={`${uid}-panel`}
            tabIndex={source === s.id ? 0 : -1}
            onClick={() => setSource(s.id)}
          >
            {s.label}
          </button>
        ))}
        <button
          type="button"
          className="btn ap-refresh"
          onClick={() => void load(source === "open" ? "open" : "installed", true)}
          disabled={loading !== null}
          aria-label={source === "open" ? "Look again for open programs" : "Look again for installed programs"}
        >
          <RefreshCw size={14} aria-hidden="true" />
          Refresh
        </button>
      </div>

      <div role="tabpanel" id={`${uid}-panel`} aria-labelledby={`${uid}-tab-${source}`} className="ap-panel">
        <p className="fm-help ap-status" role="status" aria-live="polite">
          {loading === source
            ? source === "installed"
              ? "Looking for installed programs. This can take a few seconds…"
              : "Looking for open programs…"
            : (note ?? "")}
        </p>
        {problem && (
          <p className="fm-problem" role="alert">
            {problem}
          </p>
        )}
        {loading !== source && shown.length === 0 && <p className="fm-help">{emptyText}</p>}
        <ul className="ap-list" ref={listRef} aria-label={`${sources.find((s) => s.id === source)?.label} programs`} onKeyDown={onListKey}>
          {shown.map((r, i) => {
            const on = rowSelected(r, value);
            return (
              <li key={r.id}>
                <button
                  type="button"
                  className="ap-row"
                  aria-pressed={on}
                  tabIndex={i === active ? 0 : -1}
                  onFocus={() => setActive(i)}
                  onClick={() => change(toggleRow(value, r))}
                >
                  <span className="ap-check" aria-hidden="true">
                    {on && <Check size={14} />}
                  </span>
                  <span className="ap-name">{r.name}</span>
                  {advanced && <span className="ap-exe">{r.exes.join(", ")}</span>}
                </button>
              </li>
            );
          })}
        </ul>
        {rows.length > shown.length && (
          <p className="fm-help">Showing the first {MAX_ROWS} of {rows.length}. Type in the search box to narrow the list.</p>
        )}
      </div>

      {advanced && (
        <div className="ap-typed">
          <label htmlFor={`${uid}-typed`}>Advanced: type an exe name</label>
          <div className="fm-add">
            <input
              id={`${uid}-typed`}
              className="field"
              value={typed}
              placeholder="winword.exe"
              autoComplete="off"
              aria-describedby={`${uid}-typed-help`}
              onChange={(e) => setTyped(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addTyped();
                }
              }}
            />
            <button type="button" className="btn" onClick={addTyped} disabled={!typed.trim()}>
              <Plus size={16} aria-hidden="true" />
              Add
            </button>
          </div>
          <p id={`${uid}-typed-help`} className="fm-help">
            The program’s file name, for example winword.exe; several can be separated with commas. A full path is cut
            down to the file name. Task Manager, Details tab, shows it while the program runs.
          </p>
          {typedProblem && (
            <p className="fm-problem" role="alert">
              {typedProblem}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
