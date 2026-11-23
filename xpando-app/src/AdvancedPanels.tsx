import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, FileCode2 } from "lucide-react";
import { OPERATORS } from "./search";

const STORAGE_KEY = "xpando.advanced";

/** Advanced mode on/off, remembered in localStorage when it is available. */
export function useAdvanced(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState<boolean>(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) === "1";
    } catch {
      return false;
    }
  });
  const set = (value: boolean) => {
    setOn(value);
    try {
      localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
    } catch {
      // Storage unavailable: the setting just lasts for this session.
    }
  };
  return [on, set];
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

/** The phrase's exact source text with the file's real line numbers. */
/** `version` is the file's content hash: when it changes (after a save) the source is reloaded. */
export function YamlPane({ file, index, version }: { file: string; index: number; version: string }) {
  const [source, setSource] = useState<EntrySource | null>(null);
  const [state, setState] = useState<"loading" | "ok" | "unavailable" | "error">("loading");
  const [message, setMessage] = useState("");

  useEffect(() => {
    let current = true;
    setState("loading");
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
        a flow-style list).
      </p>
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
                {text || " "}
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
}: {
  configPath: string | null;
  phrases: number;
  files: number;
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
    </footer>
  );
}
