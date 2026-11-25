# FAQ

Quick answers. Can't find yours? Try [Troubleshooting](Troubleshooting), or [open an issue](https://github.com/ultimatejimmy/xpando/issues).

## About xpando

| Question | Answer |
| :------- | :----- |
| **What is xpando?** | A text expander with a desktop app for managing phrases. You type a short trigger and xpando types a longer phrase. |
| **Is it free?** | Yes. xpando is free and open source under the GPL-3.0 license, the same as espanso. |
| **Is xpando the same as espanso?** | No. It is a fork. The engine that detects triggers and types text comes from espanso. The phrase manager around it is new. See [Migrating from espanso](Migrating-from-espanso). |
| **Is it finished?** | No. xpando is in early development. It works, but expect rough edges. Please report problems. |
| **Which systems are supported?** | Windows first. macOS and Linux builds are made on every release but get much less testing. See [Installation](Installation). |
| **Does xpando send my text anywhere?** | No. Your phrases stay in files on your computer (or in a folder you choose). The only thing xpando contacts is GitHub, to ask whether a new version exists, once a day. It sends nothing about you or your computer, and you can turn the check off. See [Updates](Updates). |
| **Why does Windows warn me about the installer?** | The Windows installer is not code-signed yet, so SmartScreen may show "Windows protected your PC". Choose **More info**, then **Run anyway**. See [Installation](Installation). |
| **Do I need espanso installed?** | No. xpando comes with its own engine. |
| **Can I run xpando and espanso together?** | Not at the same time. Both would type the same triggers. |

## Your phrases and files

| Question | Answer |
| :------- | :----- |
| **Where are my phrases?** | In xpando's data folder. On Windows the default is `%APPDATA%\xpando`. You can change it in Settings. See [Settings and Data Folder](Settings-and-Data-Folder). |
| **Can I keep them in OneDrive, Google Drive or Dropbox?** | Yes. Point the data folder at a synced folder. See [Sync Tips](Sync-Tips). |
| **I used espanso before. Can I bring my phrases?** | Yes. xpando offers to copy them the first time it starts, and you can do it later from Settings. Your espanso files are never changed. See [Migrating from espanso](Migrating-from-espanso). |
| **Can I edit the files by hand?** | Yes. Your phrases are YAML files and there is no separate database. xpando keeps your comments and layout when it saves. See [Advanced Mode](Advanced-Mode). |
| **Will xpando damage my files?** | It is built not to. Saves are written in one step, xpando keeps a one-time `.bak` copy from before its first change to each file, and a save is refused if the file changed underneath it. Keep your own backup too, since xpando is still young. |
| **Can I use my phrases with plain espanso?** | Mostly yes: they are in espanso's format. Two things are xpando's own: the **Set clipboard** macro (stock espanso reports an error for it) and the folder labels (`xpando_folder:`, which stock espanso ignores). |

## Using xpando

| Question | Answer |
| :------- | :----- |
| **Why "macro" and not "variable"?** | It is the plain word for the changing pieces of a phrase, such as today's date. See [Macros](Macros). The files use espanso's name, `vars`. |
| **What is the difference between a folder and "Works in"?** | A folder is a label to keep phrases tidy. **Works in** decides which programs a phrase expands in. See [Folders and Apps](Folders-and-Apps). |
| **Can a phrase work in only one program?** | Yes. Select the phrase, click **Change** next to **Works in**, choose **Only these apps** and pick the program. |
| **Can I paste formatted text from Word?** | Yes. In the rich text editor press **Ctrl+V**, or use **From clipboard** to start a phrase from it. See [Rich Text](Rich-Text). |
| **How do I get today's date?** | Click **Insert macro**, then **Date**. See [Macros](Macros). |
| **What is the Advanced switch for?** | It shows the technical side: the YAML of each phrase, trigger conflicts, and every option. See [Advanced Mode](Advanced-Mode). |
| **How do I search from another program?** | Press **Alt+Space** (you can change it in Settings). See [Search](Search). |
| **Does xpando start when I sign in?** | Only if you turn on **Start when I sign in to this computer** in Settings. |
| **Does closing the window stop expansion?** | No. The engine runs in the background and keeps working until you quit it. |
| **How do I stop expansion?** | The xpando window has no off switch yet. The search box you open with **Alt+Space** lists a **Quit xpando** entry that stops the engine. When the engine is stopped, **Settings** shows **Expansion is off** with a **Turn on** button. |

## Updates and versions

| Question | Answer |
| :------- | :----- |
| **How do I update?** | xpando tells you when a new version is out, and you choose when to install it. See [Updates](Updates). |
| **What do the version numbers mean?** | They are dates, like `2026.11.24`. Betas end in `-beta`. See [Updates](Updates). |
| **Can I try beta versions?** | Yes. Turn on **Get beta versions** in Settings. Betas may have problems. |

## Help

| Question | Answer |
| :------- | :----- |
| **Something does not work.** | See [Troubleshooting](Troubleshooting). |
| **How do I report a bug or ask for a feature?** | [Open an issue](https://github.com/ultimatejimmy/xpando/issues). Include your xpando version, your system, and what you did. Do not paste private phrases. |
| **Can I help?** | Yes. See [Contributing](Contributing). |

---

Related pages: [Getting Started](Getting-Started) | [Troubleshooting](Troubleshooting) | [Home](Home)
