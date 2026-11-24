// The app picker's data and pure logic: the curated list of common programs, program-name
// clean-up, de-duplication and the rows and chips the picker shows. No React, no Tauri, so
// `node --test --experimental-strip-types src/knownApps.test.ts` can run it.

/** A common program with a friendly name. `exes` are the file names it runs as (Windows). */
export interface KnownApp {
  name: string;
  exes: string[];
}

/**
 * Common Windows programs. This is the only place the list lives: add a program here and it
 * shows up in the picker. Keep names short and the way people say them. A program with two
 * generations or builds lists every file name (Teams: new and classic). File names are the
 * names Windows shows in Task Manager, Details tab.
 */
export const KNOWN_APPS: readonly KnownApp[] = [
  { name: "Word", exes: ["winword.exe"] },
  { name: "Excel", exes: ["excel.exe"] },
  { name: "PowerPoint", exes: ["powerpnt.exe"] },
  { name: "Outlook", exes: ["outlook.exe"] },
  { name: "Outlook (new)", exes: ["olk.exe"] },
  { name: "OneNote", exes: ["onenote.exe"] },
  { name: "Access", exes: ["msaccess.exe"] },
  { name: "Chrome", exes: ["chrome.exe"] },
  { name: "Edge", exes: ["msedge.exe"] },
  { name: "Firefox", exes: ["firefox.exe"] },
  { name: "Brave", exes: ["brave.exe"] },
  { name: "Opera", exes: ["opera.exe"] },
  { name: "Vivaldi", exes: ["vivaldi.exe"] },
  { name: "Teams", exes: ["ms-teams.exe", "teams.exe"] },
  { name: "Slack", exes: ["slack.exe"] },
  { name: "Discord", exes: ["discord.exe"] },
  { name: "Zoom", exes: ["zoom.exe"] },
  { name: "WhatsApp", exes: ["whatsapp.exe"] },
  { name: "Telegram", exes: ["telegram.exe"] },
  { name: "Thunderbird", exes: ["thunderbird.exe"] },
  { name: "Notepad", exes: ["notepad.exe"] },
  { name: "Notepad++", exes: ["notepad++.exe"] },
  { name: "WordPad", exes: ["wordpad.exe"] },
  { name: "Visual Studio Code", exes: ["code.exe"] },
  { name: "Visual Studio", exes: ["devenv.exe"] },
  { name: "Sublime Text", exes: ["sublime_text.exe"] },
  { name: "Obsidian", exes: ["obsidian.exe"] },
  { name: "Notion", exes: ["notion.exe"] },
  { name: "Windows Terminal", exes: ["windowsterminal.exe"] },
  { name: "PowerShell", exes: ["powershell.exe", "pwsh.exe"] },
  { name: "Command Prompt", exes: ["cmd.exe"] },
  { name: "File Explorer", exes: ["explorer.exe"] },
  { name: "Paint", exes: ["mspaint.exe"] },
  { name: "Adobe Acrobat", exes: ["acrobat.exe"] },
  { name: "Adobe Acrobat Reader", exes: ["acrord32.exe", "acrord64.exe"] },
  { name: "Claude", exes: ["claude.exe"] },
];

/** A program reported by the app (running now, or installed). */
export interface Program {
  exe: string;
  name: string;
}

const MAX_EXE_CHARS = 64;

/** Compare program names with this: trimmed, lower case, without `.exe` (as the Rust side does). */
export function exeKey(raw: string): string {
  const t = raw.trim();
  return (t.toLowerCase().endsWith(".exe") ? t.slice(0, -4) : t).trim().toLowerCase();
}

/** The characters a folder filter accepts in a program name (same rule as the Rust side). */
function nameOk(file: string): boolean {
  const stem = exeKey(file);
  if (!stem || [...file].length > MAX_EXE_CHARS) return false;
  return [...file].every((c) => /[\p{L}\p{N}]/u.test(c) || " _-.+()".includes(c));
}

/**
 * Cleans something the user typed or pasted into a program file name: drops quotes and a leading
 * folder path. Returns `null` when what is left cannot be used in a folder filter.
 */
export function normalizeExe(raw: string): string | null {
  const unquoted = raw.trim().replace(/^["']+|["']+$/g, "");
  const file = unquoted.split(/[\\/]/).pop()?.trim() ?? "";
  return nameOk(file) ? file : null;
}

/** Adds `more` to `list`, skipping names already there (ignoring case and `.exe`). */
export function addExes(list: readonly string[], more: readonly string[]): string[] {
  const out = [...list];
  const seen = new Set(out.map(exeKey));
  for (const e of more) {
    const k = exeKey(e);
    if (k && !seen.has(k)) {
      seen.add(k);
      out.push(e);
    }
  }
  return out;
}

/** Removes every name of `gone` from `list` (ignoring case and `.exe`). */
export function removeExes(list: readonly string[], gone: readonly string[]): string[] {
  const drop = new Set(gone.map(exeKey));
  return list.filter((e) => !drop.has(exeKey(e)));
}

/** Parses typed text (comma, semicolon or line separated) into clean, distinct names. */
export function parseTypedExes(text: string): { exes: string[]; rejected: string[] } {
  const exes: string[] = [];
  const rejected: string[] = [];
  for (const part of text.split(/[,;\n]/)) {
    if (!part.trim()) continue;
    const n = normalizeExe(part);
    if (n) exes.push(n);
    else rejected.push(part.trim());
  }
  return { exes: addExes([], exes), rejected };
}

const knownByKey: ReadonlyMap<string, KnownApp> = new Map(
  KNOWN_APPS.flatMap((a) => a.exes.map((e) => [exeKey(e), a] as const)),
);

/** The curated entry a program name belongs to, if any. */
export function knownFor(exe: string): KnownApp | undefined {
  return knownByKey.get(exeKey(exe));
}

/** One selected thing shown as a removable chip. */
export interface Chip {
  label: string;
  /** The program names the chip stands for; removing the chip removes all of them. */
  exes: string[];
}

/**
 * The chips for the selected program names: a curated program shows as its friendly name (with
 * every selected file name of it), anything else as the name the app reported, else the file
 * name without `.exe`.
 */
export function chipsFor(selected: readonly string[], seen: readonly Program[] = []): Chip[] {
  const names = new Map(seen.map((p) => [exeKey(p.exe), p.name] as const));
  const done = new Set<string>();
  const chips: Chip[] = [];
  for (const e of selected) {
    const k = exeKey(e);
    if (done.has(k)) continue;
    const known = knownByKey.get(k);
    if (known) {
      const keys = new Set(known.exes.map(exeKey));
      const members = selected.filter((s) => keys.has(exeKey(s)));
      members.forEach((m) => done.add(exeKey(m)));
      chips.push({ label: known.name, exes: members });
    } else {
      done.add(k);
      const friendly = names.get(k);
      chips.push({ label: friendly && friendly !== exeKey(friendly) ? friendly : e.replace(/\.exe$/i, ""), exes: [e] });
    }
  }
  return chips;
}

/** One row of the picker's list. */
export interface Row {
  id: string;
  name: string;
  exes: string[];
}

/** Rows for the curated list. */
export function knownRows(): Row[] {
  return KNOWN_APPS.map((a) => ({ id: `known:${a.name}`, name: a.name, exes: [...a.exes] }));
}

/**
 * Rows for programs the app reported. Programs that are in the curated list show the curated
 * name; the rest keep their reported name. Sorted by name.
 */
export function programRows(programs: readonly Program[]): Row[] {
  const rows = new Map<string, Row>();
  for (const p of programs) {
    const k = exeKey(p.exe);
    if (!k || rows.has(k)) continue;
    const known = knownByKey.get(k);
    rows.set(k, { id: `exe:${k}`, name: known ? known.name : p.name || k, exes: [p.exe] });
  }
  return [...rows.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

/** Rows whose name or file name contains every word of `query` (ignoring case). */
export function filterRows(rows: readonly Row[], query: string): Row[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...rows];
  return rows.filter((r) => {
    const hay = `${r.name} ${r.exes.join(" ")}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** True when every file name of the row is selected. */
export function rowSelected(row: Row, selected: readonly string[]): boolean {
  const have = new Set(selected.map(exeKey));
  return row.exes.every((e) => have.has(exeKey(e)));
}

/** Selects the row, or removes it when it is already selected. */
export function toggleRow(selected: readonly string[], row: Row): string[] {
  return rowSelected(row, selected) ? removeExes(selected, row.exes) : addExes(selected, row.exes);
}

/** True when the user agent says Windows (the curated list is made of Windows file names). */
export function isWindows(userAgent: string): boolean {
  return /Windows/i.test(userAgent);
}
