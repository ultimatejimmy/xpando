import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Icons for program file names (`firefox.exe`), as image addresses keyed by lower-case file name.
 * Programs whose icon cannot be found are left out, so the caller shows its generic icon.
 */
export function useAppIcons(exes: readonly string[]): Record<string, string> {
  const [icons, setIcons] = useState<Record<string, string>>({});
  // Only ask again when the set of programs changes.
  const key = useMemo(() => [...new Set(exes.map((e) => e.toLowerCase()))].sort().join("\n"), [exes]);
  useEffect(() => {
    if (key === "") return;
    let current = true;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const load = (triesLeft: number) => {
      invoke<Record<string, string>>("app_icons", { exes: key.split("\n") })
        .then((found) => {
          if (!current) return;
          const lower: Record<string, string> = {};
          for (const [exe, url] of Object.entries(found)) lower[exe.toLowerCase()] = url;
          setIcons((old) => ({ ...old, ...lower }));
        })
        .catch(() => {
          // The app may still be starting up: try again a couple of times, then keep the generic icon.
          if (current && triesLeft > 0) retry = setTimeout(() => load(triesLeft - 1), 3000);
        });
    };
    load(2);
    return () => {
      current = false;
      clearTimeout(retry);
    };
  }, [key]);
  return icons;
}
