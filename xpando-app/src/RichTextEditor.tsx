import { useEffect, useId, useImperativeHandle, useRef, useState } from "react";
import type { Ref } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EditorContent, Extension, useEditor, useEditorState } from "@tiptap/react";
import type { Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import {
  addColumnAfter,
  addRowAfter,
  deleteColumn,
  deleteRow,
  deleteTable,
  mergeCells,
  splitCell,
} from "@tiptap/pm/tables";
import {
  AlertTriangle,
  AlignCenter,
  AlignJustify,
  AlignLeft,
  AlignRight,
  Baseline,
  Bold,
  ClipboardPaste,
  Highlighter,
  ImagePlus,
  IndentDecrease,
  IndentIncrease,
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
  Subscript as SubscriptIcon,
  Superscript as SuperscriptIcon,
  Table as TableIcon,
  Underline as UnderlineIcon,
  Undo2,
  Unlink,
} from "lucide-react";
import { useAdvanced } from "./AdvancedPanels";
import { readClipboard } from "./clipboard";
import { cleanPastedHtml, isInternalPaste } from "./pasteClean";
import { cssFamily, cssFontSize } from "./cssSubset";
import {
  currentBlockStyle,
  getBodyHtml,
  indentBy,
  insertTable,
  patchBlockStyle,
  richFormatExtensions,
  setTextFormat,
} from "./richExtensions";
import { findPlaceholders, listWords, normalizeImageUrl, normalizeLinkUrl } from "./richtextUtil";
import type { RichHandle } from "./richtextUtil";
import "./richtext.css";

/** A line of feedback under the editor: what a paste could not keep, or why a paste failed. */
export interface Notice {
  error: boolean;
  lines: string[];
}

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
    // Small pictures may be stored inside the text (`data:`); the sanitiser limits their size.
    Image.configure({ inline: true, allowBase64: true }),
    ...richFormatExtensions,
    Placeholders,
  ];
}

// ---------------------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------------------

type Panel = "link" | "image" | "color" | "highlight" | "table" | null;

const FONT_SIZES = ["8", "9", "10", "11", "12", "14", "16", "18", "20", "24", "28", "36", "48", "72"];
const FONT_FAMILIES = ["Arial", "Calibri", "Cambria", "Courier New", "Georgia", "Tahoma", "Times New Roman", "Trebuchet MS", "Verdana"];
const LINE_SPACINGS: [string, string][] = [
  ["", "Line spacing"],
  ["1", "Single"],
  ["1.15", "1.15"],
  ["1.5", "1.5"],
  ["2", "Double"],
];
const SWATCHES: [string, string][] = [
  ["#000000", "Black"],
  ["#7f7f7f", "Grey"],
  ["#c00000", "Dark red"],
  ["#ff0000", "Red"],
  ["#ff9900", "Orange"],
  ["#ffff00", "Yellow"],
  ["#00b050", "Green"],
  ["#00b0f0", "Light blue"],
  ["#0070c0", "Blue"],
  ["#7030a0", "Purple"],
];

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

function Toolbar({ editor, onNotice }: { editor: Editor; onNotice: (n: Notice | null) => void }) {
  const barRef = useRef<HTMLDivElement>(null);
  const [panel, setPanel] = useState<Panel>(null);
  const [advanced] = useAdvanced();
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      align: currentBlockStyle(e).align ?? "left",
      lineHeight: currentBlockStyle(e).lineHeight ?? "",
      color: (e.getAttributes("textStyle").color as string | null) ?? null,
      background: (e.getAttributes("textStyle").background as string | null) ?? null,
      fontSize: ((e.getAttributes("textStyle").fontSize as string | null) ?? "").replace(/pt$/, ""),
      fontFamily: ((e.getAttributes("textStyle").fontFamily as string | null) ?? "").split(",")[0].replace(/'/g, ""),
      sub: e.isActive("subscript"),
      sup: e.isActive("superscript"),
      inTable: e.isActive("table"),
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

  async function pasteFromClipboard() {
    const r = await readClipboard();
    if (!r.ok) {
      onNotice({ error: true, lines: [r.message] });
      return;
    }
    onNotice(r.content.notes.length > 0 ? { error: false, lines: r.content.notes } : null);
    if (r.content.html !== "") editor.chain().focus().insertContent(r.content.html).run();
  }

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
        {advanced && (
          <>
            <ToolbarButton
              label="Subscript"
              icon={<SubscriptIcon size={16} aria-hidden="true" />}
              pressed={s.sub}
              onClick={() => run().toggleMark("subscript").run()}
            />
            <ToolbarButton
              label="Superscript"
              icon={<SuperscriptIcon size={16} aria-hidden="true" />}
              pressed={s.sup}
              onClick={() => run().toggleMark("superscript").run()}
            />
          </>
        )}
        <Separator />
        <ToolbarButton
          label="Text colour"
          icon={<Baseline size={16} aria-hidden="true" style={{ color: s.color ?? undefined }} />}
          pressed={panel === "color"}
          onClick={() => setPanel(panel === "color" ? null : "color")}
        />
        <ToolbarButton
          label="Highlight"
          icon={<Highlighter size={16} aria-hidden="true" style={{ color: s.background ?? undefined }} />}
          pressed={panel === "highlight"}
          onClick={() => setPanel(panel === "highlight" ? null : "highlight")}
        />
        {advanced && (
          <>
            <select
              className="rt-select"
              aria-label="Font"
              value={FONT_FAMILIES.includes(s.fontFamily) ? s.fontFamily : ""}
              onChange={(e) => {
                const v = e.target.value;
                setTextFormat(editor, { fontFamily: v ? cssFamily(v) : null });
              }}
            >
              <option value="">Font</option>
              {FONT_FAMILIES.map((f) => (
                <option key={f} value={f}>
                  {f}
                </option>
              ))}
            </select>
            <select
              className="rt-select"
              aria-label="Font size"
              value={FONT_SIZES.includes(s.fontSize) ? s.fontSize : ""}
              onChange={(e) => {
                const v = e.target.value;
                setTextFormat(editor, { fontSize: v ? cssFontSize(`${v}pt`) : null });
              }}
            >
              <option value="">Size</option>
              {FONT_SIZES.map((f) => (
                <option key={f} value={f}>
                  {f}
                </option>
              ))}
            </select>
          </>
        )}
        <Separator />
        <ToolbarButton
          label="Align left"
          icon={<AlignLeft size={16} aria-hidden="true" />}
          pressed={s.align === "left"}
          onClick={() => patchBlockStyle(editor, { align: null })}
        />
        <ToolbarButton
          label="Centre"
          icon={<AlignCenter size={16} aria-hidden="true" />}
          pressed={s.align === "center"}
          onClick={() => patchBlockStyle(editor, { align: "center" })}
        />
        <ToolbarButton
          label="Align right"
          icon={<AlignRight size={16} aria-hidden="true" />}
          pressed={s.align === "right"}
          onClick={() => patchBlockStyle(editor, { align: "right" })}
        />
        <ToolbarButton
          label="Justify"
          icon={<AlignJustify size={16} aria-hidden="true" />}
          pressed={s.align === "justify"}
          onClick={() => patchBlockStyle(editor, { align: "justify" })}
        />
        {advanced && (
          <>
            <ToolbarButton
              label="Decrease indent"
              icon={<IndentDecrease size={16} aria-hidden="true" />}
              onClick={() => indentBy(editor, -1)}
            />
            <ToolbarButton
              label="Increase indent"
              icon={<IndentIncrease size={16} aria-hidden="true" />}
              onClick={() => indentBy(editor, 1)}
            />
            <select
              className="rt-select"
              aria-label="Line spacing"
              value={LINE_SPACINGS.some(([v]) => v === s.lineHeight) ? s.lineHeight : ""}
              onChange={(e) => patchBlockStyle(editor, { lineHeight: e.target.value || null })}
            >
              {LINE_SPACINGS.map(([v, name]) => (
                <option key={v} value={v}>
                  {name}
                </option>
              ))}
            </select>
          </>
        )}
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
          onClick={() => {
            run().unsetAllMarks().clearNodes().run();
            patchBlockStyle(editor, { align: null, lineHeight: null, marginTop: null, marginBottom: null, marginLeft: null, textIndent: null });
          }}
        />
        {advanced && (
          <ToolbarButton
            label="Table"
            icon={<TableIcon size={16} aria-hidden="true" />}
            pressed={panel === "table" || s.inTable}
            onClick={() => setPanel(panel === "table" ? null : "table")}
          />
        )}
        <Separator />
        <button
          type="button"
          className="rt-btn rt-btn-text"
          title="Paste what you copied, keeping its formatting (or press Ctrl+V)"
          onClick={() => void pasteFromClipboard()}
        >
          <ClipboardPaste size={16} aria-hidden="true" />
          <span>Paste from clipboard</span>
        </button>
      </div>
      {(panel === "link" || panel === "image") && (
        <AddressPanel key={panel} kind={panel} editor={editor} onClose={() => setPanel(null)} />
      )}
      {(panel === "color" || panel === "highlight") && (
        <ColorPanel key={panel} kind={panel} editor={editor} onClose={() => setPanel(null)} />
      )}
      {panel === "table" && <TablePanel editor={editor} onClose={() => setPanel(null)} />}
    </>
  );
}

/** Row of colours for the text or its highlight, plus "None" and a free choice. */
function ColorPanel({ kind, editor, onClose }: { kind: "color" | "highlight"; editor: Editor; onClose: () => void }) {
  const isText = kind === "color";
  const uid = useId();
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => first.current?.focus(), []);
  function choose(value: string | null) {
    setTextFormat(editor, isText ? { color: value } : { background: value });
    onClose();
  }
  return (
    <div
      className="rt-panel"
      role="group"
      aria-label={isText ? "Text colour" : "Highlight colour"}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation(); // would otherwise close the whole phrase editor
          onClose();
          editor.commands.focus();
        }
      }}
    >
      {SWATCHES.map(([value, name], i) => (
        <button
          key={value}
          ref={i === 0 ? first : undefined}
          type="button"
          className="rt-swatch"
          style={{ background: value }}
          aria-label={name}
          title={name}
          onClick={() => choose(value)}
        />
      ))}
      <button type="button" className="btn" onClick={() => choose(null)}>
        {isText ? "Automatic" : "No highlight"}
      </button>
      <label htmlFor={`${uid}-other`}>Other colour</label>
      <input
        id={`${uid}-other`}
        type="color"
        className="rt-color-input"
        defaultValue={isText ? "#000000" : "#ffff00"}
        onChange={(e) => choose(e.target.value)}
      />
    </div>
  );
}

/** Table commands (Advanced mode). Inside a table they act on the cell under the cursor. */
function TablePanel({ editor, onClose }: { editor: Editor; onClose: () => void }) {
  const inTable = editor.isActive("table");
  const act = (cmd: typeof addRowAfter) => () => {
    editor.chain().focus().command(({ state, dispatch }) => cmd(state, dispatch)).run();
  };
  return (
    <div
      className="rt-panel"
      role="group"
      aria-label="Table"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
          editor.commands.focus();
        }
      }}
    >
      <button
        type="button"
        className="btn"
        disabled={inTable}
        onClick={() => {
          insertTable(editor, 3, 3);
          onClose();
        }}
      >
        Insert table (3 by 3)
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(addRowAfter)}>
        Add row
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(addColumnAfter)}>
        Add column
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(deleteRow)}>
        Delete row
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(deleteColumn)}>
        Delete column
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(mergeCells)}>
        Merge cells
      </button>
      <button type="button" className="btn" disabled={!inTable} onClick={act(splitCell)}>
        Split cell
      </button>
      <button type="button" className="btn btn-danger" disabled={!inTable} onClick={act(deleteTable)}>
        Delete table
      </button>
      <p className="rt-hint">
        <Info size={16} aria-hidden="true" />
        Select several cells by dragging, then choose Merge cells.
      </p>
    </div>
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
  /** Lets the Insert macro menu put text at the cursor. */
  handleRef?: Ref<RichHandle>;
}

/** The WYSIWYG editor: toolbar plus editable content. */
export function RichTextEditor({ initialHtml, onChange, labelledBy, handleRef }: RichTextEditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const [notice, setNotice] = useState<Notice | null>(null);
  const noticeRef = useRef(setNotice);
  noticeRef.current = setNotice;
  const editor = useEditor({
    extensions: extensions(),
    content: initialHtml,
    editorProps: {
      // Ctrl+V of anything copied outside xpando (Word, Excel, Outlook, a web page) is cleaned
      // first; copies made inside the editor are already clean.
      transformPastedHTML: (html) => {
        if (isInternalPaste(html)) return html;
        const r = cleanPastedHtml(html);
        noticeRef.current(r.notes.length > 0 ? { error: false, lines: r.notes } : null);
        return r.html;
      },
      attributes: {
        class: "rt-content",
        role: "textbox",
        "aria-multiline": "true",
        ...(labelledBy ? { "aria-labelledby": labelledBy } : { "aria-label": "Text" }),
      },
    },
    onUpdate: ({ editor: e }) => onChangeRef.current(getBodyHtml(e)),
  });
  useImperativeHandle(
    handleRef,
    () => ({
      insertText(text: string) {
        if (!editor || editor.isDestroyed) return;
        // A text node, not a string: `<` in the snippet must stay text. The selection the
        // editor had when the menu was opened is used, and the caret ends after the text.
        editor.chain().focus().insertContent({ type: "text", text }).run();
      },
    }),
    [editor],
  );
  if (!editor) return null;
  return (
    <>
      <div className="rt-editor">
        <Toolbar editor={editor} onNotice={setNotice} />
        <EditorContent editor={editor} />
      </div>
      {notice && (
        <div role={notice.error ? "alert" : "status"}>
          {notice.lines.map((line) => (
            <p key={line} className={notice.error ? "rt-error" : "rt-hint"}>
              {notice.error ? <AlertTriangle size={16} aria-hidden="true" /> : <Info size={16} aria-hidden="true" />}
              {line}
            </p>
          ))}
        </div>
      )}
    </>
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
export function RichBodyEditor({
  html,
  onChange,
  labelledBy,
  handleRef,
}: {
  html: string;
  onChange: (html: string) => void;
  labelledBy?: string;
  handleRef?: Ref<RichHandle>;
}) {
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
      <RichTextEditor initialHtml={start.html} onChange={onChange} labelledBy={labelledBy} handleRef={handleRef} />
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
