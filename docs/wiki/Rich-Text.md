# Rich Text

A phrase can be **plain text** or **rich text** (bold, links, lists). Above the text box, the **Plain text / Rich text** switch chooses which.

![The rich text editor](images/rich-text.png)

## The toolbar

A text style drop-down, then **Bold**, **Italic**, **Underline**, **Strikethrough**, **Link**, **Remove link**, **Image**, **Bulleted list**, **Numbered list**, **Quote**, **Horizontal line**, **Undo**, **Redo** and **Clear formatting**.

## Links and images

* **Link** asks for a **Link address**. Allowed addresses start with `http://`, `https://`, `mailto:` or `tel:`.
* **Image** asks for an **Image address (https)**. Pictures are linked, not stored in the phrase, so they need to stay online. Some programs never show linked pictures.

## What is kept and what is not

xpando keeps simple, widely understood formatting: paragraphs, bold, italic, underline, strikethrough, code, links, lists, headings (three levels), quotes, code blocks, horizontal lines and linked images. Colours, fonts and sizes are **not** kept. Anything else is cleaned out, so scripts and unknown formatting cannot end up in your files.

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
