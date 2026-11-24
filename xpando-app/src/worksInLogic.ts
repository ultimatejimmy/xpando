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

/** Splits folders into the ordinary ones and the app-specific ones, keeping their order. */
export function splitFolders<T extends { file: string }>(
  folders: readonly T[],
  infos: Readonly<Record<string, ScopeFolder | undefined>>,
): { general: T[]; apps: T[] } {
  const general: T[] = [];
  const apps: T[] = [];
  for (const f of folders) (isAppFolder(infos[f.file]) ? apps : general).push(f);
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
