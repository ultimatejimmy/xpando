# Rich Text

A phrase can be **plain text** or **rich text** (bold, links, lists). Above the text box, the **Plain text / Rich text** switch chooses which.

![The rich text editor](images/rich-text.png)

## The toolbar

A text style drop-down, then **Bold**, **Italic**, **Underline**, **Strikethrough**, **Text colour**, **Highlight**, four alignment buttons, **Link**, **Remove link**, **Image**, **Bulleted list**, **Numbered list**, **Quote**, **Horizontal line**, **Undo**, **Redo**, **Clear formatting** and **Paste from clipboard**.

Advanced mode adds **Subscript**, **Superscript**, **Font**, **Font size**, **Decrease indent**, **Increase indent**, **Line spacing** and **Table** (insert a table, add or delete rows and columns, merge and split cells).

## Links and images

* **Link** asks for a **Link address**. Allowed addresses start with `http://`, `https://`, `mailto:` or `tel:`.
* **Image** asks for an **Image address (https)**. Pictures are linked, not stored in the phrase, so they need to stay online. Some programs never show linked pictures. A small picture that comes with something you paste (up to about 300 KB) is stored inside the phrase.

## What is kept and what is not

xpando keeps the formatting people use in documents: bold, italic, underline, strikethrough, subscript and superscript, text colour, highlight, font and size, links, headings (three levels), alignment, indents and line spacing, lists with sub-lists, tables (borders, shading, widths, merged cells), quotes, code blocks, horizontal lines and pictures. Anything else is cleaned out, so scripts and unknown formatting cannot end up in your files.

## Pasting from Word

You can copy formatted text from Word (and Outlook, Excel, Google Docs or a web page) and use it as the text of a phrase.

* **Ctrl+V** in the rich text editor pastes it with its formatting, cleaned up.
* **Paste from clipboard** (the clipboard button in the toolbar) does the same from a button. If your computer does not allow xpando to read the clipboard, it says so; press **Ctrl+V** instead.
* **From clipboard** next to **New phrase** starts a new phrase with what you copied. If you copied formatted text it opens in rich text; if you copied plain text it opens in plain text. The trigger is left empty for you to fill in.

What happens to Word's formatting:

| In Word | In the phrase |
|---|---|
| Bold, italic, underline, strikethrough, subscript, superscript | Kept |
| Text colour, highlight, font, font size | Kept |
| Headings, alignment (left, centre, right, justified), indents, line and paragraph spacing | Kept |
| Bulleted and numbered lists, including sub-levels | Kept as real lists |
| Links | Kept (`http`, `https`, `mailto`, `tel`; others become plain text) |
| Tables with borders, shading, widths, merged cells | Kept |
| Pictures on the web, or small pictures stored in the document | Kept |
| Pictures on your computer or in an email (`file:`, `cid:`) | Left out, with a note under the editor |
| Comments, hidden text, Word's style sheets | Left out |
| Text such as `<wr:forEach select="/Report/Survey">` | Kept exactly as text |

Some programs show pasted formatting differently (see "How it is pasted"), and fonts need to be installed where the phrase is used.

## Switching between plain and rich

* Plain to rich keeps your text; blank lines become paragraphs and single line breaks stay line breaks.
* Rich to plain loses formatting. xpando tells you what would be lost ("Plain text can't hold ... Switching removes it.") and you choose **Switch to plain text** or **Keep rich text**.

## How it is pasted

Rich text is always pasted through the clipboard, and your previous clipboard is put back afterwards. Programs that cannot take formatting get a readable plain version instead (paragraphs, bullet lists as `*`, link addresses dropped).

Programs differ in how they show rich text (Outlook, Word, Gmail, Slack, Teams). Test your phrase where you use it. If you find a problem, [report it](https://github.com/ultimatejimmy/xpando/issues).

## Macros in rich text

[[Macros]] work in rich text, with two cautions:

* **Cursor position** is not offered in rich text.
* A macro's value is inserted as is. If the clipboard contains something that looks like HTML (`<b>`), it is pasted as formatting. Prefer plain text when you insert clipboard content you do not control.
