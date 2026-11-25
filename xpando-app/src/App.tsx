import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, AppWindow, ChevronDown, Folder as FolderIcon, Library, Panda, Search } from "lucide-react";
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
import {
  NewPhraseButton,
  NO_OPTIONS,
  OptionChips,
  PhraseActions,
  PhraseEditor,
  UnsavedChangesDialog,
  normalizeOptions,
  type ClipboardStart,
  type EditorOptions,
} from "./PhraseEditor";
import { useFolderInfo } from "./FolderManager";
import { FolderLabelMenu, NewFolderLabelButton, fileInFolder } from "./FolderLabels";
import {
  NO_FOLDER,
  allFolderNames,
  effectiveFolder,
  folderOfScope,
  folderScope,
  isFolderScope,
  sameFolder,
} from "./folderNames";
import { RichTextView } from "./RichLazy";
import { VarsAka, macroLabel } from "./VarsEditor";
import { ClipboardHackOffer, EngineWarning, useEngine } from "./EnginePanels";
import {
  DataBanners,
  ImportDialog,
  SettingsButton,
  SettingsDialog,
  useDataInfo,
  useImportOffer,
} from "./DataFolder";
import { UpdateBanner, useUpdates } from "./Updates";
import { MovedNotice, NewPhraseWorksIn, WorksIn, type MoveOutcome, type NewPlace } from "./WorksIn";
import { allAppsFolders, appCategories, isAppFolder, isAutoAppFolder, type AppCategory } from "./worksInLogic";
import { PaneGrip } from "./PaneGrip";
import { useAppIcons } from "./appIcons";

interface MacroInfo {
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
  vars: MacroInfo[];
  comment: string | null;
  index: number;
  line: number | null;
  options: EditorOptions;
}

interface FolderData {
  name: string;
  file: string;
  phrases: Phrase[];
  warnings: string[];
  hash: string;
}

/** Tolerate a backend that predates the `vars` field instead of crashing the page. */
function normalizeFolder(f: FolderData): FolderData {
  return {
    ...f,
    phrases: f.phrases.map((p) => ({ ...p, vars: p.vars ?? [], options: normalizeOptions(p.options ?? NO_OPTIONS) })),
  };
}

interface Row {
  id: string;
  folder: string;
  folderName: string;
  /** The xpando folder the phrase is in; `null` for no folder. */
  label: string | null;
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
    return `Made of ${kinds.map(macroLabel).join(", ")}`;
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

/** A pane width in px that is remembered between runs. */
function useStoredWidth(key: string, fallback: number, min: number, max: number) {
  const [width, setWidth] = useState(() => {
    try {
      const n = Number(localStorage.getItem(key));
      return n >= min && n <= max ? n : fallback;
    } catch {
      return fallback;
    }
  });
  const change = (w: number) => {
    setWidth(w);
    try {
      localStorage.setItem(key, String(w));
    } catch {
      /* the width just isn't remembered */
    }
  };
  return [width, change] as const;
}

/** The program's own icon when it was found, else the generic app-window icon. */
function AppIcon({ exes, icons }: { exes: string[]; icons: Record<string, string> }) {
  const url = exes.map((e) => icons[e.toLowerCase()]).find(Boolean);
  return url ? (
    <img className="app-icon" src={url} width={16} height={16} alt="" aria-hidden="true" />
  ) : (
    <AppWindow size={16} aria-hidden="true" />
  );
}

/** A new phrase made while one app is selected starts as "only in that app". */
function newPlaceFor(c: AppCategory | undefined): NewPlace {
  return c && c.exes.length > 0 ? { exes: c.exes, names: [c.name] } : null;
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
  const engine = useEngine();

  // Editing: an existing phrase (by row id) or a new phrase in a file.
  // The phrase is a snapshot taken when editing started; the editor compares it with the file.
  const [editing, setEditing] = useState<
    { id: string; file: string; base: Phrase } | { newIn: string; n: number; start?: ClipboardStart; place?: NewPlace; folder?: string | null } | null
  >(null);
  // Unsaved-changes guard: the editor reports `dirty`; a blocked action waits in `pendingLeave`.
  const [dirty, setDirty] = useState(false);
  const [pendingLeave, setPendingLeave] = useState<(() => void) | null>(null);
  const guard = (action: () => void) => {
    if (editing && dirty) setPendingLeave(() => action);
    else action();
  };
  const closeEditor = () => setEditing(null);
  const folderActionsLocked = editing !== null && dirty;

  const load = (): Promise<FolderData[] | null> =>
    invoke<FolderData[]>("list_phrases")
      .then((data) => {
        const fresh = data.map(normalizeFolder);
        setFolders(fresh);
        setError(null);
        return fresh;
      })
      .catch((e) => {
        setError(String(e));
        return null;
      })
      .finally(() => setLoaded(true));

  useEffect(() => {
    void load();
  }, []);

  // The data folder: where it is, whether it can be reached, sync conflict copies, first-run import.
  const data = useDataInfo();
  const dataProblem = data.info?.problem ?? data.error;
  const importOffer = useImportOffer(data.info !== null && !data.info.problem);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const updates = useUpdates();

  // A first run with nothing to import starts with a few sample phrases in a sample folder.
  useEffect(() => {
    if (!loaded || folders.length > 0 || importOffer.preview || !data.info || data.info.problem) return;
    invoke<boolean>("seed_samples")
      .then((added) => (added ? load() : null))
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded, folders.length, importOffer.preview, data.info]);

  /** The data folder changed or files were imported: start over from what is on disk. */
  const dataChanged = async () => {
    await data.refresh();
    await load();
    setEditing(null);
    setDirty(false);
    setSelected(null);
    setScope(ALL);
  };

  // A cloud folder can change while the window is in the background: look again on return,
  // unless an edit is open (saving checks the file hash, so nothing is overwritten either way).
  const editingRef = useRef(false);
  editingRef.current = editing !== null;
  useEffect(() => {
    const onFocus = () => {
      void data.refresh();
      if (!editingRef.current) void load();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.refresh]);

  const infos = useFolderInfo(folders);
  // App folders with no phrases are hidden (they come back with their first phrase), except the
  // one being viewed, and in Advanced mode, which shows every file.
  const appExes = useMemo(
    () =>
      folders
        .filter((f) => isAppFolder(infos[f.file]))
        .flatMap((f) => (infos[f.file]?.scope.apps ?? []).flatMap((a) => a.exe_names)),
    [folders, infos],
  );
  const appIcons = useAppIcons(appExes);
  // The Apps list has one entry per program, not per folder. Entries with no phrases are hidden
  // (they come back with their first phrase), except the one being viewed and, in Advanced mode,
  // all of them.
  // Every folder limited to programs feeds it, including folders the user made themselves.
  const categories = useMemo(
    () => appCategories(folders.filter((f) => isAppFolder(infos[f.file])), infos),
    [folders, infos],
  );
  const phrasesIn = (files: string[]) =>
    folders.filter((f) => files.includes(f.file)).reduce((n, f) => n + f.phrases.length, 0);
  const shownCategories = categories.filter((c) => advanced || phrasesIn(c.files) > 0 || c.key === scope);
  const scopeCategory = categories.find((c) => c.key === scope);
  // What the list shows: everything, one folder, the phrases in no folder, or one app.
  const inScope = (r: Row) =>
    scope === ALL ||
    (isFolderScope(scope)
      ? sameFolder(r.label, folderOfScope(scope))
      : scope === NO_FOLDER
        ? r.label === null
        : !!scopeCategory?.files.includes(r.folder));

  // After "Works in" moved a phrase: what happened, with a way to put it back.
  const [moved, setMoved] = useState<{ outcome: MoveOutcome; name: string } | null>(null);
  const [undoBusy, setUndoBusy] = useState(false);
  const [undoError, setUndoError] = useState<string | null>(null);
  const phraseMoved = async (outcome: MoveOutcome, name: string) => {
    await load();
    setEditing(null);
    setScope(ALL);
    setSelected(`${outcome.to_file}#${outcome.to_index}`);
    setUndoError(null);
    setMoved({ outcome, name });
  };
  const undoMove = async () => {
    if (!moved) return;
    setUndoBusy(true);
    setUndoError(null);
    try {
      const out = await invoke<MoveOutcome>("set_phrase_where", {
        file: moved.outcome.to_file,
        index: moved.outcome.to_index,
        place: { kind: "folder", file: moved.outcome.from_file },
        expectedHash: folders.find((f) => f.file === moved.outcome.to_file)?.hash ?? "",
      });
      await load();
      setScope(ALL);
      setSelected(`${out.to_file}#${out.to_index}`);
      setMoved(null);
    } catch (e) {
      setUndoError(String(e).replace(/^file-changed:\s*/, ""));
    } finally {
      setUndoBusy(false);
    }
  };

  useEffect(() => {
    if (!advanced) return;
    invoke<Conflict[]>("list_conflicts")
      .then(setConflicts)
      .catch(() => setConflicts([]));
    invoke<string>("config_path")
      .then(setConfigPath)
      .catch(() => setConfigPath(null));
  }, [advanced, folders]);

  /** A folder came back from a save: swap it in and select the phrase at `index`. */
  const saved = (folder: FolderData, index: number | null) => {
    setFolders((fs) => fs.map((f) => (f.file === folder.file ? normalizeFolder(folder) : f)));
    setSelected(index === null ? null : `${folder.file}#${index}`);
    setEditing(null);
  };

  // The Apps group in the folder list can be folded away; the choice is remembered.
  const [appsOpen, setAppsOpen] = useState(() => {
    try {
      return localStorage.getItem("xpando.appsOpen") !== "0";
    } catch {
      return true;
    }
  });
  const changeAppsOpen = (open: boolean) => {
    setAppsOpen(open);
    try {
      localStorage.setItem("xpando.appsOpen", open ? "1" : "0");
    } catch {
      /* the choice just isn't remembered */
    }
  };

  // Widths of the folder list and the phrase list, kept between runs.
  const [sideWidth, setSideWidth] = useStoredWidth("xpando.sideWidth", 220, 160, 420);
  const [listWidth, setListWidth] = useStoredWidth("xpando.listWidth", 380, 240, 700);

  const rows = useMemo<Row[]>(
    () =>
      folders.flatMap((f) =>
        f.phrases.map((phrase) => ({
          // Address a phrase by its entry index, not its list position: they differ once an
          // unparseable entry has been skipped.
          id: `${f.file}#${phrase.index}`,
          folder: f.file,
          folderName: f.name,
          label: effectiveFolder(phrase.options.folder, isAutoAppFolder(f.name, infos[f.file]) ? null : f.name),
          phrase,
        })),
      ),
    [folders, infos],
  );

  // Folders are labels on phrases. The ones kept while empty are in a small list on disk.
  const [kept, setKept] = useState<string[]>([]);
  const loadKept = useCallback(() => {
    invoke<string[]>("folder_names_get")
      .then(setKept)
      .catch(() => setKept([]));
  }, []);
  useEffect(loadKept, [loadKept]);
  const folderNameList = useMemo(() => allFolderNames(rows.map((r) => r.label), kept), [rows, kept]);
  const countIn = (name: string) => rows.filter((r) => sameFolder(r.label, name)).length;
  const unfiled = rows.filter((r) => r.label === null).length;
  const scopeLabel = isFolderScope(scope) ? folderOfScope(scope) : null;

  const itemsIn = (name: string) =>
    rows.filter((r) => sameFolder(r.label, name)).map((r) => ({ file: r.folder, index: r.phrase.index }));
  const createFolder = async (name: string) => {
    const next = [...kept.filter((k) => !sameFolder(k, name)), name];
    await invoke("folder_names_set", { names: next });
    setKept(next);
    setEditing(null);
    setSelected(null);
    setScope(folderScope(name));
  };
  const renameFolder = async (from: string, to: string) => {
    const items = itemsIn(from);
    if (items.length > 0) await fileInFolder(items, to);
    const next = [...kept.filter((k) => !sameFolder(k, from) && !sameFolder(k, to)), to];
    await invoke("folder_names_set", { names: next });
    setKept(next);
    await load();
    setEditing(null);
    setSelected(null);
    setScope(folderScope(to));
  };
  const deleteFolder = async (name: string) => {
    const items = itemsIn(name);
    if (items.length > 0) await fileInFolder(items, "");
    const next = kept.filter((k) => !sameFolder(k, name));
    await invoke("folder_names_set", { names: next });
    setKept(next);
    await load();
    setEditing(null);
    setSelected(null);
    setScope(ALL);
  };
  /** What to write on a phrase before it moves to another file, so its folder does not change. */
  const pinFor = (r: Row | undefined): string | undefined =>
    r && (r.phrase.options.folder ?? null) === null ? (r.label ?? "") : undefined;
  /** Deletes one phrase of a trigger clash (the panel asks first), then looks at the files again. */
  const deleteConflicting = async (file: string, index: number) => {
    const folder = folders.find((f) => f.file === file);
    if (!folder) throw new Error("That file is no longer there.");
    await invoke("delete_phrase", { file, index, expectedHash: folder.hash });
    await load();
    setEditing(null);
    setSelected(null);
  };
  const moveToFolder = async (row: Row, name: string) => {
    await fileInFolder([{ file: row.folder, index: row.phrase.index }], name);
    await load();
  };
  const terms = useMemo(() => parseQuery(query, advanced), [query, advanced]);
  const [sortBy, setSortBy] = useState<"name" | "file">(() => {
    try {
      return localStorage.getItem("xpando.phraseSort") === "file" ? "file" : "name";
    } catch {
      return "name";
    }
  });
  const changeSort = (v: "name" | "file") => {
    setSortBy(v);
    try {
      localStorage.setItem("xpando.phraseSort", v);
    } catch {
      /* the choice just isn't remembered */
    }
  };
  const visible = useMemo(() => {
    const found = rows.filter((r) => inScope(r) && matches(r, terms));
    if (sortBy === "file") return found;
    const key = (r: Row) => title(r.phrase);
    return [...found].sort((a, b) =>
      key(a).localeCompare(key(b), undefined, { sensitivity: "base", numeric: true }),
    );
  }, [rows, scope, terms, sortBy]);
  const jumpTo = (file: string, index: number) => {
    const row = rows.find((r) => r.folder === file && r.phrase.index === index);
    if (!row) return;
    guard(() => {
      setEditing(null);
      setScope(ALL);
      setQuery("");
      setSelected(row.id);
    });
  };
  const changeScope = (next: string) => {
    if (next !== scope) guard(() => {
      setEditing(null);
      setScope(next);
    });
  };
  // Typing in the search box is guarded only when the new text would hide the phrase being edited.
  const changeQuery = (next: string) => {
    if (editing && "id" in editing) {
      const t = parseQuery(next, advanced);
      const stillShown = rows.some(
        (r) => r.id === editing.id && inScope(r) && matches(r, t),
      );
      if (!stillShown) {
        guard(() => {
          setEditing(null);
          setQuery(next);
        });
        return;
      }
    }
    setQuery(next);
  };
  const current = visible.find((r) => r.id === selected) ?? null;
  const scopeName =
    scope === ALL
      ? "All phrases"
      : scope === NO_FOLDER
        ? "No folder"
        : (scopeLabel ?? scopeCategory?.name ?? "");
  // New phrases are stored in an all-apps file and filed in the folder being viewed.
  const newFile = (allAppsFolders(folders, infos)[0] ?? folders[0])?.file ?? "";
  // With no match file at all (a fresh start), the first new phrase creates match/base.yml.
  const withNewFile = async (open: (file: string) => void) => {
    if (newFile) {
      open(newFile);
      return;
    }
    try {
      const out = await invoke<{ file: string | null }>("create_folder", { name: "base" });
      const fresh = await load();
      const file = out.file ?? fresh?.[0]?.file;
      if (file) open(file);
    } catch (e) {
      setError(String(e));
    }
  };
  const editorPane =
    editing && "newIn" in editing ? (
      <PhraseEditor
        key={`new:${editing.newIn}:${editing.n}`}
        folders={folders}
        file={editing.newIn}
        folderNames={folderNameList}
        initialFolder={editing.folder ?? null}
        newWorksIn={(place, setPlace) => (
          <NewPhraseWorksIn value={place} onChange={setPlace} advanced={advanced} />
        )}
        onNotice={setError}
        initialPlace={editing.place}
        phrase={null}
        start={editing.start}
        onSaved={saved}
        onCancel={() => guard(closeEditor)}
        onReload={load}
        onDirtyChange={setDirty}
      />
    ) : editing ? (
      <PhraseEditor
        key={editing.id}
        folders={folders}
        file={editing.file}
        phrase={editing.base}
        folderNames={folderNameList}
        fileDefaultFolder={
          isAutoAppFolder(folders.find((f) => f.file === editing.file)?.name ?? "", infos[editing.file])
            ? null
            : (folders.find((f) => f.file === editing.file)?.name ?? null)
        }
        onSaved={saved}
        onCancel={() => guard(closeEditor)}
        onReload={load}
        onDirtyChange={setDirty}
        worksIn={(isDirty) => (
          <WorksIn
            compact
            blocked={isDirty ? "Save or cancel your changes first." : undefined}
            folder={{
              file: editing.file,
              name: folders.find((f) => f.file === editing.file)?.name ?? editing.file,
              hash: folders.find((f) => f.file === editing.file)?.hash ?? "",
            }}
            phrase={{ index: editing.base.index, triggers: editing.base.triggers, name: title(editing.base) }}
            infos={infos}
            folders={folders}
            advanced={advanced}
            onMoved={(o, n) => void phraseMoved(o, n)}
            onReload={() => void load()}
            pinFolder={pinFor(rows.find((r) => r.id === editing.id))}
          />
        )}
      />
    ) : null;
  const warnings = folders.flatMap((f) => f.warnings.map((w) => `${f.file}: ${w}`));

  const categoryItem = (c: AppCategory) => (
    <li key={c.key}>
      <button
        type="button"
        className="folder"
        aria-current={scope === c.key}
        title={c.name}
        onClick={() => changeScope(c.key)}
      >
        <AppIcon exes={c.exes} icons={appIcons} />
        <span className="grow folder-name">
          <span className="folder-label">{c.name}</span>
          {advanced && <span className="adv-folder-file">{c.files.join(", ")}</span>}
        </span>
        <span className="count">{phrasesIn(c.files)}</span>
      </button>
    </li>
  );

  const labelItem = (name: string, count: number) => (
    <li key={name}>
      <button
        type="button"
        className="folder"
        aria-current={scope === folderScope(name)}
        title={name}
        onClick={() => changeScope(folderScope(name))}
      >
        <FolderIcon size={16} aria-hidden="true" />
        <span className="grow folder-name">
          <span className="folder-label">{name}</span>
        </span>
        <span className="count">{count}</span>
      </button>
    </li>
  );

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
            onChange={(e) => changeQuery(e.target.value)}
            onFocus={() => setSearchFocused(true)}
            onBlur={() => setSearchFocused(false)}
            aria-describedby={advanced ? "search-hint" : undefined}
          />
        </div>
        <AdvancedSwitch on={advanced} onChange={setAdvanced} />
        <SettingsButton onClick={() => setSettingsOpen(true)} />
      </header>
      {advanced && (searchFocused || !query) && <SearchHint />}

      <DataBanners
        info={data.info}
        error={data.error}
        onRetry={() => void dataChanged()}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <UpdateBanner u={updates} />
      {error && !dataProblem && (
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

      <div className="panes" style={{ "--side-w": `${sideWidth}px`, "--list-w": `${listWidth}px` } as CSSProperties}>
        <PaneGrip label="Folder list width" width={sideWidth} min={160} max={420} fallback={220} start={0} onChange={setSideWidth} />
        <PaneGrip
          label="Phrase list width"
          width={listWidth}
          min={240}
          max={700}
          fallback={380}
          start={sideWidth + 8}
          onChange={setListWidth}
        />
        <nav className="pane sidebar" aria-label="Folders">
          <ul>
            <li>
              <button
                type="button"
                className="folder"
                aria-current={scope === ALL}
                onClick={() => changeScope(ALL)}
              >
                <Library size={16} aria-hidden="true" />
                <span className="grow">All phrases</span>
                <span className="count">{rows.length}</span>
              </button>
            </li>
            {folderNameList.map((n) => labelItem(n, countIn(n)))}
            {unfiled > 0 && folderNameList.length > 0 && (
              <li>
                <button
                  type="button"
                  className="folder"
                  aria-current={scope === NO_FOLDER}
                  onClick={() => changeScope(NO_FOLDER)}
                >
                  <FolderIcon size={16} aria-hidden="true" />
                  <span className="grow folder-name">
                    <span className="folder-label">No folder</span>
                  </span>
                  <span className="count">{unfiled}</span>
                </button>
              </li>
            )}
          </ul>
          {/* Folder actions reload everything and close the editor, so they wait for unsaved edits. */}
          <div
            inert={folderActionsLocked}
            className={folderActionsLocked ? "side-new folder-actions-locked" : "side-new"}
          >
            <NewFolderLabelButton names={folderNameList} onCreate={createFolder} />
          </div>
          {shownCategories.length > 0 && (
            <section className="side-apps">
              <button
                type="button"
                className="side-heading"
                aria-expanded={appsOpen || shownCategories.some((c) => c.key === scope)}
                aria-controls="side-apps-list"
                onClick={() => changeAppsOpen(!appsOpen)}
              >
                <ChevronDown size={16} aria-hidden="true" className="side-chev" />
                <span className="grow">Apps</span>
                <span className="count">{shownCategories.length}</span>
              </button>
              {(appsOpen || shownCategories.some((c) => c.key === scope)) && (
                <ul id="side-apps-list" className="side-apps-list">
                  {shownCategories.map((c) => categoryItem(c))}
                </ul>
              )}
            </section>
          )}
          <div className="side-bottom">
            {advanced && <ConflictsPanel conflicts={conflicts} onJump={jumpTo} onDelete={deleteConflicting} />}
          </div>
        </nav>

        <section className="pane list" aria-label={`${scopeName} phrases`}>
          <div className="pane-head">
            <h2 className="pane-title">
              {scopeName} <span className="count">{visible.length}</span>
            </h2>
            {scopeLabel && (
              <div
                inert={folderActionsLocked}
                className={folderActionsLocked ? "folder-actions-locked" : undefined}
                title={folderActionsLocked ? "Save or cancel your edit to change this folder" : undefined}
              >
                <FolderLabelMenu
                  key={scopeLabel}
                  name={scopeLabel}
                  count={countIn(scopeLabel)}
                  names={folderNameList}
                  onRename={(to) => renameFolder(scopeLabel, to)}
                  onDelete={() => deleteFolder(scopeLabel)}
                />
              </div>
            )}
          </div>
          {query && (
            <div className="search-active" role="status">
              <Search size={14} aria-hidden="true" />
              <span className="grow">
                Filtered by “{query}”
                {visible.length === 0 ? "" : ` · ${visible.length} shown`}
              </span>
              <button type="button" onClick={() => changeQuery("")} aria-label="Clear search">
                Clear
              </button>
            </div>
          )}
          <label className="list-sort">
            Sort
            <select value={sortBy} onChange={(e) => changeSort(e.target.value as "name" | "file")}>
              <option value="name">Name, A to Z</option>
              <option value="file">Order in the file</option>
            </select>
          </label>
          <NewPhraseButton
            disabled={!loaded}
            onClick={() =>
              guard(() =>
                void withNewFile((file) =>
                  setEditing({ newIn: file, n: Date.now(), place: newPlaceFor(scopeCategory), folder: scopeLabel }),
                ),
              )
            }
            onFromClipboard={(start) =>
              guard(() =>
                void withNewFile((file) =>
                  setEditing({ newIn: file, n: Date.now(), start, place: newPlaceFor(scopeCategory), folder: scopeLabel }),
                ),
              )
            }
          />
          {loaded && visible.length === 0 && (
            <p className="empty">
              {query
                ? (
                  <>
                    {scope === ALL ? "No phrases match" : `No phrases in ${scopeName} match`} “{query}”.{" "}
                    {scope !== ALL && rows.some((r) => matches(r, terms)) && (
                      <button type="button" className="link" onClick={() => changeScope(ALL)}>
                        Search all phrases
                      </button>
                    )}{" "}
                    <button type="button" className="link" onClick={() => changeQuery("")}>
                      Clear search
                    </button>
                  </>
                )
                : folders.length === 0
                  ? "No phrases yet. Use New phrase to start, or open Settings to bring in your espanso phrases."
                  : "No phrases yet."}
            </p>
          )}
          <ul>
            {visible.map((r) => (
              <li key={r.id}>
                <button
                  type="button"
                  className="phrase"
                  aria-current={r.id === selected}
                  onClick={() =>
                    guard(() => {
                      setSelected(r.id);
                      setEditing(null);
                    })
                  }
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
          {editorPane ?? (current ? (
            <>
              <PhraseActions
                folder={folders.find((f) => f.file === current.folder)!}
                phrase={current.phrase}
                onEdit={() => setEditing({ id: current.id, file: current.folder, base: current.phrase })}
                onDeleted={(f) => saved(f, null)}
                onReload={load}
              />
              <h2 className="detail-title">{title(current.phrase)}</h2>
              <label className="detail-folder">
                Folder
                <select
                  value={current.label ?? ""}
                  onChange={(e) => void moveToFolder(current, e.target.value)}
                >
                  <option value="">No folder</option>
                  {folderNameList.map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
              {advanced && <FileMeta file={current.folder} line={current.phrase.line} />}
              {current.phrase.comment && current.phrase.comment !== title(current.phrase) && (
                <p className="detail-comment">{current.phrase.comment}</p>
              )}
              <EngineWarning
                engine={engine}
                usesSetClipboard={current.phrase.vars.some((v) => v.kind === "set_clipboard")}
              />
              {advanced && (
                <ClipboardHackOffer<FolderData>
                  file={current.folder}
                  index={current.phrase.index}
                  hash={folders.find((f) => f.file === current.folder)?.hash ?? ""}
                  onConverted={(f) => saved(normalizeFolder(f), current.phrase.index)}
                />
              )}
              {advanced && <DetailTabs tab={tab} onChange={setTab} />}
              {advanced && tab === "yaml" ? (
                <div role="tabpanel" id="detail-tabpanel" aria-labelledby="detail-tab-yaml">
                  <YamlPane
                    file={current.folder}
                    index={current.phrase.index}
                    version={folders.find((f) => f.file === current.folder)?.hash ?? ""}
                    onSaved={(f) => saved(f as FolderData, current.phrase.index)}
                    onReload={load}
                  />
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
              {advanced && <OptionChips options={current.phrase.options} />}
              {(advanced || (current.phrase.kind !== "replace" && current.phrase.kind !== "html")) && (
                <p className="detail-meta">{KIND_LABEL[current.phrase.kind] ?? current.phrase.kind}</p>
              )}
              {current.phrase.kind === "html" ? (
                <RichTextView html={current.phrase.content} />
              ) : (
                <pre className="content-text">{withPlaceholders(current.phrase.content)}</pre>
              )}
              {current.phrase.vars.length > 0 && !advanced && (
                <>
                  <h3 className="detail-meta">Macros</h3>
                  <ul className="dm-chips">
                    {current.phrase.vars.map((v) => (
                      <li key={v.name}>
                        <span className="placeholder">{`{{${v.name}}}`}</span>
                        <span className="dm-kind">{macroLabel(v.kind)}</span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {current.phrase.vars.length > 0 && advanced && (
                <>
                  <h3 className="detail-meta">
                    Macros
                    <VarsAka />
                  </h3>
                  <ul className="dm-list">
                    {current.phrase.vars.map((v) => (
                      <li className="dm" key={v.name}>
                        <div className="dm-head">
                          <span className="placeholder">{`{{${v.name}}}`}</span>
                          <span className="dm-kind">
                            {macroLabel(v.kind)} <code>{v.kind || "unknown type"}</code>
                          </span>
                        </div>
                        {v.params.length > 0 && (
                          <dl className="dm-params">
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
              <WorksIn
                folder={{
                  file: current.folder,
                  name: current.folderName,
                  hash: folders.find((f) => f.file === current.folder)?.hash ?? "",
                }}
                phrase={{ index: current.phrase.index, triggers: current.phrase.triggers, name: title(current.phrase) }}
                infos={infos}
                folders={folders}
                advanced={advanced}
                onMoved={(o, n) => void phraseMoved(o, n)}
                onReload={() => void load()}
                pinFolder={pinFor(current)}
              />
            </>
          ) : (
            <p className="empty">Select a phrase to see its details.</p>
          ))}
        </section>
      </div>
      {importOffer.preview && (
        <ImportDialog
          preview={importOffer.preview}
          onClose={importOffer.close}
          onImported={() => void dataChanged()}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          info={data.info}
          updates={updates}
          onClose={() => setSettingsOpen(false)}
          onChanged={() => void dataChanged()}
          onOffer={importOffer.open}
        />
      )}
      {pendingLeave && (
        <UnsavedChangesDialog
          onKeep={() => setPendingLeave(null)}
          onDiscard={() => {
            const action = pendingLeave;
            setPendingLeave(null);
            setDirty(false);
            action();
          }}
        />
      )}
      {moved && (
        <MovedNotice
          outcome={moved.outcome}
          phraseName={moved.name}
          busy={undoBusy}
          error={undoError}
          onUndo={() => void undoMove()}
          onClose={() => setMoved(null)}
        />
      )}
      {advanced && <StatusBar configPath={configPath} phrases={rows.length} files={folders.length} engine={engine} />}
    </div>
  );
}
