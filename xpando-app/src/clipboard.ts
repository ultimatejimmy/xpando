/** Reads the system clipboard for the "Paste from clipboard" buttons. */
import { fromClipboard } from "./pasteClean";
import type { ClipboardContent } from "./pasteClean";

export type ClipboardRead = { ok: true; content: ClipboardContent } | { ok: false; message: string };

const FALLBACK = "xpando could not read the clipboard. Click in the text and press Ctrl+V instead.";

/**
 * Reads text/html (and text/plain) with the async Clipboard API and cleans it. The reason a
 * read fails is not shown to the user; they get one plain message and a way forward.
 */
export async function readClipboard(): Promise<ClipboardRead> {
  const nav = typeof navigator === "undefined" ? undefined : navigator;
  try {
    let html: string | null = null;
    let text: string | null = null;
    if (nav?.clipboard?.read) {
      for (const item of await nav.clipboard.read()) {
        if (html === null && item.types.includes("text/html")) html = await (await item.getType("text/html")).text();
        if (text === null && item.types.includes("text/plain")) text = await (await item.getType("text/plain")).text();
      }
    } else if (nav?.clipboard?.readText) {
      text = await nav.clipboard.readText();
    } else {
      return { ok: false, message: FALLBACK };
    }
    const content = fromClipboard(html, text);
    if (content.kind === "empty") return { ok: false, message: "The clipboard has no text to paste." };
    return { ok: true, content };
  } catch {
    return { ok: false, message: FALLBACK };
  }
}
