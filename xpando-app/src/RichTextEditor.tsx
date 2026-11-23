import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EditorContent, Extension, useEditor, useEditorState } from "@tiptap/react";
import type { Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import {
  AlertTriangle,
  Bold,
  ImagePlus,
  Info,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  Minus,
  Quote,
  Redo2,
  RemoveFormatting,
  Strikethrough,
  Underline as UnderlineIcon,
  Undo2,
  Unlink,
} from "lucide-react";
import { findPlaceholders, listWords, normalizeImageUrl, normalizeLinkUrl } from "./richtextUtil";
import "./richtext.css";

/** What the backend sanitiser returns: the canonical HTML and what it had to drop. */
interface Sanitized {
  html: string;
  dropped: string[];
}

/** Highlights `{{placeholders}}` without changing the document, so they round-trip as text. */
const Placeholders = Extension.create({
  name: "placeholders",
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("xpandoPlaceholders"),
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];
            state.doc.descendants((node, pos) => {
              if (!node.isText || !node.text) return;
              for (const span of findPlaceholders(node.text)) {
                decorations.push(Decoration.inline(pos + span.from, pos + span.to, { class: "placeholder" }));
              }
            });
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});

/** The node and mark types the backend sanitiser keeps; anything else is not in the schema. */
function extensions() {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
      // The saved HTML has no rel/target; clicking a link never navigates inside the editor.
      link: { openOnClick: false, autolink: false, linkOnPaste: true, HTMLAttributes: { rel: null, target: null } },
      // espanso pastes the value as is, so no empty paragraph may be added after a block.
      trailingNode: false,
    }),
    Image.configure({ inline: true, allowBase64: false }),
    Placeholders,
  ];
}

// ---------------------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------------------

type Panel = "link" | "image" | null;

interface ToolbarButtonProps {
  label: string;
  icon: ReactNode;
  onClick: () => void;
  pressed?: boolean;
  disabled?: boolean;
}

function ToolbarButton({ label, icon, onClick, pressed, disabled }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      className="rt-btn"
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {icon}
    </button>
  );
}

function Separator() {
  return <span className="rt-sep" role="separator" aria-orientation="vertical" />;
}

function Toolbar({ editor }: { editor: Editor }) {
  const barRef = useRef<HTMLDivElement>(null);
  const [panel, setPanel] = useState<Panel>(null);
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      bold: e.isActive("bold"),
      italic: e.isActive("italic"),
      underline: e.isActive("underline"),
      strike: e.isActive("strike"),
      link: e.isActive("link"),
      bullet: e.isActive("bulletList"),
      ordered: e.isActive("orderedList"),
      quote: e.isActive("blockquote"),
      block: e.isActive("heading", { level: 1 })
        ? "1"
        : e.isActive("heading", { level: 2 })
          ? "2"
          : e.isActive("heading", { level: 3 })
            ? "3"
            : "p",
      canUndo: e.can().undo(),
      canRedo: e.can().redo(),
    }),
  });

  // Roving tab stop: one control is reachable with Tab, the arrow keys move between them.
  const controls = () =>
    Array.from(barRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled), select") ?? []);
  const syncTabStops = (active?: HTMLElement) => {
    const items = controls();
    const current = active && items.includes(active) ? active : (items.find((i) => i.tabIndex === 0) ?? items[0]);
    for (const item of items) item.tabIndex = item === current ? 0 : -1;
  };
  useEffect(() => {
    syncTabStops();
  });

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const target = e.target as HTMLElement;
    if (target.tagName === "SELECT" || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
    const items = controls();
    const at = items.indexOf(target);
    if (at < 0) return;
    const next =
      e.key === "Home"
        ? 0
        : e.key === "End"
          ? items.length - 1
          : (at + (e.key === "ArrowRight" ? 1 : -1) + items.length) % items.length;
    e.preventDefault();
    items[next].focus();
  }

  const run = () => editor.chain().focus();

  return (
    <>
      <div
        className="rt-toolbar"
        role="toolbar"
        aria-label="Text formatting"
        ref={barRef}
        onKeyDown={onKeyDown}
        onFocus={(e) => syncTabStops(e.target as HTMLElement)}
      >
        <select
          className="rt-select"
          aria-label="Text style"
          value={s.block}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "p") run().setParagraph().run();
            else run().setHeading({ level: Number(v) as 1 | 2 | 3 }).run();
          }}
        >
          <option value="p">Paragraph</option>
          <option value="1">Heading 1</option>
          <option value="2">Heading 2</option>
          <option value="3">Heading 3</option>
        </select>
        <Separator />
        <ToolbarButton
          label="Bold"
          icon={<Bold size={16} aria-hidden="true" />}
          pressed={s.bold}
          onClick={() => run().toggleBold().run()}
        />
        <ToolbarButton
          label="Italic"
          icon={<Italic size={16} aria-hidden="true" />}
          pressed={s.italic}
          onClick={() => run().toggleItalic().run()}
        />
        <ToolbarButton
          label="Underline"
          icon={<UnderlineIcon size={16} aria-hidden="true" />}
          pressed={s.underline}
          onClick={() => run().toggleUnderline().run()}
        />
        <ToolbarButton
          label="Strikethrough"
          icon={<Strikethrough size={16} aria-hidden="true" />}
          pressed={s.strike}
          onClick={() => run().toggleStrike().run()}
        />
        <Separator />
        <ToolbarButton
          label="Link"
          icon={<LinkIcon size={16} aria-hidden="true" />}
          pressed={s.link || panel === "link"}
          onClick={() => setPanel(panel === "link" ? null : "link")}
        />
        <ToolbarButton
          label="Remove link"
          icon={<Unlink size={16} aria-hidden="true" />}
          disabled={!s.link}
          onClick={() => run().extendMarkRange("link").unsetLink().run()}
        />
        <ToolbarButton
          label="Image"
          icon={<ImagePlus size={16} aria-hidden="true" />}
          pressed={panel === "image"}
          onClick={() => setPanel(panel === "image" ? null : "image")}
        />
        <Separator />
        <ToolbarButton
          label="Bulleted list"
          icon={<List size={16} aria-hidden="true" />}
          pressed={s.bullet}
          onClick={() => run().toggleBulletList().run()}
        />
        <ToolbarButton
          label="Numbered list"
          icon={<ListOrdered size={16} aria-hidden="true" />}
          pressed={s.ordered}
          onClick={() => run().toggleOrderedList().run()}
        />
        <ToolbarButton
          label="Quote"
          icon={<Quote size={16} aria-hidden="true" />}
          pressed={s.quote}
          onClick={() => run().toggleBlockquote().run()}
        />
        <ToolbarButton
          label="Horizontal line"
          icon={<Minus size={16} aria-hidden="true" />}
          onClick={() => run().setHorizontalRule().run()}
        />
        <Separator />
        <ToolbarButton
          label="Undo"
          icon={<Undo2 size={16} aria-hidden="true" />}
          disabled={!s.canUndo}
          onClick={() => run().undo().run()}
        />
        <ToolbarButton
          label="Redo"
          icon={<Redo2 size={16} aria-hidden="true" />}
          disabled={!s.canRedo}
          onClick={() => run().redo().run()}
        />
        <ToolbarButton
          label="Clear formatting"
          icon={<RemoveFormatting size={16} aria-hidden="true" />}
          onClick={() => run().unsetAllMarks().clearNodes().run()}
        />
      </div>
      {panel && <AddressPanel key={panel} kind={panel} editor={editor} onClose={() => setPanel(null)} />}
    </>
  );
}

/** Inline form that asks for a link or image address. Never submits the surrounding form. */
function AddressPanel({ kind, editor, onClose }: { kind: "link" | "image"; editor: Editor; onClose: () => void }) {
  const isLink = kind === "link";
  const uid = useId();
  const input = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState<string>(isLink ? ((editor.getAttributes("link").href as string) ?? "") : "");
  const [alt, setAlt] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  function apply() {
    const r = isLink ? normalizeLinkUrl(value) : normalizeImageUrl(value);
    if (!r.ok) {
      setError(r.error);
      input.current?.focus();
      return;
    }
    if (isLink) {
      const chain = editor.chain().focus();
      if (editor.state.selection.empty && !editor.isActive("link")) {
        chain
          .insertContent({ type: "text", text: r.url, marks: [{ type: "link", attrs: { href: r.url } }] })
          .run();
      } else {
        chain.extendMarkRange("link").setLink({ href: r.url }).run();
      }
    } else {
      editor.chain().focus().setImage({ src: r.url, alt: alt.trim() || undefined }).run();
    }
    onClose();
  }

  function onKeyDown(e: KeyboardEvent<HTMLElement>) {
    if (e.key === "Enter") {
      e.preventDefault(); // would otherwise save the whole phrase form
      apply();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation(); // would otherwise close the whole phrase editor
      onClose();
      editor.commands.focus();
    }
  }

  return (
    <div className="rt-panel" role="group" aria-label={isLink ? "Link address" : "Image address"} onKeyDown={onKeyDown}>
      <label htmlFor={`${uid}-url`}>{isLink ? "Link address" : "Image address (https)"}</label>
      <input
        id={`${uid}-url`}
        ref={input}
        className="field"
        value={value}
        placeholder={isLink ? "https://example.com" : "https://example.com/picture.png"}
        spellCheck={false}
        autoComplete="off"
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? `${uid}-err` : undefined}
        onChange={(e) => {
          setValue(e.target.value);
          setError(null);
        }}
      />
      {!isLink && (
        <>
          <label htmlFor={`${uid}-alt`}>Description for screen readers</label>
          <input
            id={`${uid}-alt`}
            className="field"
            value={alt}
            autoComplete="off"
            onChange={(e) => setAlt(e.target.value)}
          />
        </>
      )}
      <button type="button" className="btn btn-primary" onClick={apply}>
        {isLink ? "Apply" : "Insert"}
      </button>
      <button type="button" className="btn" onClick={onClose}>
        Cancel
      </button>
      {error && (
        <p className="rt-error" id={`${uid}-err`} role="alert">
          <AlertTriangle size={16} aria-hidden="true" />
          {error}
        </p>
      )}
      {!isLink && (
        <p className="rt-hint">
          <Info size={16} aria-hidden="true" />
          Pictures are linked, not stored in the phrase, so they need to stay online.
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// Editor and viewer
// ---------------------------------------------------------------------------------------

interface RichTextEditorProps {
  /** Canonical HTML to start from (already sanitised). Not watched after the first render. */
  initialHtml: string;
  /** Called with the editor's HTML after each change by the user, never on load. */
  onChange: (html: string) => void;
  /** Id of the element that names the editor, for assistive technology. */
  labelledBy?: string;
}

/** The WYSIWYG editor: toolbar plus editable content. */
export function RichTextEditor({ initialHtml, onChange, labelledBy }: RichTextEditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const editor = useEditor({
    extensions: extensions(),
    content: initialHtml,
    editorProps: {
      attributes: {
        class: "rt-content",
        role: "textbox",
        "aria-multiline": "true",
        ...(labelledBy ? { "aria-labelledby": labelledBy } : { "aria-label": "Text" }),
      },
    },
    onUpdate: ({ editor: e }) => onChangeRef.current(e.getHTML()),
  });
  if (!editor) return null;
  return (
    <div className="rt-editor">
      <Toolbar editor={editor} />
      <EditorContent editor={editor} />
    </div>
  );
}

/** Asks the backend for the canonical, safe form of `html`. */
function useSanitized(html: string): { result: Sanitized | null; failed: boolean } {
  const [state, setState] = useState<{ html: string; result: Sanitized | null; failed: boolean } | null>(null);
  useEffect(() => {
    let live = true;
    invoke<Sanitized>("sanitize_html", { html })
      .then((result) => live && setState({ html, result, failed: false }))
      .catch(() => live && setState({ html, result: null, failed: true }));
    return () => {
      live = false;
    };
  }, [html]);
  // Ignore an answer that belongs to an earlier body.
  return state && state.html === html ? state : { result: null, failed: false };
}

/**
 * Editor for an existing `html:` body. The body is sanitised by the backend first, so the
 * editor only ever sees the allowed subset. `onChange` fires only when the user edits, so
 * opening a phrase and saving without changes never rewrites it.
 */
export function RichBodyEditor({ html, onChange, labelledBy }: { html: string; onChange: (html: string) => void; labelledBy?: string }) {
  // The editor owns the content once it is running, so only the body it started with is checked.
  const [initial] = useState(html);
  const { result: start, failed } = useSanitized(initial);

  if (failed) {
    return (
      <p className="rt-error" role="alert">
        <AlertTriangle size={16} aria-hidden="true" />
        The rich text editor could not start because the text could not be checked.
      </p>
    );
  }
  if (!start) return <p className="rt-hint">Loading the editor…</p>;
  return (
    <>
      <RichTextEditor initialHtml={start.html} onChange={onChange} labelledBy={labelledBy} />
      {start.dropped.length > 0 && (
        <p className="rt-hint" role="status">
          <Info size={16} aria-hidden="true" />
          This phrase has formatting xpando can't keep ({listWords(start.dropped)}). It is removed if you edit and
          save the phrase.
        </p>
      )}
    </>
  );
}

/** Read-only rendering of an `html:` body through a non-editable editor. */
export function RichTextView({ html }: { html: string }) {
  const { result, failed } = useSanitized(html);
  const editor = useEditor({
    extensions: extensions(),
    content: "",
    editable: false,
    editorProps: { attributes: { class: "rt-content rt-readonly", role: "document", "aria-label": "Rich text" } },
  });
  useEffect(() => {
    if (editor && result) editor.commands.setContent(result.html, { emitUpdate: false });
  }, [editor, result]);
  if (failed) return <pre className="content-text">{html}</pre>;
  return editor ? <EditorContent editor={editor} /> : null;
}
