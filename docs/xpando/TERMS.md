# Words used in xpando

The app uses one word for each idea. Use the same words in the wiki, the README and the release notes.

## Glossary (what users see)

| Word | Meaning |
|---|---|
| **Phrase** | One piece of text that xpando types for you when you type its trigger. |
| **Trigger** | The short text you type to get the phrase, for example `;sig`. A phrase can have several. |
| **Text** | What the phrase puts in. Either plain text or rich text (bold, links, lists). |
| **Macro** | A dynamic piece inside the text that is filled in when the phrase expands, such as today's date, whatever is on the clipboard, or the output of a command. It shows in the text as `{{name}}`. |
| **Setting** | One field of a macro, for example the date format of a Date macro or the command of a Command output macro. Not to be confused with a config file. |
| **Folder** | A group of phrases. Each folder is one file, and can be limited to certain apps. |
| **Works in** | The apps a phrase works in: **All apps** or **Only these apps**. Changing it moves the phrase to the folder that has those apps. |
| **Update** | A newer version of xpando. xpando tells you when one exists; you choose when to install it. |
| **Beta** | An early version of xpando that is offered only to people who turn on "Get beta versions". It may have problems. |
| **Advanced mode** | A switch in the header. It shows the technical side: file names, line numbers, YAML, option details, macro names and every setting. Everything stays reachable in normal mode through "More options" or Advanced mode, nothing is removed. |

Rules for writing:

* Never say "variable", "placeholder" or "parameter" to a normal-mode user. Say **macro** for the thing and **setting** for its fields.
* Say "config file" (not "settings file") for espanso's own files, so "settings" only ever means a macro's fields.
* In Advanced mode the technical name may appear as secondary text, for example the Macros section says "(`vars:` in the YAML)". The `{{name}}` form is always shown, because that is what is in the text.

## Code-level mapping

| User-facing | In the code and the files |
|---|---|
| Macro | An entry of the match's `vars:` list in YAML (espanso calls it a variable). `VarSpec` in the TypeScript, `xpando_core::vars` in Rust, Tauri commands `get_phrase_vars`, `save_phrase_vars`, `create_phrase_with_vars`. |
| Macro name | `name:` of the `vars` entry. Used in the text as `{{name}}`. |
| Macro type ("What it fills in") | `type:` (`date`, `clipboard`, `echo`, `random`, `choice`, `shell`, `script`, `form`, and xpando's `set_clipboard`). |
| Setting | A key under the entry's `params:`. |
| Cursor position | The `$\|$` marker in the text. It is not a macro entry. Plain text only. |
| Trigger | `trigger:` or `triggers:`. |
| Phrase | One entry of `matches:`. |
| Folder | One match file; its scope is stored in app-specific config files. |

The backend contract (command names, `VarSpec`, YAML field names) uses the technical words and does not change.
Error messages written in Rust (`xpando-core`) that reach the user use the user-facing words.
