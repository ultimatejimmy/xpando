// Folders are labels on phrases. They have nothing to do with which apps a phrase works in
// (that decides which file the phrase is stored in). A phrase's label is `xpando_folder:` in its
// entry. Without one, a phrase is in the folder named after its file, unless that file is one
// xpando made for a program (Word, Firefox...): then it is in no folder.

/** The `scope` value of the list of phrases that are in no folder. */
export const NO_FOLDER = "none:";
/** Prefix of the `scope` value of a folder. */
const FOLDER_PREFIX = "f:";

export const folderScope = (name: string): string => `${FOLDER_PREFIX}${name}`;
export const isFolderScope = (scope: string): boolean => scope.startsWith(FOLDER_PREFIX);
export const folderOfScope = (scope: string): string => scope.slice(FOLDER_PREFIX.length);

/** Folder names are compared without regard to capitals. */
export const sameFolder = (a: string | null, b: string | null): boolean =>
  a !== null && b !== null && a.toLowerCase() === b.toLowerCase();

/**
 * The folder a phrase is in: its own label when it has one (an empty label means "no folder"),
 * else the one its file stands for (`null` for an app file).
 */
export function effectiveFolder(own: string | null | undefined, fileDefault: string | null): string | null {
  if (own === null || own === undefined) return fileDefault;
  return own === "" ? null : own;
}

/** Every folder to list: the ones phrases are in and the ones kept empty, A to Z, one per name. */
export function allFolderNames(inUse: readonly (string | null)[], kept: readonly string[]): string[] {
  const out: string[] = [];
  for (const n of [...inUse, ...kept]) {
    if (n && !out.some((o) => sameFolder(o, n))) out.push(n);
  }
  return out.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base", numeric: true }));
}
