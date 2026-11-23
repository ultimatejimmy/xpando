# FAQ

**Is xpando the same as espanso?**
It is a fork. The engine that detects triggers and types text is espanso's, and the app is new. Differences are small and listed here (for example the [[Set Clipboard]] macro).

**Do I need espanso installed?**
The plan is for xpando to bundle its engine in one installer, so no.

**Can I use my existing espanso phrases?**
Yes. xpando is meant to use espanso's configuration folder, so they appear without importing. Comments and layout are kept when xpando edits a file, and a `.bak` backup is made.

**Can I still edit the files by hand?**
Yes. The files are the source of truth, and xpando never uses a separate database.

**Does xpando send my text anywhere?**
No. Everything is local files.

**Where are my phrases?**
In your espanso `match` folder, one file per folder. In [[Advanced Mode]] the status bar shows the config folder path.

**Why "macro" and not "variable"?**
It is the plain-language word for the dynamic pieces of a phrase. See [[Macros]]. The files still use espanso's name (`vars`).

**Which systems are supported?**
Windows first. macOS and Linux are built in CI but get less hands-on testing.

**What do the version numbers mean?**
They are dates. See [[Releasing]].

**How do I report a bug or ask for a feature?**
[Open an issue](https://github.com/ultimatejimmy/xpando/issues).
