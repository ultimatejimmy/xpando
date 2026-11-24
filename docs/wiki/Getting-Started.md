# Getting Started

xpando has three parts you will meet:

* **The app**: the window you manage phrases in.
* **The engine**, which runs in the background and watches what you type. When you type a trigger it replaces it with the phrase.
* **Your phrase files**: plain text files (YAML) that hold your phrases. The app reads and writes them for you. You never have to open them.

## The window

![The three panes](images/main-window-annotated.png)

* **Header**: the xpando name, a search box ("Search triggers, names and text"), the **Advanced** switch and **Settings** (the gear), where you see or change the [[Data Folder]].
* **Folders** (left): **All phrases**, then your folders. **New folder** adds one.
* **Phrase list** (middle): the phrases of the selected folder. **New phrase** adds one.
* **Details** (right): the selected phrase with **Edit** and **Delete**, its text, its **Macros** and its **Options**.

Pick a phrase and press **Edit** to change it. Press **Save** (or Ctrl+S) when done, or **Cancel** (or Esc) to leave without saving. If you leave with unsaved changes, xpando asks "Discard your changes?" so nothing is lost by accident.

Every save is safe: the file is written in one step and a backup copy (`.bak`) is kept.

## Next

* [[Install]]
* [[Your First Phrase]]
