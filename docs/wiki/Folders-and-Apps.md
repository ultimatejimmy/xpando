# Folders and Apps

**Folders** and **apps** are two separate ways to organize phrases.

* A **folder** is only a label you put on a phrase, such as "Client work". It has nothing to do with where the phrase works. Put a phrase in a folder, or take it out, from the **Folder** menu on the phrase or in the editor. A phrase can be in one folder or in none.
* **Works in** says which apps a phrase expands in: all apps, or only certain programs. The **Apps** list in the sidebar shows one entry per program. A phrase that works in several programs appears under each of them.

## Folders

* **New folder** asks for a name. A new folder stays even while it is empty.
* The **...** menu on a folder renames it, or deletes it. Deleting a folder does not delete its phrases: they stay in your list, in no folder.
* Phrases that are in no folder are listed under **No folder** and in **All phrases**.
* Behind the scenes the folder is stored on the phrase as `xpando_folder:` in its entry. espanso ignores it.
* Phrases that were in your files before this existed keep working: a phrase in a file such as `base.yml` is in a folder named after the file, until you file it somewhere else. Phrases in files xpando made for one program (Word, Firefox...) start in no folder.

## Choosing programs

The same picker is used for a folder's apps and for a phrase's **Works in**. Search by name, and pick from two lists:

* **Installed**: programs found through the Start Menu and the registry. The first look can take a few seconds; the answer is kept until xpando closes. Windows only.
* **Open now**: programs that have a window open at this moment. Open the program first if it is missing.

**Browse...** opens a file dialog to pick a program file (`.exe`). Chosen programs show as chips with a remove button. In [[Advanced Mode]] there is also a field to type a file name such as `winword.exe`.

Everything is keyboard operable: arrow keys move in the tab row and the list, Space or Enter picks or unpicks a program.

## Works in (one phrase)

espanso limits apps per folder, not per phrase. So xpando offers **Works in** on each phrase (**All apps** or **Only these apps**) and keeps the rule by moving the phrase to the right folder:

* Choosing apps puts the phrase in the folder that works in exactly those programs. If there is none, xpando creates one named after them (for example "Word" or "Word, Outlook") and moves the phrase there. Several programs share one folder.
* Choosing **All apps** moves it back to the folder it came from (if xpando remembers it), otherwise to your default all-apps folder, and you can pick another one.
* Before anything happens the dialog says what will happen, for example: "This moves the phrase to the folder "Word"."  Afterwards an **Undo** button moves it back. A folder xpando created stays (empty) after an undo; delete it under Folder options if you do not need it.
* Folders for specific apps are listed under **Apps** in the folder list, so it is clear why a phrase moved.
* The phrase text, its description comment and all its settings travel unchanged. A backup copy of each file is kept the first time it changes. Either both files change or neither.

xpando only reuses folders whose apps are a plain list of programs. A folder that also filters on a window title, or uses a hand-written pattern, is never reused or changed: a new folder is created instead and the dialog says so. A move is refused when the same trigger already exists in the destination (or, for an app folder, in a folder that works in all apps), when a file is read-only, or when the phrase belongs to an installed package.

[[Advanced Mode]] shows the files that will change in the dialog.

## Limits

* Programs are matched by file name, not by full path. On macOS and Linux, "Open now" uses the process list (`ps`); on Linux that is every running process. "Installed" is Windows only, and "Browse..." works on Windows (tested) and Linux with `zenity` (not tested); on macOS use "Open now". The macOS and Linux paths have not been tested on those systems.

* Folders inside subfolders of `match`, and the `packages` folder, cannot be changed here.
* If a configuration is too unusual for xpando to change safely, the command is disabled and says why. Nothing is changed.
* Only one configuration is active at a time: the first app-specific one that matches the window, otherwise the default. A folder works in an app only if that configuration loads it.
