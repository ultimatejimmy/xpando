# Import from espanso

If you used espanso before, xpando can copy your phrases over. xpando is its own app with its own folder, so this is a copy: **your espanso files are only read, never changed, moved or deleted.**

## First start

When xpando starts for the first time and finds an espanso setup, it shows **Bring your espanso phrases to xpando?** with:

* where it found them,
* the list of files it will copy,
* anything it will **not** copy and why (shown under "will not be copied").

Choose:

* **Import** copies the files.
* **Not now** closes the window. xpando asks again the next time it starts.
* **Don't ask again** stops the offer. You can still import later from **Settings**.

## What is copied

* Your folders of phrases: every `.yml` or `.yaml` file in espanso's `match` folder, including files whose name starts with `_` and anything in subfolders (for example installed packages).
* Your app settings: every `.yml` or `.yaml` file in espanso's `config` folder, so the apps each folder works in are kept.

## What is not copied

* Files that are not YAML (for example pictures) and shortcuts or links.
* Files that look like a copy made by a sync service, such as `base (conflicted copy).yml`. See [[Sync Tips]].

xpando lists these so nothing disappears silently.

## Nothing is overwritten

If xpando already has a file with the same name, the existing file is kept and the import lists it as "already there". So you can run the import again safely.

## Import later

**Settings**, then **Find espanso phrases**, shows the same list and asks again.

## Where xpando looks

| System | Folders checked |
|---|---|
| Windows | `%APPDATA%\espanso` |
| macOS | `~/.config/espanso`, then `~/Library/Application Support/espanso` |
| Linux | `$XDG_CONFIG_HOME/espanso` (default `~/.config/espanso`) |

A folder named by the `ESPANSO_CONFIG_DIR` setting of your computer is checked first.

## After importing

Your espanso is still installed and its folder is untouched. Two engines would both expand the same triggers, so stop espanso once xpando works for you. Phrases are now edited in xpando's [[Data Folder]]; changes you make in espanso's folder after importing do not appear in xpando.
