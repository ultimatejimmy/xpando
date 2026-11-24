# Your data folder

xpando keeps your phrases in its own folder. You do not have to set anything up: the first time you start xpando it creates the folder for you.

## Where it is

| System | Default folder |
|---|---|
| Windows | `%APPDATA%\xpando` (for example `C:\Users\you\AppData\Roaming\xpando`) |
| macOS | `~/Library/Application Support/xpando` |
| Linux | `~/.config/xpando` (or `$XDG_CONFIG_HOME/xpando`) |

Open **Settings** (the gear icon in the header) to see the folder xpando is using. The folder has two parts: `match` (your folders of phrases) and `config` (which apps each folder works in). xpando's engine is started with this folder, so it never reads espanso's own folder.

Where xpando remembers your choice of folder is a small settings file in the app's own settings location, kept apart from your phrases so it does not travel with them.

## Use another folder

You may want your phrases in OneDrive, Google Drive or Dropbox to back them up or use them on more than one computer.

1. Open **Settings** and press **Change...**.
2. Press **Browse...** to pick a folder, or type its full path. xpando checks it and tells you if it cannot be used (it must exist, be a folder and be writable; only the last folder of the path is created for you).
3. Press **Next**. xpando asks: **Copy my phrases?**
   * **Copy my phrases** copies everything from the old folder into the new one. Nothing is moved: the old folder stays exactly as it is. Files that already exist in the new folder are kept and never replaced.
   * **Start empty** (or **Just use this folder** when the new folder already has phrases) switches without copying.

If any file cannot be copied, xpando tells you which and **keeps using the old folder**, so you never end up with half your phrases in each place.

**Use the default folder** in Settings switches back. The folder you leave is not deleted.

## If the folder cannot be reached

A cloud folder can be missing when you are signed out, offline or the disk is not connected. xpando then shows a warning at the top of the window, saying which folder it could not open. It does **not** switch to another folder behind your back and it does not create the missing folder. Fix the problem (sign in, reconnect) and press **Try again**, or open **Settings** to choose another folder.

## Changes made outside xpando

When you come back to the xpando window it looks at the folder again, so changes made by a sync service or by editing a file by hand show up. If you are in the middle of editing a phrase, xpando waits, and saving still refuses to overwrite a file that changed in the meantime (you get the **Reload** message).

See [[Sync Tips]] for working with cloud folders, and [[Import from espanso]] if you used espanso before.
