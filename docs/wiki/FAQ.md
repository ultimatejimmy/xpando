# FAQ

Quick answers. Can't find yours? [Open an issue](https://github.com/ultimatejimmy/xpando/issues).

## About xpando

| Question | Answer |
| :------- | :----- |
| **Is xpando the same as espanso?** | No. It is a fork: the engine that detects triggers and types text comes from espanso, and the app around it is new. |
| **Is it free?** | Yes. xpando is free and open source under the GPL-3.0 license, the same as espanso. |
| **Does xpando send my text anywhere?** | No. Everything stays in files on your computer (or in a folder you choose, see below). |
| **Which systems are supported?** | Windows first. macOS and Linux are built automatically but get less hands-on testing. |
| **What do the version numbers mean?** | They are dates, like `2026.11.23`. Betas end in `-beta`. See [[Releasing]]. |

## Your phrases and files

| Question | Answer |
| :------- | :----- |
| **Where are my phrases?** | In xpando's data folder, one file per folder. The default is `%APPDATA%\xpando` on Windows. You can change it in Settings. See [[Data Folder]]. |
| **Can I keep them in OneDrive or Google Drive?** | Yes. Point the data folder at a synced folder to back it up or share it between your devices. xpando shows a notice if the sync service leaves a "conflicted copy" file. See [[Sync Tips]]. |
| **I used espanso before. Can I bring my phrases?** | Yes. xpando offers to import your espanso files the first time it starts, and you can do it later from Settings. It only copies them and never changes the originals. See [[Import from espanso]]. |
| **Can I still edit the files by hand?** | Yes. The YAML files are the source of truth, and there is no separate database. Comments and layout are kept when xpando saves. |
| **Will xpando damage my files?** | It is built not to. Saves are atomic, a one-time `.bak` backup is made before the first edit, and a save is refused if the file changed underneath you. |

## Words and features

| Question | Answer |
| :------- | :----- |
| **Why "macro" and not "variable"?** | It is the plain-language word for the changing pieces of a phrase, such as today's date. See [[Macros]]. The files still use espanso's name, `vars`. |
| **Do I need espanso installed?** | No. xpando ships with its own engine. |
| **Can I run xpando and espanso together?** | Not at the same time. Both would expand the same triggers. Quit espanso first. |
| **What is Set clipboard?** | A macro that puts text on the clipboard for you to paste. See [[Set Clipboard]]. |

## Help

| Question | Answer |
| :------- | :----- |
| **Something doesn't work.** | Try [[Troubleshooting]] first. |
| **How do I report a bug or ask for a feature?** | [Open an issue](https://github.com/ultimatejimmy/xpando/issues). |
