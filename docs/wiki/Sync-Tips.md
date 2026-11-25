# Sync Tips

Putting your [data folder](Settings-and-Data-Folder) in OneDrive, Google Drive, Dropbox or a similar service backs it up and shares it between computers. These tips keep it trouble free.

## Good habits

* **Make sure the folder is fully downloaded.** Choose "Always keep on this device" (OneDrive), "Available offline" (Google Drive) or the Dropbox equivalent for the xpando folder. A file that is only online may not open when xpando needs it.
* **Use one computer at a time.** Edit on one computer, let the sync finish, then edit on the other. Two computers saving the same file at once is what creates conflicts.
* **Watch for the warning.** If the folder cannot be reached, xpando shows **xpando cannot reach your data folder.** and does not use another folder. Fix the connection and click **Try again**.
* **Set up each computer.** On every computer, open **Settings**, click **Change...** and pick the same synced folder. Choose **Just use this folder** when it already has phrases in it.

## Conflicted copies

When two computers change the same file, a sync service keeps both and renames one. For example:

| Service | Looks like |
|---|---|
| Dropbox, Nextcloud | `base (conflicted copy).yml`, `base (Jane's conflicted copy 2026-01-31).yml` |
| OneDrive | `base-DESKTOP-ABC123.yml` |
| Google Drive, Windows | `base (1).yml` |
| Syncthing | `base.sync-conflict-20260131-120000-ABCDEFG.yml` |

xpando checks the `match` and `config` folders for names like these. If it finds some, it shows a banner: "N files look like a copy made by your sync service." It lists each file and which file it seems to be a copy of.

This matters because xpando (and the engine) load **every** file in the folder. Both versions are active, so a trigger can expand twice, or an older version can win.

To fix it:

1. Open the data folder in File Explorer (or Finder).
2. Compare the two files. Copy over anything you need from the copy.
3. Delete the copy you do not want.

xpando never deletes or merges these files for you. A file you named yourself, such as `notes (1).yml`, can look like a conflicted copy. If that is the case, click the **x** to hide the banner.

## Saving is safe

* xpando writes a file in one step and keeps a one-time `.bak` copy of the file from before its first change.
* Before saving, xpando checks that the file did not change since you opened it. If a sync brought a newer version, you get **Reload** instead of an overwrite.
* When you come back to the xpando window, it reads the folder again, so changes from the sync show up.

---

Related pages: [Settings and Data Folder](Settings-and-Data-Folder) | [Troubleshooting](Troubleshooting) | [FAQ](FAQ)
