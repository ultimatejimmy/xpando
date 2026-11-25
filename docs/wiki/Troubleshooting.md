# Troubleshooting

xpando is in early development, so you may hit rough edges. This page lists the most likely problems and what to try.

## Nothing expands

Go through this list in order.

1. **Is the engine running?** Open **Settings** (the gear). If you see **Expansion is off**, click **Turn on**. If xpando cannot start the engine, the message under it says why (see [Engine messages](#engine-messages)).
2. **Did you save the phrase?** An unsaved edit shows **Unsaved changes**. Click **Save**.
3. **Is the trigger typed exactly?** Triggers are exact, including capital letters and symbols. Check the phrase's trigger in its details.
4. **Is the phrase set to work in another program?** Look at **Works in** in the phrase details. If it says "Only" and a program name, it only expands there. Change it to **All apps** if you want it everywhere. See [Folders and Apps](Folders-and-Apps).
5. **Does the phrase say "No setting loads the folder ... so this phrase does not expand"?** Then nothing loads the file the phrase is in. Click **Change** next to **Works in** and choose **All apps**.
6. **Is whole word turned on?** If **Only expand when I type it as a whole word** is on, the trigger only works with a space or punctuation on both sides. It will not fire in the middle of a word.
7. **Does the text use a macro?** A macro only runs when its `{{name}}` is in the text. See [Macros](Macros).
8. **Is another expander running?** If espanso (or a similar program) is also running, both may type the same triggers, or get in each other's way. Quit the other one. See [Migrating from espanso](Migrating-from-espanso).
9. **Is a banner showing at the top?** A banner about the data folder or conflicted copies means something needs attention (see below).
10. **Does the program block simulated typing?** Some programs do. Try setting **How the text is inserted** (Advanced mode, **More options**) to **Paste from the clipboard**.

## Engine messages

These messages appear under **Expansion and search** in **Settings**:

| Message | What to do |
|---|---|
| **Expansion is off** with "You turned it off" or "Phrases are not expanding." | Click **Turn on**. |
| "xpando's engine was not found next to the app. Reinstall xpando to fix this." | The engine file that comes with xpando is missing. Reinstall xpando. |
| "The engine did not start. Its log is in ..." | Open the log file named in the message. It says what went wrong. Include it (without private phrases) if you report the problem. |
| "Could not start the engine: ..." | The system reported an error when starting the engine, and the rest of the message says which. Try **Turn on** again. If it keeps failing, report it. |
| "Windows did not allow xpando to start at sign-in." | Your system blocked the start entry. Try again, or start xpando by hand. |

When the engine starts, you usually see a notification "xpando is running!". The engine's search box (**Alt+Space**) also lists **Show xpando's logs**.

## "New phrase" is greyed out

xpando needs at least one phrase file to put phrases in. On a data folder with no files, the phrase list says "No folders yet. Use New folder to start, or open Settings to bring in your espanso phrases." If you have espanso phrases, use **Settings**, **Find phrases**. If not, and the button stays grey, please [open an issue](https://github.com/ultimatejimmy/xpando/issues). As a workaround you can create a text file named `base.yml` in the `match` folder of your data folder, containing the single line `matches:`, and then reopen xpando.

## "xpando cannot reach your data folder"

The folder is missing or offline, often a OneDrive, Google Drive or Dropbox folder when you are signed out or offline. xpando does not use another folder in the meantime. Fix the connection and click **Try again**, or click **Settings** to choose another folder. See [Settings and Data Folder](Settings-and-Data-Folder).

## "N files look like a copy made by your sync service"

Your sync service left a "conflicted copy" of a file. Both versions are active, so a phrase can expand twice or an older version can win. Compare the files, keep the one you want and delete the other. See [Sync Tips](Sync-Tips).

## A phrase expands twice, or the wrong phrase expands

* The same trigger may be used twice, or one trigger may start another (`:ad` and `:addr`). Turn on [Advanced Mode](Advanced-Mode) and look at the **trigger conflicts** list. See [Trigger Conflicts](Trigger-Conflicts).
* A conflicted copy file from sync (see above) can also cause this.

## "Can't save yet"

The message beside **Save** says what to fix, for example "Every trigger needs text." or "Enter the text to insert."

## The file changed on disk (Reload)

You see a message with a **Reload** button when something else (a sync service, an editor) changed the file after you opened it. xpando does not overwrite the new version. Click **Reload** to see the current version. If the phrase changed, your unsaved draft is shown read-only so you can copy it.

## Characters are dropped or in the wrong order

Turn on [Advanced Mode](Advanced-Mode), open **More options**, and set **How the text is inserted** to **Paste from the clipboard**. If a program blocks paste, choose **Type with the keyboard** instead.

## Rich text looks wrong in a program

Programs read pasted formatting in different ways. Try simpler formatting, or use plain text. See [Rich Text](Rich-Text).

## A Set clipboard macro does not work

That macro needs xpando's own engine. If you see "This phrase uses a Set clipboard macro, but xpando's engine was not found, so it will not run.", reinstall xpando. Stock espanso cannot run it. Also remember that the text of the phrase must contain a `{{...}}` reference. See [Macros](Macros).

## A phrase is read-only in the form

Regex triggers and phrases whose text is stored as markdown, an image path or a form cannot be fully edited in the form yet. Use [Advanced Mode](Advanced-Mode) and **Edit YAML**.

## macOS and Linux

* **macOS:** the engine watches the keyboard, so macOS may need your permission in **System Settings, Privacy and Security** (for example Accessibility and Input Monitoring). The app is not signed. See [Installation](Installation).
* **Linux:** Wayland support is experimental. If the app will not start, the wxWidgets 3 GTK library may be missing. See [Installation](Installation). The macOS and Linux versions of the program picker for **Works in** are untested.

## Where things are stored

| What | Where |
|---|---|
| Your phrases and app settings | The data folder shown in **Settings** (default `%APPDATA%\xpando` on Windows). See [Settings and Data Folder](Settings-and-Data-Folder). |
| Backup copies | Next to each changed file, with `.bak` added to the name. |
| xpando's own settings (data folder choice, update choices) | A small `settings.yml` in xpando's settings folder, apart from your phrases. |
| The engine's log | `espanso.log` in the engine's own folder inside xpando's local app data. The "did not start" message shows the full path. |

## Still stuck

Open an [issue](https://github.com/ultimatejimmy/xpando/issues). Tell us your xpando version (shown in **Settings**, under **Updates**), your system, and what you typed. Please do not paste private phrases.

---

Related pages: [Getting Started](Getting-Started) | [Writing Phrases](Writing-Phrases) | [Sync Tips](Sync-Tips) | [FAQ](FAQ)
