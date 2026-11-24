/**
 * The rich text pieces the rest of the app uses. The editor itself (TipTap) is large, so it
 * is loaded on first use instead of with the app; phrases without rich text never pay for it.
 */
import { lazy, Suspense } from "react";
import type { Ref } from "react";
import type { RichHandle } from "./richtextUtil";

const Editor = lazy(() => import("./RichTextEditor").then((m) => ({ default: m.RichBodyEditor })));
const View = lazy(() => import("./RichTextEditor").then((m) => ({ default: m.RichTextView })));

/** WYSIWYG editor for an `html:` body; `onChange` fires only when the user edits. */
export function RichBodyEditor(props: {
  html: string;
  onChange: (html: string) => void;
  labelledBy?: string;
  handleRef?: Ref<RichHandle>;
}) {
  return (
    <Suspense fallback={<p className="rt-hint">Loading the editor…</p>}>
      <Editor {...props} />
    </Suspense>
  );
}

/** Read-only rendering of an `html:` body. */
export function RichTextView(props: { html: string }) {
  return (
    <Suspense fallback={null}>
      <View {...props} />
    </Suspense>
  );
}

export { BodyModeSwitch } from "./RichBodySwitch";
export type { BodyMode } from "./RichBodySwitch";
