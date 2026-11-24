# Troubleshooting

## Nothing expands

* Did you save? An unsaved edit shows "Unsaved changes".
* Is the folder **Not used**? Under the folder name, "Applies to" shows this in a warning colour. Open **Folder options**, **Apps...**, and choose **All apps**.
* Is the folder limited to an app that is not in front?
* **Only expand when I type it as a whole word** is on and the trigger is inside a word.
* A macro only runs if `{{name}}` is in the text.
* Is the engine using the folder xpando edits? In [[Advanced Mode]] the status bar shows the data folder path. See [[Data Folder]].
* Is a stock espanso still running? It would also expand your triggers, or conflict with xpando.
* A banner says xpando cannot reach the data folder, or lists conflicted copies? See [[Data Folder]] and [[Sync Tips]].

## "Can't save yet"

The message beside **Save** says what to fix, for example an empty trigger or text.

## The file changed on disk (Reload)

The file was edited elsewhere while you worked. Press **Reload** to see the current version. Your unsaved draft is kept read-only so you can copy it.

## Characters are dropped or in the wrong order

Turn on [[Advanced Mode]], open **More options** and set **How the text is inserted** to **Paste from the clipboard**. If an app blocks paste, choose **Type with the keyboard**.

## Rich text looks wrong in some app

Programs read formatting differently. Try simpler formatting, or use plain text. See [[Rich Text]].

## Set clipboard macro gives an error in espanso

That macro needs xpando's own engine. See [[Set Clipboard]].

## A phrase is read-only

Things the form cannot edit yet (such as regular expression triggers) are shown read-only. Use [[Editing YAML]].

## macOS and Linux

macOS needs accessibility and input monitoring permissions. On Linux, Wayland support is experimental.

## Still stuck

Open an [issue](https://github.com/ultimatejimmy/xpando/issues) with your version, system and what you typed. Do not paste private phrases.
