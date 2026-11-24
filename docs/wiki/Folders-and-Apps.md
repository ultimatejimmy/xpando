# Folders and Apps

A **folder** is a group of phrases. Each folder is one file in your `match` folder. A folder can work in **all apps** or only in certain ones.

![Folder options](images/folder-options.png)

## Create a folder

Press **New folder**, type a **Folder name** (letters, digits, spaces, `,`, `-` and `_`) and press **Create folder**. A new folder applies to all apps.

## Folder options

Select a folder and open **Folder options**:

* **Apps...** chooses where the phrases work.
* **Rename** changes the name and updates everything that refers to it.
* **Delete** asks for confirmation, lists every file it touches and keeps a backup copy so the folder can be restored.

The line under the folder says where it applies, for example "Applies to: All apps", "Word only", "All apps except Word", or **Not used** (nothing loads it, so its phrases never expand; shown in a warning colour).

## Limit a folder to some apps

In **Apps...**, under "Where should this folder work?", pick **Only these apps**:

* **Programs**: choose them from the list (see [Choosing programs](#choosing-programs)).
* **Window title**: only windows whose title has this text, such as one website. Capitals do not matter. With programs chosen, both must match.

Pick **All apps** to make it work everywhere again.

Behind the scenes xpando writes a file in your `config` folder. File names and patterns are shown in [[Advanced Mode]] only.

## Choosing programs

The same picker is used for a folder's apps and for a phrase's **Works in**. Search by name, and pick from three lists:

* **Common**: well known programs (Word, Excel, Outlook, Chrome, Teams, Slack, Notepad, ...) with their friendly names. Windows only.
* **Open now**: programs that have a window open at this moment. Open the program first if it is missing.
* **Installed**: programs found through the Start Menu and the registry. The first look can take a few seconds; the answer is kept until xpando closes. Windows only.

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

* Programs are matched by file name, not by full path. On macOS and Linux, "Open now" uses the process list (`ps`); on Linux that is every running process. "Common" and "Installed" are Windows only, and "Browse..." works on Windows (tested) and Linux with `zenity` (not tested); on macOS use "Open now". The macOS and Linux paths have not been tested on those systems.

* Folders inside subfolders of `match`, and the `packages` folder, cannot be changed here.
* If a configuration is too unusual for xpando to change safely, the command is disabled and says why. Nothing is changed.
* Only one configuration is active at a time: the first app-specific one that matches the window, otherwise the default. A folder works in an app only if that configuration loads it.
