# Macros

A **macro** is a dynamic piece of a phrase that is filled in when the phrase expands: today's date, what you last copied, a name you pick from a list. In the text it appears as `{{name}}`.

![Insert macro menu](images/insert-macro.png)

## Adding a macro

1. While editing a phrase, press **Insert macro** above the text and choose one. xpando puts `{{name}}` in the text at the cursor and adds a card for the macro under **Macros**.
2. Fill in the macro's **settings** on its card.
3. Save.

If you remove a macro, xpando offers **Undo**.

## The macros

| Macro | What it fills in | Shown in |
|---|---|---|
| **Date** | Today's date or time. The *format* setting controls the look; it starts as `%Y-%m-%d`. | normal mode |
| **Clipboard** | What you copied last. | normal mode |
| **Random pick** | One of several options, at random. | normal mode |
| **Choice list** | You pick one when the phrase expands. | normal mode |
| **Cursor position** | Where the cursor lands after the text is typed. It is a marker in plain text, not a macro card. Not available in rich text. | normal mode |
| **Command output** | The output of a command. | Advanced mode |
| **Fixed text** | Text to reuse inside other macros. | Advanced mode |
| **Set clipboard** | Fills the clipboard. xpando's own engine only; see [[Set Clipboard]]. | Advanced mode |
| **Program output** | The output of a program. | Advanced mode |
| **Form** | Asks several questions when the phrase expands. | Advanced mode |

Normal mode offers the everyday macros. Turn on [[Advanced Mode]] for the rest. Macros of any type that a phrase already has always show.

## Using one macro inside another

A macro can use the ones before it. Example: a **Date** named `d`, a **Clipboard** named `clip`, then **Fixed text** `{{d}}_{{clip}}`. Keep a macro after the ones it uses.

## Good to know

* A macro only runs if its `{{name}}` appears in the text.
* Command and program macros run on your computer. Only use ones you understand.

In [[Advanced Mode]] each macro also shows its technical name (`vars:` in the YAML).
