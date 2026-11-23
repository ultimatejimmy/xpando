# Folders and Apps

A **folder** is a group of phrases. Each folder is one file in your `match` folder. A folder can work in **all apps** or only in certain ones.

![Folder options](images/folder-options.png)

## Create a folder

Press **New folder**, type a **Folder name** (letters, digits, spaces, `-` and `_`) and press **Create folder**. A new folder applies to all apps.

## Folder options

Select a folder and open **Folder options**:

* **Apps...** chooses where the phrases work.
* **Rename** changes the name and updates everything that refers to it.
* **Delete** asks for confirmation, lists every file it touches and keeps a backup copy so the folder can be restored.

The line under the folder says where it applies, for example "Applies to: All apps", "Word only", "All apps except Word", or **Not used** (nothing loads it, so its phrases never expand; shown in a warning colour).

## Limit a folder to some apps

In **Apps...**, under "Where should this folder work?", pick **Only these apps**:

* **Programs**: the program's file name, for example `winword.exe`. On Windows, Task Manager, Details tab, shows it while the program runs.
* **Window title**: only windows whose title has this text, such as one website. Capitals do not matter. With programs chosen, both must match.

Pick **All apps** to make it work everywhere again.

Behind the scenes xpando writes a file in your `config` folder. File names and patterns are shown in [[Advanced Mode]] only.

## Limits

* Folders inside subfolders of `match`, and the `packages` folder, are managed by espanso and cannot be changed here.
* If a configuration is too unusual for xpando to change safely, the command is disabled and says why. Nothing is changed.
* Only one configuration is active at a time: the first app-specific one that matches the window, otherwise the default. A folder works in an app only if that configuration loads it.
