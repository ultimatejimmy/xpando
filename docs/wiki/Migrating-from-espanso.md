# Migrating from espanso

If you already use [espanso](https://espanso.org), xpando can copy your phrases over. This page explains what is copied, what is not, and how xpando is different from stock espanso.

## xpando is its own app

xpando is built on espanso, but it does **not** run on top of an espanso install, and it does not try to share espanso's folder.

* xpando comes with **its own build of espanso's engine**. You do not need espanso installed.
* xpando keeps **its own data folder** (see [Settings and Data Folder](Settings-and-Data-Folder)). Its engine is started with that folder, so it never reads or writes espanso's own folder.
* xpando's files stay in espanso's format, so most of what you know still applies. But a few things are xpando's own, and stock espanso does not understand them: the **Set clipboard** macro (see [Macros](Macros)) is only understood by xpando's engine, and folder labels are saved on phrases as `xpando_folder:`, which stock espanso ignores.
* Do not point xpando's data folder at espanso's folder. xpando is not designed to share a folder with stock espanso.

## Import your phrases

The first time xpando starts, if it finds an espanso setup (and has no data of its own yet), it shows **Bring your espanso phrases to xpando?** The dialog tells you:

* where it found your espanso setup,
* how many files will be copied and which ones,
* anything it will **not** copy and why ("N will not be copied").

Choose:

* **Import**: copy the files. A summary says what was copied.
* **Not now**: close the window. xpando asks again the next time it starts.
* **Don't ask again**: stop asking. You can still import later.

To import later: **Settings**, then **Find phrases** (under **Import from espanso**).

### What is copied

* Your phrase files: every `.yml` or `.yaml` file in espanso's `match` folder, including files whose name starts with `_` and files in subfolders (for example installed packages).
* Your app settings: every `.yml` or `.yaml` file in espanso's `config` folder. This keeps which programs your files work in.

### What is not copied

* Files that are not YAML (for example pictures), and shortcuts or links.
* Files that look like a copy made by a sync service, such as `base (conflicted copy).yml`.

xpando lists these in the dialog so nothing disappears silently.

### Safe to repeat

Your espanso files are only **read**, never changed, moved or deleted. If xpando already has a file with the same name, it keeps its own file and lists the other as "already in xpando, kept". So you can import again safely.

### Where xpando looks

| System | Folders it checks |
|---|---|
| Windows | `%APPDATA%\espanso` |
| macOS | `~/.config/espanso`, then `~/Library/Application Support/espanso` |
| Linux | `$XDG_CONFIG_HOME/espanso` (`~/.config/espanso` if that is not set), then `~/.config/espanso` |

A folder named by the `ESPANSO_CONFIG_DIR` setting of your computer is checked first.

## What your phrases look like after the import

* Each of your espanso `match` files becomes a **folder** with the file's name. Phrases from `base.yml` are in a folder called `base`. You can move them to your own folders. See [Folders and Apps](Folders-and-Apps).
* Phrases in files that espanso limits to certain programs (through the `config` folder) show up under **Apps** if the limit is a plain list of programs. A limit that was written by hand in another way (for example by window title) is kept, and the phrase details say so.
* Phrases that xpando's form cannot edit yet (for example **regex triggers**, or text stored as markdown, an image path or a form) are shown and kept, but read-only in the form. Use [Advanced Mode](Advanced-Mode) to edit their YAML.
* Installed espanso packages are copied as files. xpando will not change them. Change packages with espanso.
* Your comments and layout stay as they were when xpando saves a file.

## After importing: stop espanso

Your espanso is still installed, and its folder is untouched. But **two expanders would both type the same triggers** if both run at once. Stop espanso once xpando works for you (for example with `espanso stop`, or quit it from its icon), and consider turning off espanso's start at sign-in. xpando does not (yet) warn you when a stock espanso is running.

From then on, edit phrases in xpando. Changes you make in espanso's folder after importing do not appear in xpando.

## What stays the same

Triggers, multiple triggers, whole-word and case options, `date`, `clipboard`, `random`, `choice`, `shell`, `script` and `form` macros, rich text (`html:`), regex triggers, and app-specific files are all part of espanso's format. xpando reads and writes them. For the full list of what the engine can do, see the [espanso documentation](https://espanso.org/docs/).

---

Related pages: [Settings and Data Folder](Settings-and-Data-Folder) | [Folders and Apps](Folders-and-Apps) | [Macros](Macros) | [FAQ](FAQ)
