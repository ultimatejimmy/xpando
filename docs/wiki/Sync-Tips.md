# Sync tips

Putting your [[Data Folder]] in OneDrive, Google Drive, Dropbox or a similar service backs it up and shares it between computers. These tips keep it trouble free.

## Good habits

* **Make sure the folder is fully downloaded.** Choose "Always keep on this device" (OneDrive), "Available offline" (Google Drive) or the Dropbox equivalent for the xpando folder. A file that is only online may not open when xpando needs it.
* **Use one computer at a time.** Edit on one computer, let the sync finish, then edit on the other. Two computers saving the same file at once is what creates conflicts.
* **Watch for the warning.** xpando shows a warning if the folder cannot be reached and will not use another one.

## Conflicted copies

When two computers change the same file, sync services keep both and rename one, for example:

| Service | Looks like |
|---|---|
| Dropbox, Nextcloud | `base (conflicted copy).yml`, `base (Jane's conflicted copy 2026-01-31).yml` |
| OneDrive | `base-DESKTOP-ABC123.yml` |
| Google Drive, Windows | `base (1).yml` |
| Syncthing | `base.sync-conflict-20260131-120000-ABCDEFG.yml` |

xpando checks `match` and `config` for names like these and shows a banner listing them and which file each seems to be a copy of. xpando loads every file in the folder, so both versions are active: a trigger can expand twice or an older version can win.

To fix it, open the data folder, compare the two files, keep the version you want (copy over anything you need from the other) and delete the other. xpando never deletes or merges files for you. A file you named yourself, such as `notes (1).yml`, can look like a conflicted copy; hide the banner if that is the case.

## Saving is safe

* xpando writes a file in one step and keeps a `.bak` copy of the previous version.
* Before saving, xpando checks the file did not change since you opened it. If a sync brought a newer version, you get **Reload** instead of an overwrite.
* When you return to the xpando window it looks at the folder again, so changes from the sync show up.
