# FAQ

**Is xpando the same as espanso?**
It is a fork. The engine that detects triggers and types text is espanso's, and the app is new. Differences are small and listed here (for example the [[Set Clipboard]] macro).

**Do I need espanso installed?**
The plan is for xpando to bundle its engine in one installer, so no.

**Can I use my existing espanso phrases?**
Yes. xpando offers to copy them the first time it starts, and you can do it later from Settings. Your espanso files are never changed. See [[Import from espanso]]. Comments and layout are kept when xpando edits a file, and a `.bak` backup is made.

**Can I still edit the files by hand?**
Yes. The files are the source of truth, and xpando never uses a separate database.

**Does xpando send my text anywhere?**
No. Everything is local files.

**Where are my phrases?**
In xpando's own data folder, one file per folder. Settings shows where it is and lets you change it (for example to OneDrive). See [[Data Folder]]. In [[Advanced Mode]] the status bar shows the path too.

**Why "macro" and not "variable"?**
It is the plain-language word for the dynamic pieces of a phrase. See [[Macros]]. The files still use espanso's name (`vars`).

**Which systems are supported?**
Windows first. macOS and Linux are built in CI but get less hands-on testing.

**What do the version numbers mean?**
They are dates. See [[Releasing]].

**How do I report a bug or ask for a feature?**
[Open an issue](https://github.com/ultimatejimmy/xpando/issues).
