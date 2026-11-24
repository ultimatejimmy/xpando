// "Works in" for a phrase: which apps a phrase works in, worked out from its folder, and the
// small memory of where a phrase came from. Pure logic (no React, no Tauri) so it can be tested
// with `node --test --experimental-strip-types src/worksInLogic.test.ts`.

import { exeKey, knownFor, type Program } from "./knownApps.ts";

/** The parts of the backend's `FolderInfo` this logic reads (the full type is in FolderManager). */
export interface ScopeApp {
  exe_names: string[];
  exec_simple: boolean;
  title_pattern: string | null;
  class_pattern: string | null;
  os: string | null;
}

export interface ScopeFolder {
  file: string;
  name: string;
  scope: { kind: "all" | "only" | "except" | "nowhere"; apps: ScopeApp[] };
}

/**
 * Where the phrases of a folder work, in the words of the "Works in" setting.
 * `apps` is a plain list of programs (what xpando can set). `custom` is limited by something
 * xpando does not manage here (a window title, a class, a hand-written pattern, an operating
 * system), `except` is "all apps except some", `nowhere` means no config loads the folder.
 */
export type WorksIn =
  | { kind: "all" }
  | { kind: "apps"; exes: string[] }
  | { kind: "custom" }
  | { kind: "except" }
  | { kind: "nowhere" };

/** Mirrors the Rust `simple_app_set`: a plain list of program names and nothing else. */
export function worksIn(folder: ScopeFolder | undefined): WorksIn {
  if (!folder) return { kind: "all" };
  const { kind, apps } = folder.scope;
  if (kind === "all") return { kind: "all" };
  if (kind === "except") return { kind: "except" };
  if (kind === "nowhere") return { kind: "nowhere" };
  const plain = apps.length > 0 && apps.every((a) => a.exec_simple && a.exe_names.length > 0 && !a.title_pattern && !a.class_pattern && !a.os);
  if (!plain) return { kind: "custom" };
  const seen = new Set<string>();
  const exes: string[] = [];
  for (const e of apps.flatMap((a) => a.exe_names)) {
    const k = exeKey(e);
    if (!seen.has(k)) {
      seen.add(k);
      exes.push(e);
    }
  }
  return { kind: "apps", exes };
}

function friendly(exe: string, seen: readonly Program[]): string {
  const known = knownFor(exe);
  if (known) return known.name;
  const k = exeKey(exe);
  const hit = seen.find((p) => exeKey(p.exe) === k)?.name;
  return hit && hit !== k ? hit : exe.replace(/\.exe$/i, "");
}

/** Friendly names for a list of programs, one per program (two builds of one program once). */
export function friendlyNames(exes: readonly string[], seen: readonly Program[] = []): string[] {
  const out: string[] = [];
  for (const e of exes) {
    const n = friendly(e, seen);
    if (!out.some((x) => x.toLowerCase() === n.toLowerCase())) out.push(n);
  }
  return out;
}

function joinNames(names: string[]): string {
  if (names.length <= 3) return names.join(", ");
  return `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`;
}

/** The sentence-sized answer to "Works in": All apps, Only Word, Outlook, and so on. */
export function worksInLabel(w: WorksIn, seen: readonly Program[] = []): string {
  switch (w.kind) {
    case "all":
      return "All apps";
    case "apps":
      return `Only ${joinNames(friendlyNames(w.exes, seen))}`;
    case "custom":
      return "Only some apps (set on its folder)";
    case "except":
      return "All apps except some (set on its folder)";
    case "nowhere":
      return "Nowhere (its folder is not used)";
  }
}

/** Folders that only some apps load, which the folder list shows under "Apps". */
export function isAppFolder(folder: ScopeFolder | undefined): boolean {
  return folder?.scope.kind === "only";
}

const plain = (t: string) => t.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

/**
 * True for a folder xpando made (or would make) for some programs: it works only in them and is
 * named after them ("Word", "Word, Outlook"). A folder the user named and filled themselves stays
 * a folder, whatever apps it is limited to.
 */
export function isAutoAppFolder(name: string, folder: ScopeFolder | undefined): boolean {
  if (!folder || !isAppFolder(folder)) return false;
  const exes = [...new Set(folder.scope.apps.flatMap((a) => a.exe_names))];
  if (exes.length === 0) return false;
  const n = plain(name);
  const friendlyJoined = plain(friendlyNames(exes).slice(0, 3).join(", "));
  return (
    n === friendlyJoined ||
    exes.some((e) => n === plain(exeKey(e)) || n === plain(friendlyNames([e])[0])) ||
    n === plain(exes.map((e) => exeKey(e)).join(", "))
  );
}

/**
 * Splits folders into the ones listed as folders and the ones listed as apps (see
 * [`isAutoAppFolder`]), each sorted A to Z by name.
 */
export function splitFolders<T extends { file: string; name: string }>(
  folders: readonly T[],
  infos: Readonly<Record<string, ScopeFolder | undefined>>,
): { general: T[]; apps: T[] } {
  const general: T[] = [];
  const apps: T[] = [];
  for (const f of folders) (isAutoAppFolder(f.name, infos[f.file]) ? apps : general).push(f);
  // A to Z, ignoring capitals, so the list is in a predictable order.
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true });
  general.sort(byName);
  apps.sort(byName);
  return { general, apps };
}

/** Folders a phrase can go back to: the ones that work in all apps. */
export function allAppsFolders<T extends { file: string }>(
  folders: readonly T[],
  infos: Readonly<Record<string, ScopeFolder | undefined>>,
): T[] {
  return folders.filter((f) => infos[f.file]?.scope.kind === "all");
}

// ---- where a phrase came from ------------------------------------------------------------

/** The memory of where phrases were before they were moved to an app folder. */
export type Origins = Record<string, string>;

/** Key for a phrase in its new folder: the folder's file and its first trigger. */
export function originKey(file: string, firstTrigger: string): string {
  return `${file}\u0000${firstTrigger}`;
}

export function rememberOrigin(origins: Origins, toFile: string, firstTrigger: string, fromFile: string): Origins {
  return { ...origins, [originKey(toFile, firstTrigger)]: fromFile };
}

/** The all-apps folder to offer first when a phrase goes back: where it came from if that
 * still works in all apps, else `base.yml`, else the first all-apps folder. */
export function returnFolder(
  origins: Origins,
  file: string,
  firstTrigger: string,
  candidates: readonly { file: string }[],
): string | null {
  const from = origins[originKey(file, firstTrigger)];
  if (from && candidates.some((c) => c.file === from)) return from;
  return candidates.find((c) => c.file.toLowerCase() === "base.yml")?.file ?? candidates[0]?.file ?? null;
}

// ---- apps as categories -------------------------------------------------------------------

/**
 * One entry of the "Apps" list. Files are still limited per folder, but the list shows one entry
 * per program: a folder that works in Word and Excel is listed under both.
 */
export interface AppCategory {
  /** What the list selects by: `app:<program>`, or the folder's file for a folder with a
   * hand-written filter that has no program names. */
  key: string;
  name: string;
  /** The program file names this entry stands for (empty for a hand-written filter). */
  exes: string[];
  /** The folder files whose phrases belong under this entry. */
  files: string[];
}

export function appCategories<T extends { file: string; name: string }>(
  appFolders: readonly T[],
  infos: Readonly<Record<string, ScopeFolder | undefined>>,
): AppCategory[] {
  const byKey = new Map<string, AppCategory>();
  const sole = new Map<string, string>(); // program key -> name of a folder made for just that program
  for (const f of appFolders) {
    const exes = [...new Set((infos[f.file]?.scope.apps ?? []).flatMap((a) => a.exe_names))];
    if (exes.length === 0) {
      byKey.set(f.file, { key: f.file, name: f.name, exes: [], files: [f.file] });
      continue;
    }
    if (exes.length === 1 && !sole.has(exeKey(exes[0]))) sole.set(exeKey(exes[0]), f.name);
    for (const e of exes) {
      const key = `app:${exeKey(e)}`;
      const c = byKey.get(key) ?? { key, name: "", exes: [], files: [] };
      if (!c.exes.some((x) => exeKey(x) === exeKey(e))) c.exes.push(e);
      if (!c.files.includes(f.file)) c.files.push(f.file);
      byKey.set(key, c);
    }
  }
  for (const c of byKey.values()) {
    if (c.name === "") c.name = sole.get(exeKey(c.exes[0])) ?? friendlyNames([c.exes[0]])[0];
  }
  return [...byKey.values()].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true }),
  );
}
