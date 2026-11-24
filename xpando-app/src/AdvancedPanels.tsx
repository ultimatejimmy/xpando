import { useEffect, useState, useSyncExternalStore, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, FileCode2, Pencil } from "lucide-react";
import { OPERATORS } from "./search";
import { EngineLine, type EngineStatus } from "./EnginePanels";
import { YamlEditor, hasDraft, type SavedFolder } from "./YamlEditor";

const STORAGE_KEY = "xpando.advanced";

const advancedListeners = new Set<() => void>();
let advancedValue: boolean | null = null;

function readAdvanced(): boolean {
  if (advancedValue === null) {
    try {
      advancedValue = localStorage.getItem(STORAGE_KEY) === "1";
    } catch {
      advancedValue = false;
    }
  }
  return advancedValue;
}

function setAdvancedValue(value: boolean) {
  advancedValue = value;
  try {
    localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
  } catch {
    // Storage unavailable: the setting just lasts for this session.
  }
  advancedListeners.forEach((l) => l());
}

function subscribeAdvanced(listener: () => void) {
  advancedListeners.add(listener);
  return () => {
    advancedListeners.delete(listener);
  };
}

/**
 * Advanced mode on/off, remembered in localStorage when it is available. One shared value:
 * every component that asks sees the switch change at once, even inside an open editor.
 */
export function useAdvanced(): [boolean, (on: boolean) => void] {
  const on = useSyncExternalStore(subscribeAdvanced, readAdvanced, () => false);
  return [on, setAdvancedValue];
}

export function AdvancedSwitch({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="adv-switch">
      <input type="checkbox" role="switch" checked={on} onChange={(e) => onChange(e.target.checked)} />
      <span className="adv-track" aria-hidden="true" />
      <span>Advanced</span>
    </label>
  );
}

export function SearchHint() {
  return (
    <p className="adv-hint" id="search-hint">
      Operators:{" "}
      {OPERATORS.map((o) => (
        <code key={o}>{o}:</code>
      ))}{" "}
      Use "quotes" for a phrase; terms are combined with AND.
    </p>
  );
}

export type DetailTab = "content" | "yaml";

export function DetailTabs({ tab, onChange }: { tab: DetailTab; onChange: (tab: DetailTab) => void }) {
  const tabs: [DetailTab, string][] = [
    ["content", "Content"],
    ["yaml", "YAML"],
  ];
  return (
    <div className="adv-tabs" role="tablist" aria-label="Phrase view">
      {tabs.map(([id, label]) => (
        <button
          key={id}
          type="button"
          role="tab"
          id={`detail-tab-${id}`}
          aria-selected={tab === id}
          aria-controls="detail-tabpanel"
          onClick={() => onChange(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** `file · line N` under the phrase title. */
export function FileMeta({ file, line }: { file: string; line: number | null }) {
  return (
    <p className="adv-filemeta">
      <FileCode2 size={14} aria-hidden="true" />
      <span>
        {file}
        {line != null && ` · line ${line}`}
      </span>
    </p>
  );
}

interface EntrySource {
  first_line: number;
  lines: string[];
  valid: boolean;
  error: string | null;
}

interface YamlPaneProps {
  file: string;
  index: number;
  /** The file's content hash: when it changes (after a save or reload) the source is reloaded. */
  version: string;
  /** Called with the reloaded folder after the YAML was saved. */
  onSaved: (folder: SavedFolder) => void;
  /** Reloads every folder from disk. */
  onReload: () => Promise<unknown>;
}

/** The phrase's exact source text with the file's real line numbers, and its editor. */
export function YamlPane(props: YamlPaneProps) {
  // A new phrase gets a fresh pane, so edit mode never leaks between phrases.
  return <YamlPaneFor key={`${props.file}#${props.index}`} {...props} />;
}

function YamlPaneFor({ file, index, version, onSaved, onReload }: YamlPaneProps) {
  const [source, setSource] = useState<EntrySource | null>(null);
  const [state, setState] = useState<"loading" | "ok" | "unavailable" | "error">("loading");
  const [message, setMessage] = useState("");
  const [editing, setEditing] = useState(() => hasDraft(file, index));

  useEffect(() => {
    let current = true;
    // The old source stays on screen while a reload runs, so an open editor is not torn down.
    invoke<EntrySource | null>("get_entry_source", { file, index })
      .then((s) => {
        if (!current) return;
        setSource(s);
        setState(s ? "ok" : "unavailable");
      })
      .catch((e) => {
        if (!current) return;
        setMessage(String(e));
        setState("error");
      });
    return () => {
      current = false;
    };
  }, [file, index, version]);

  if (state === "loading") return <p className="empty">Loading source...</p>;
  if (state === "error") {
    return (
      <p className="error" role="alert">
        {message}
      </p>
    );
  }
  if (state === "unavailable" || !source) {
    return (
      <p className="empty">
        The exact source of this phrase cannot be located reliably in {file} (for example, the file uses
        a flow-style list), so it cannot be edited here either.
      </p>
    );
  }
  if (editing) {
    return (
      <YamlEditor
        file={file}
        index={index}
        entryBase={source.lines.join("\n")}
        firstLine={source.first_line}
        hash={version}
        onSaved={(f) => {
          setEditing(false);
          onSaved(f);
        }}
        onClose={() => setEditing(false)}
        onReload={onReload}
      />
    );
  }
  return (
    <div className="adv-yaml">
      <div className="adv-yaml-head">
        <span className="adv-yaml-file">{file}</span>
        {source.valid ? (
          <span className="adv-status adv-ok">
            <CheckCircle2 size={14} aria-hidden="true" />
            Valid
          </span>
        ) : (
          <span className="adv-status adv-bad">
            <AlertTriangle size={14} aria-hidden="true" />
            Invalid{source.error ? `: ${source.error}` : ""}
          </span>
        )}
        <button type="button" className="btn" onClick={() => setEditing(true)}>
          <Pencil size={16} aria-hidden="true" />
          Edit YAML
        </button>
      </div>
      <table className="adv-code" aria-label={`Source of ${file}`}>
        <tbody>
          {source.lines.map((text, i) => (
            <tr key={i}>
              <td className="adv-ln">{source.first_line + i}</td>
              <td
                className={text.trimStart().startsWith("#") ? "adv-src adv-cmt" : "adv-src"}
                style={{ "--indent": text.length - text.trimStart().length } as CSSProperties}
              >
                {text || " "}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export interface ConflictPhrase {
  file: string;
  index: number;
  trigger: string;
  title: string;
  word: "none" | "word" | "left_word" | "right_word";
  role: "duplicate" | "shorter" | "longer";
}

export interface Conflict {
  kind: "duplicate" | "prefix";
  certainty: "definite" | "possible";
  low_risk: boolean;
  trigger: string;
  other_trigger: string | null;
  phrases: ConflictPhrase[];
}

const WORD_LABEL: Record<ConflictPhrase["word"], string> = {
  none: "",
  word: "word",
  left_word: "left_word",
  right_word: "right_word",
};

function conflictSummary(c: Conflict): string {
  if (c.kind === "duplicate") {
    return `${c.trigger} is defined ${c.phrases.length} times`;
  }
  return `${c.trigger} is a prefix of ${c.other_trigger}`;
}

function conflictNote(c: Conflict): string {
  const certainty = c.certainty === "definite" ? "Same file" : "Different files (possible)";
  if (c.kind === "duplicate") return certainty;
  const risk = c.low_risk ? "Lower risk: word option on the shorter trigger" : "Shorter trigger fires first";
  return `${certainty} · ${risk}`;
}

/** Collapsible list of trigger conflicts at the bottom of the sidebar. */
export function ConflictsPanel({
  conflicts,
  onJump,
}: {
  conflicts: Conflict[];
  onJump: (file: string, index: number) => void;
}) {
  const [open, setOpen] = useState(false);
  if (conflicts.length === 0) return null;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <section className="adv-conflicts" aria-label="Trigger conflicts">
      <button type="button" className="adv-conflicts-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <AlertTriangle size={16} aria-hidden="true" />
        <span className="grow">
          {conflicts.length} trigger {conflicts.length === 1 ? "conflict" : "conflicts"}
        </span>
        <Chevron size={16} aria-hidden="true" />
      </button>
      {open && (
        <ul className="adv-conflict-list">
          {conflicts.map((c, i) => (
            <li key={i} className="adv-conflict">
              <p className="adv-conflict-title">{conflictSummary(c)}</p>
              <p className="adv-conflict-note">{conflictNote(c)}</p>
              <ul>
                {c.phrases.map((p, j) => (
                  <li key={j}>
                    <button type="button" onClick={() => onJump(p.file, p.index)}>
                      <span>{p.title}</span>
                      <span className="adv-conflict-where">
                        {p.trigger} · {p.file}
                        {p.word !== "none" && ` · ${WORD_LABEL[p.word]}`}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function StatusBar({
  configPath,
  phrases,
  files,
  engine,
}: {
  configPath: string | null;
  phrases: number;
  files: number;
  engine: EngineStatus | null;
}) {
  return (
    <footer className="adv-statusbar">
      <span className="adv-status-path" title={configPath ?? undefined}>
        Config: {configPath ?? "unknown"}
      </span>
      <span>
        {phrases} {phrases === 1 ? "phrase" : "phrases"}
      </span>
      <span>
        {files} {files === 1 ? "file" : "files"}
      </span>
      <EngineLine engine={engine} />
    </footer>
  );
}
