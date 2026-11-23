import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Folder as FolderIcon, Library, Panda, Search } from "lucide-react";

interface Phrase {
  triggers: string[];
  is_regex: boolean;
  label: string | null;
  kind: string;
  content: string;
  preview: string;
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

function title(p: Phrase): string {
  return p.label || p.triggers[0] || "(untitled)";
}

function matches(row: Row, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const p = row.phrase;
  return [...p.triggers, p.label ?? "", p.content].some((s) => s.toLowerCase().includes(q));
}

export default function App() {
  const [folders, setFolders] = useState<FolderData[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scope, setScope] = useState<string>(ALL);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    invoke<FolderData[]>("list_phrases")
      .then(setFolders)
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
  }, []);

  const rows = useMemo<Row[]>(
    () =>
      folders.flatMap((f) =>
        f.phrases.map((phrase, i) => ({ id: `${f.file}#${i}`, folder: f.file, phrase })),
      ),
    [folders],
  );
  const visible = useMemo(
    () => rows.filter((r) => (scope === ALL || r.folder === scope) && matches(r, query)),
    [rows, scope, query],
  );
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
          />
        </div>
      </header>

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
                  <span className="grow">{f.name}</span>
                  <span className="count">{f.phrases.length}</span>
                </button>
              </li>
            ))}
          </ul>
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
                    <span className="chip">{triggerText(r.phrase)}</span>
                  </span>
                  <span className="preview">{r.phrase.preview}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>

        <section className="pane detail" aria-label="Phrase details">
          {current ? (
            <>
              <h2 className="detail-title">{title(current.phrase)}</h2>
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
              <pre className="content-text">{current.phrase.content}</pre>
            </>
          ) : (
            <p className="empty">Select a phrase to see its details.</p>
          )}
        </section>
      </div>
    </div>
  );
}
