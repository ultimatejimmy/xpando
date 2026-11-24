# xpando

**xpando** is a text expander with a friendly app on top. You type a short **trigger** such as `;sig`, and xpando types out a whole **phrase** for you: your signature, an address, today's date, a formatted email reply.

Under the hood xpando is a fork of [espanso](https://espanso.org), so it works in almost any program on Windows, macOS and Linux, and everything stays in plain files on your computer.

![The xpando window](images/main-window.png)

## Start here

1. [[Install]] xpando.
2. Follow [[Your First Phrase]] to make one in two minutes.
3. Learn how to add [[Rich Text]] and [[Macros]] such as the date or the clipboard.

## Words used in this wiki

| Word | Meaning |
|---|---|
| **Phrase** | One piece of text that xpando types for you when you type its trigger. |
| **Trigger** | The short text you type to get the phrase, for example `;sig`. A phrase can have several. |
| **Text** | What the phrase puts in. Either plain text or rich text (bold, links, lists). |
| **Macro** | A dynamic piece inside the text that is filled in when the phrase expands, such as today's date, whatever is on the clipboard, or the output of a command. It shows in the text as `{{name}}`. |
| **Setting** | One field of a macro, for example the date format of a Date macro. |
| **Folder** | A group of phrases. Each folder is one file, and can be limited to certain apps. |
| **Advanced mode** | A switch in the header that shows the technical side: file names, line numbers, YAML and every option. |

## All pages

* [[Getting Started]] and [[Install]]
* [[Your First Phrase]]
* [[Rich Text]]
* [[Macros]]
* [[Folders and Apps]]
* [[Data Folder]], [[Import from espanso]], [[Sync Tips]]
* [[Advanced Mode]], [[Editing YAML]], [[Search Operators]]
* [[Set Clipboard]] (xpando's own engine only)
* [[Troubleshooting]] and [[FAQ]]
* [[Contributing]] and [[Releasing]]
