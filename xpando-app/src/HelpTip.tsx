import { useId, useState } from "react";
import type { ReactNode } from "react";
import { CircleHelp } from "lucide-react";

/**
 * A small "?" button that shows a longer explanation when asked. Keeps a screen calm: the
 * label says what a thing does in a few words, and the detail is one click away.
 * Put it inside a flex-wrap row; the text takes a full line below the row's content.
 */
export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <>
      <button
        type="button"
        className="help-btn"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={`More about: ${label}`}
        title="More about this"
        onClick={() => setOpen((o) => !o)}
      >
        <CircleHelp size={16} aria-hidden="true" />
      </button>
      {open && (
        <span className="help-text" id={id} role="note">
          {children}
        </span>
      )}
    </>
  );
}
