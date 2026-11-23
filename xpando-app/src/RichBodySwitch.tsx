import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Info } from "lucide-react";
import { listWords } from "./richtextUtil";
import "./richtext.css";

interface HtmlConversion {
  html: string;
  lost: string[];
}

interface PlainConversion {
  text: string;
  lost: string[];
}


export type BodyMode = "plain" | "rich";

interface SwitchProps {
  mode: BodyMode;
  /** The current body: plain text in plain mode, HTML in rich mode. */
  body: string;
  /** Called with the new mode and the body converted for it. */
  onSwitch: (mode: BodyMode, body: string) => void;
}

/**
 * Plain text / Rich text switch. Going to rich text keeps the text and adds nothing; going
 * back asks first when formatting would be lost. Switching back to where the user started,
 * with no edits since, restores the original body exactly.
 */
export function BodyModeSwitch({ mode, body, onSwitch }: SwitchProps) {
  const [pending, setPending] = useState<PlainConversion | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const stash = useRef<{ from: BodyMode; body: string; converted: string } | null>(null);

  function finish(to: BodyMode, converted: string, from: BodyMode, original: string) {
    stash.current = { from, body: original, converted };
    onSwitch(to, converted);
  }

  async function toRich() {
    setError(null);
    setPending(null);
    const s = stash.current;
    if (s && s.from === "rich" && body === s.converted) {
      stash.current = null;
      setNote(null);
      onSwitch("rich", s.body);
      return;
    }
    try {
      const c = await invoke<HtmlConversion>("plain_to_html", { text: body });
      setNote(c.lost.length > 0 ? `Changed while converting: ${listWords(c.lost)}.` : null);
      finish("rich", c.html, "plain", body);
    } catch (e) {
      setError(String(e));
    }
  }

  async function toPlain(confirmed: boolean) {
    setError(null);
    const s = stash.current;
    if (s && s.from === "plain" && body === s.converted) {
      stash.current = null;
      setNote(null);
      onSwitch("plain", s.body);
      return;
    }
    try {
      const c = await invoke<PlainConversion>("html_to_plain", { html: body });
      if (c.lost.length > 0 && !confirmed) {
        setPending(c);
        return;
      }
      setPending(null);
      setNote(null);
      finish("plain", c.text, "rich", body);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="rt-switch-wrap">
      <div className="rt-switch" role="group" aria-label="Text format">
        <button type="button" className="rt-seg" aria-pressed={mode === "plain"} onClick={() => mode !== "plain" && void toPlain(false)}>
          Plain text
        </button>
        <button type="button" className="rt-seg" aria-pressed={mode === "rich"} onClick={() => mode !== "rich" && void toRich()}>
          Rich text
        </button>
      </div>
      {pending && (
        <div className="rt-confirm" role="alert">
          <AlertTriangle size={16} aria-hidden="true" />
          <span className="grow">Plain text can't hold {listWords(pending.lost)}. Switching removes it.</span>
          <button type="button" className="btn" onClick={() => void toPlain(true)}>
            Switch to plain text
          </button>
          <button type="button" className="btn" onClick={() => setPending(null)}>
            Keep rich text
          </button>
        </div>
      )}
      {note && mode === "rich" && (
        <p className="rt-hint" role="status">
          <Info size={16} aria-hidden="true" />
          {note}
        </p>
      )}
      {error && (
        <p className="rt-error" role="alert">
          <AlertTriangle size={16} aria-hidden="true" />
          {error}
        </p>
      )}
    </div>
  );
}
