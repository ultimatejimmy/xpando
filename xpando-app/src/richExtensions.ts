/**
 * Small TipTap extensions for the formatting rich text keeps beyond StarterKit: text colour,
 * highlight, font and size (one `textStyle` mark), subscript and superscript, paragraph
 * alignment / indent / spacing, and tables. Written here instead of installing the official
 * extension packages. Values are checked with `cssSubset.ts` (the twin of the Rust allow-list),
 * so the editor never holds a style the sanitiser would drop.
 *
 * The saved HTML uses inline `style` attributes, because that is what survives pasting into
 * Outlook, Word and Gmail.
 */
import { Extension, Mark, Node as TipNode } from "@tiptap/react";
import type { Editor } from "@tiptap/react";
import { DOMSerializer } from "@tiptap/pm/model";
import { goToNextCell, tableEditing } from "@tiptap/pm/tables";
import {
  blockStyleCss,
  blockStyleFrom,
  boxStyleCss,
  boxStyleFrom,
  cssAlign,
  parseDecls,
  textStyleCss,
  textStyleFrom,
} from "./cssSubset";
import type { BlockStyle } from "./cssSubset";

// ---------------------------------------------------------------------------------------
// Keeping styles exactly as written
// ---------------------------------------------------------------------------------------
//
// ProseMirror writes a `style` attribute through the browser's CSS object model, which
// rewrites it (`border: 1pt solid #000; border-top: none` becomes `border-width: medium 1pt
// 1pt; border-style: none solid solid; ...`, `#c00000` becomes `rgb(192, 0, 0)`). So every
// styled element also carries the exact text in `data-style`; `getBodyHtml` puts that back as
// the `style` and removes the helper, and the parse rules read `data-style` first.

/** The style an element was written with: `data-style` if present, else `style`. */
const rawStyle = (el: HTMLElement) => el.getAttribute("data-style") ?? el.getAttribute("style");

const styleAttrs = (css: string): Record<string, string> => (css ? { style: css, "data-style": css } : {});

/** The editor's content as HTML, with styles exactly as stored (what is sent to be saved). */
export function getBodyHtml(editor: Editor): string {
  const frag = DOMSerializer.fromSchema(editor.schema).serializeFragment(editor.state.doc.content);
  const container = document.implementation.createHTMLDocument().createElement("div");
  container.appendChild(frag);
  for (const el of Array.from(container.querySelectorAll("[data-style]"))) {
    el.setAttribute("style", el.getAttribute("data-style") ?? "");
    el.removeAttribute("data-style");
  }
  return container.innerHTML;
}

// ---------------------------------------------------------------------------------------
// Character formatting
// ---------------------------------------------------------------------------------------

export interface TextFormat {
  color: string | null;
  background: string | null;
  fontFamily: string | null;
  fontSize: string | null;
}

/** `<span style="color; background-color; font-family; font-size">`. */
export const TextFormatMark = Mark.create({
  name: "textStyle",
  addAttributes() {
    return {
      color: { default: null, rendered: false },
      background: { default: null, rendered: false },
      fontFamily: { default: null, rendered: false },
      fontSize: { default: null, rendered: false },
    };
  },
  parseHTML() {
    return [
      {
        tag: "span",
        getAttrs: (el) => {
          const t = textStyleFrom(parseDecls(rawStyle(el as HTMLElement)));
          if (!t.color && !t.background && !t.family && !t.size) return false;
          return { color: t.color, background: t.background, fontFamily: t.family, fontSize: t.size };
        },
      },
    ];
  },
  renderHTML({ mark }) {
    const a = mark.attrs as TextFormat;
    const css = textStyleCss({ color: a.color, background: a.background, family: a.fontFamily, size: a.fontSize });
    return ["span", styleAttrs(css), 0];
  },
});

export const SubscriptMark = Mark.create({
  name: "subscript",
  excludes: "superscript",
  parseHTML: () => [{ tag: "sub" }],
  renderHTML: () => ["sub", 0],
});

export const SuperscriptMark = Mark.create({
  name: "superscript",
  excludes: "subscript",
  parseHTML: () => [{ tag: "sup" }],
  renderHTML: () => ["sup", 0],
});

/** Changes some parts of the text style of the selection (null removes a part). */
export function setTextFormat(editor: Editor, patch: Partial<TextFormat>): void {
  const now = editor.getAttributes("textStyle") as Partial<TextFormat>;
  const next: TextFormat = {
    color: now.color ?? null,
    background: now.background ?? null,
    fontFamily: now.fontFamily ?? null,
    fontSize: now.fontSize ?? null,
    ...patch,
  };
  const chain = editor.chain().focus();
  if (!next.color && !next.background && !next.fontFamily && !next.fontSize) chain.unsetMark("textStyle").run();
  else chain.setMark("textStyle", next).run();
}

// ---------------------------------------------------------------------------------------
// Paragraph formatting
// ---------------------------------------------------------------------------------------

/** Alignment, indent and spacing of paragraphs and headings, kept as one `style` string. */
export const BlockFormat = Extension.create({
  name: "blockFormat",
  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading"],
        attributes: {
          blockStyle: {
            default: null,
            parseHTML: (el: HTMLElement) => {
              const decls = parseDecls(rawStyle(el));
              const legacy = el.getAttribute("align");
              if (legacy && cssAlign(legacy) && !decls.some(([n]) => n === "text-align")) {
                decls.push(["text-align", legacy]);
              }
              return blockStyleCss(blockStyleFrom(decls)) || null;
            },
            renderHTML: (attrs: Record<string, unknown>) =>
              styleAttrs(typeof attrs.blockStyle === "string" ? attrs.blockStyle : ""),
          },
        },
      },
    ];
  },
});

/** Changes some parts of the paragraph style of every paragraph and heading in the selection. */
export function patchBlockStyle(editor: Editor, patch: Partial<BlockStyle>): void {
  editor
    .chain()
    .focus()
    .command(({ tr, state }) => {
      const { from, to } = state.selection;
      state.doc.nodesBetween(from, to, (node, pos) => {
        if (node.type.name !== "paragraph" && node.type.name !== "heading") return;
        const cur = blockStyleFrom(parseDecls(node.attrs.blockStyle as string | null));
        const css = blockStyleCss({ ...cur, ...patch });
        tr.setNodeMarkup(pos, undefined, { ...node.attrs, blockStyle: css || null });
      });
      return true;
    })
    .run();
}

/** The paragraph style at the cursor. */
export function currentBlockStyle(editor: Editor): BlockStyle {
  const attrs = editor.isActive("heading") ? editor.getAttributes("heading") : editor.getAttributes("paragraph");
  return blockStyleFrom(parseDecls((attrs.blockStyle as string | null) ?? ""));
}

/** One indent step is 36pt (half an inch), as in Word. */
export function indentBy(editor: Editor, steps: 1 | -1): void {
  const cur = currentBlockStyle(editor).marginLeft;
  const m = cur ? /^(\d+(?:\.\d+)?)pt$/.exec(cur) : null;
  const now = m ? parseFloat(m[1]) : cur ? 0 : 0;
  const next = Math.max(0, Math.min(720, now + 36 * steps));
  patchBlockStyle(editor, { marginLeft: next === 0 ? null : `${next}pt` });
}

// ---------------------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------------------

const span = (v: string | null) => {
  const n = parseInt(v ?? "", 10);
  return Number.isFinite(n) && n >= 1 && n <= 100 ? n : 1;
};

const boxAttr = {
  boxStyle: {
    default: null,
    parseHTML: (el: HTMLElement) => boxStyleCss(boxStyleFrom(parseDecls(rawStyle(el)))) || null,
    renderHTML: (attrs: Record<string, unknown>) => styleAttrs(typeof attrs.boxStyle === "string" ? attrs.boxStyle : ""),
  },
};

/** ProseMirror's table plugin needs a `tableRole` on each node of the schema; pass it on. */
export const TableSchema = Extension.create({
  name: "tableSchema",
  extendNodeSchema(extension) {
    const role = (extension.config as { tableRole?: string }).tableRole;
    return role ? { tableRole: role } : {};
  },
});

export const TableNode = TipNode.create({
  name: "table",
  group: "block",
  content: "tableRow+",
  isolating: true,
  ...({ tableRole: "table" } as object),
  addAttributes() {
    return { ...boxAttr };
  },
  parseHTML: () => [{ tag: "table" }],
  renderHTML: ({ HTMLAttributes }) => ["table", HTMLAttributes, ["tbody", 0]],
  addProseMirrorPlugins: () => [tableEditing()],
  addKeyboardShortcuts() {
    return {
      Tab: () => this.editor.commands.command(({ state, dispatch }) => goToNextCell(1)(state, dispatch)),
      "Shift-Tab": () => this.editor.commands.command(({ state, dispatch }) => goToNextCell(-1)(state, dispatch)),
    };
  },
});

export const TableRowNode = TipNode.create({
  name: "tableRow",
  content: "(tableCell | tableHeader)*",
  ...({ tableRole: "row" } as object),
  parseHTML: () => [{ tag: "tr" }],
  renderHTML: () => ["tr", 0],
});

function cellAttrs() {
  return {
    colspan: {
      default: 1,
      parseHTML: (el: HTMLElement) => span(el.getAttribute("colspan")),
      renderHTML: (a: Record<string, unknown>) => ((a.colspan as number) > 1 ? { colspan: a.colspan } : {}),
    },
    rowspan: {
      default: 1,
      parseHTML: (el: HTMLElement) => span(el.getAttribute("rowspan")),
      renderHTML: (a: Record<string, unknown>) => ((a.rowspan as number) > 1 ? { rowspan: a.rowspan } : {}),
    },
    colwidth: { default: null, rendered: false },
    ...boxAttr,
  };
}

export const TableCellNode = TipNode.create({
  name: "tableCell",
  content: "block+",
  isolating: true,
  ...({ tableRole: "cell" } as object),
  addAttributes: cellAttrs,
  parseHTML: () => [{ tag: "td" }],
  renderHTML: ({ HTMLAttributes }) => ["td", HTMLAttributes, 0],
});

export const TableHeaderNode = TipNode.create({
  name: "tableHeader",
  content: "block+",
  isolating: true,
  ...({ tableRole: "header_cell" } as object),
  addAttributes: cellAttrs,
  parseHTML: () => [{ tag: "th" }],
  renderHTML: ({ HTMLAttributes }) => ["th", HTMLAttributes, 0],
});

/** Inserts an empty table of `rows` by `cols` cells at the cursor, with a thin border. */
export function insertTable(editor: Editor, rows: number, cols: number): void {
  const border = "border:1px solid #000000";
  const cell = { type: "tableCell", attrs: { boxStyle: border }, content: [{ type: "paragraph" }] };
  const row = { type: "tableRow", content: Array.from({ length: cols }, () => cell) };
  editor
    .chain()
    .focus()
    .insertContent({
      type: "table",
      attrs: { boxStyle: "border-collapse:collapse" },
      content: Array.from({ length: rows }, () => row),
    })
    .run();
}

export const richFormatExtensions = [
  TextFormatMark,
  SubscriptMark,
  SuperscriptMark,
  BlockFormat,
  TableSchema,
  TableNode,
  TableRowNode,
  TableCellNode,
  TableHeaderNode,
];
