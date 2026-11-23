import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Folder as FolderIcon, Library, Panda, Search } from "lucide-react";
import {
  AdvancedSwitch,
  ConflictsPanel,
  DetailTabs,
  FileMeta,
  SearchHint,
  StatusBar,
  YamlPane,
  useAdvanced,
  type Conflict,
  type DetailTab,
} from "./AdvancedPanels";
import { matchesTerms, parseQuery } from "./search";

interface Variable {
  name: string;
  kind: string;
  params: [string, string][];
}

interface Phrase {
  triggers: string[];
  is_regex: boolean;
  label: string | null;
  kind: string;
  content: string;
  preview: string;
  vars: Variable[];
  comment: string | null;
  index: number;
  line: number | null;
}

interface FolderData {
  name: string;
  file: string;
  phrases: Phrase[];
  warnings: string[];
}

interface Row {
  id: string;
  folder: string;
  folderName: string;
  phrase: Phrase;
}

const ALL = "*";

const KIND_LABEL: Record<string, string> = {
  replace: "Plain text",
  html: "Rich text (HTML)",
  markdown: "Markdown",
  image_path: "Image path",
  form: "Form",
};

function triggerText(p: Phrase): string {
  return p.is_regex ? "regex" : p.triggers.join("  ");
}

const MAX_COMMENT_TITLE = 40;

/**
 * A name taken from a comment: the whole comment if it is one short line, else the part before
 * a " (note)" suffix, e.g. "CR COURSES (assumed TortoiseGit)" -> "CR COURSES". Otherwise none.
 */
function commentName(comment: string | null): string | null {
  if (!comment || comment.includes("\n")) return null;
  if (comment.length <= MAX_COMMENT_TITLE) return comment;
  const head = comment.split(" (")[0].trim();
  return head && head !== comment && head.length <= MAX_COMMENT_TITLE ? head : null;
}

/**
 * Name shown for a phrase: its label, else a name read from the comment above it, else its
 * trigger. Longer comments are notes and are shown as the description instead.
 */
function title(p: Phrase): string {
  return p.label || commentName(p.comment) || p.triggers[0] || "(untitled)";
}

const PLACEHOLDER_ONLY = /^(\s*\{\{[^}]+\}\}\s*)+$/;

/** A body made only of {{placeholders}} says nothing, so describe how it is built. */
function previewText(p: Phrase): string {
  if (p.vars.length > 0 && PLACEHOLDER_ONLY.test(p.content)) {
    const kinds = [...new Set(p.vars.map((v) => v.kind).filter(Boolean))];
    return `Built from ${kinds.join(", ")}`;
  }
  return p.preview;
}

/** Renders text with each {{placeholder}} highlighted. */
function withPlaceholders(text: string) {
  return text.split(/(\{\{[^}]+\}\})/).map((part, i) =>
    /^\{\{[^}]+\}\}$/.test(part) ? (
      <span className="placeholder" key={i}>
        {part}
      </span>
    ) : (
      part
    ),
  );
}

function matches(row: Row, terms: ReturnType<typeof parseQuery>): boolean {
  const p = row.phrase;
  return matchesTerms(
    {
      triggers: p.triggers,
      label: title(p),
      comment: p.comment ?? "",
      body: p.content,
      folder: row.folderName,
      file: row.folder,
    },
    terms,
  );
}

export default function App() {
  const [folders, setFolders] = useState<FolderData[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scope, setScope] = useState<string>(ALL);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [advanced, setAdvanced] = useAdvanced();
  const [tab, setTab] = useState<DetailTab>("content");
  const [searchFocused, setSearchFocused] = useState(false);
  const [conflicts, setConflicts] = useState<Conflict[]>([]);
  const [configPath, setConfigPath] = useState<string | null>(null);

  useEffect(() => {
    invoke<FolderData[]>("list_phrases")
      .then((data) =>
        // Tolerate a backend that predates the `vars` field instead of crashing the page.
        setFolders(
          data.map((f) => ({ ...f, phrases: f.phrases.map((p) => ({ ...p, vars: p.vars ?? [] })) })),
        ),
      )
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    if (!advanced) return;
    invoke<Conflict[]>("list_conflicts")
      .then(setConflicts)
      .catch(() => setConflicts([]));
    invoke<string>("config_path")
      .then(setConfigPath)
      .catch(() => setConfigPath(null));
  }, [advanced, folders]);

  const rows = useMemo<Row[]>(
    () =>
      folders.flatMap((f) =>
        f.phrases.map((phrase, i) => ({
          id: `${f.file}#${i}`,
          folder: f.file,
          folderName: f.name,
          phrase,
        })),
      ),
    [folders],
  );
  const terms = useMemo(() => parseQuery(query, advanced), [query, advanced]);
  const visible = useMemo(
    () => rows.filter((r) => (scope === ALL || r.folder === scope) && matches(r, terms)),
    [rows, scope, terms],
  );
  const jumpTo = (file: string, index: number) => {
    const row = rows.find((r) => r.folder === file && r.phrase.index === index);
    if (!row) return;
    setScope(ALL);
    setQuery("");
    setSelected(row.id);
  };
  const current = visible.find((r) => r.id === selected) ?? null;
  const scopeFolder = folders.find((f) => f.file === scope);
  const scopeName = scope === ALL ? "All phrases" : (scopeFolder?.name ?? "");
  const warnings = folders.flatMap((f) => f.warnings.map((w) => `${f.file}: ${w}`));

  return (
    <div className="app">
      <header className="header">
        <div className="brand">
          <Panda size={24} strokeWidth={2} aria-hidden="true" />
          <span className="wordmark">xpando</span>
        </div>
        <div className="search">
          <Search size={16} aria-hidden="true" />
          <input
            type="search"
            aria-label="Search phrases"
            placeholder="Search triggers, names and text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setSearchFocused(true)}
            onBlur={() => setSearchFocused(false)}
            aria-describedby={advanced ? "search-hint" : undefined}
          />
        </div>
        <AdvancedSwitch on={advanced} onChange={setAdvanced} />
      </header>
      {advanced && (searchFocused || !query) && <SearchHint />}

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {warnings.length > 0 && (
        <details className="warnings">
          <summary>
            <AlertTriangle size={16} aria-hidden="true" />
            {warnings.length} {warnings.length === 1 ? "entry" : "entries"} could not be read
          </summary>
          <ul>
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}

      <div className="panes">
        <nav className="pane sidebar" aria-label="Folders">
          <ul>
            <li>
              <button
                type="button"
                className="folder"
                aria-current={scope === ALL}
                onClick={() => setScope(ALL)}
              >
                <Library size={16} aria-hidden="true" />
                <span className="grow">All phrases</span>
                <span className="count">{rows.length}</span>
              </button>
            </li>
            {folders.map((f) => (
              <li key={f.file}>
                <button
                  type="button"
                  className="folder"
                  aria-current={scope === f.file}
                  title={f.file}
                  onClick={() => setScope(f.file)}
                >
                  <FolderIcon size={16} aria-hidden="true" />
                  <span className="grow">
                    {f.name}
                    {advanced && <span className="adv-folder-file">{f.file}</span>}
                  </span>
                  <span className="count">{f.phrases.length}</span>
                </button>
              </li>
            ))}
          </ul>
          {advanced && <ConflictsPanel conflicts={conflicts} onJump={jumpTo} />}
        </nav>

        <section className="pane list" aria-label={`${scopeName} phrases`}>
          <h2 className="pane-title">
            {scopeName} <span className="count">{visible.length}</span>
          </h2>
          {loaded && visible.length === 0 && (
            <p className="empty">
              {query ? "No phrases match your search." : "No phrases found in your espanso match files."}
            </p>
          )}
          <ul>
            {visible.map((r) => (
              <li key={r.id}>
                <button
                  type="button"
                  className="phrase"
                  aria-current={r.id === selected}
                  onClick={() => setSelected(r.id)}
                >
                  <span className="phrase-top">
                    <span className="phrase-name">{title(r.phrase)}</span>
                    {title(r.phrase) !== triggerText(r.phrase) && (
                      <span className="chip">{triggerText(r.phrase)}</span>
                    )}
                  </span>
                  <span className="preview">{previewText(r.phrase)}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>

        <section className="pane detail" aria-label="Phrase details">
          {current ? (
            <>
              <h2 className="detail-title">{title(current.phrase)}</h2>
              {advanced && <FileMeta file={current.folder} line={current.phrase.line} />}
              {current.phrase.comment && current.phrase.comment !== title(current.phrase) && (
                <p className="detail-comment">{current.phrase.comment}</p>
              )}
              {advanced && <DetailTabs tab={tab} onChange={setTab} />}
              {advanced && tab === "yaml" ? (
                <div role="tabpanel" id="detail-tabpanel" aria-labelledby="detail-tab-yaml">
                  <YamlPane file={current.folder} index={current.phrase.index} />
                </div>
              ) : (
              <div role={advanced ? "tabpanel" : undefined} id={advanced ? "detail-tabpanel" : undefined}>
              <p className="detail-meta">
                {current.phrase.is_regex ? "Regex trigger" : "Trigger"}
                {current.phrase.triggers.length > 1 ? "s" : ""}
              </p>
              <div className="chips">
                {current.phrase.triggers.map((t) => (
                  <span className="chip" key={t}>
                    {t}
                  </span>
                ))}
              </div>
              <p className="detail-meta">{KIND_LABEL[current.phrase.kind] ?? current.phrase.kind}</p>
              <pre className="content-text">{withPlaceholders(current.phrase.content)}</pre>
              {current.phrase.vars.length > 0 && (
                <>
                  <h3 className="detail-meta">Variables</h3>
                  <ul className="vars">
                    {current.phrase.vars.map((v) => (
                      <li className="var" key={v.name}>
                        <div className="var-head">
                          <span className="placeholder">{`{{${v.name}}}`}</span>
                          <span className="var-kind">{v.kind || "unknown type"}</span>
                        </div>
                        {v.params.length > 0 && (
                          <dl className="var-params">
                            {v.params.map(([k, val]) => (
                              <div key={k}>
                                <dt>{k}</dt>
                                <dd>{withPlaceholders(val)}</dd>
                              </div>
                            ))}
                          </dl>
                        )}
                      </li>
                    ))}
                  </ul>
                </>
              )}
              </div>
              )}
            </>
          ) : (
            <p className="empty">Select a phrase to see its details.</p>
          )}
        </section>
      </div>
      {advanced && <StatusBar configPath={configPath} phrases={rows.length} files={folders.length} />}
    </div>
  );
}
