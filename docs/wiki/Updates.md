# Updates

xpando tells you when a new version is available. You decide when to install it. It never installs anything by itself.

## How you find out

* **Once a day.** When xpando starts, and while it stays open, it checks GitHub (where xpando's releases are published) whether a newer version exists. If it does, a small notice appears under the header: **Version 2026.11.23 is available**.
* **Whenever you like.** Open **Settings**, go to **Updates** and press **Check for updates**. It tells you whether you have the newest version.
* **Quietly.** If you are offline or GitHub cannot be reached, the daily check says nothing and tries again later. Only the **Check for updates** button shows an error.

The check sends no information about you or your computer. It only asks GitHub for the list of xpando releases.

## The notice

| Button | What it does |
|---|---|
| **Update now** | Downloads the new version, checks it, and starts the installer. |
| **Later** | Hides the notice until you start xpando again. |
| **Skip this version** | Stops the daily check from telling you about this version. A newer version will still be announced. |

## What "Update now" does

1. xpando downloads the installer from xpando's releases on GitHub into a temporary folder. It shows the progress.
2. It checks the file against the list of checksums published with the release (`SHA256SUMS.txt`). If the file does not match, or is not on the list, xpando deletes it and stops. Nothing is installed.
3. **Windows:** it starts the installer and closes xpando so the update can finish. Open xpando again afterwards.
4. **macOS and Linux:** it shows the downloaded file in your file manager. Open it to install. Installing on these systems from inside xpando is planned.

Your phrases and settings are not touched by an update.

## Beta versions

Betas are early versions, released so people can try new things before everyone gets them. They may have problems that finished versions do not.

* Turn on **Get beta versions** in Settings, under Updates, to be told about betas as well as finished versions. They are labelled **Beta**.
* Leave it off (the default) and you only ever see finished versions.
* If you are using a beta and you turn beta versions off, xpando keeps your beta until a finished version that is **newer** comes out. It never quietly replaces your version with an older one.

Version numbers are dates. For example `2026.11.23-beta2` is the second beta, which comes before the finished `2026.11.23`. See [[Releasing]].

## Settings

| Setting | Meaning |
|---|---|
| **Check automatically every day** | On by default. Turn it off to check only when you press the button. |
| **Get beta versions** | Off by default. |

Both are saved in `settings.yml` in xpando's settings folder (next to the setting for your [[Data Folder]]): `auto_check` and `update_channel` (`stable` or `beta`).

## If something goes wrong

* **"Could not reach GitHub"**: check your internet connection and try again. A company network or a firewall may block it.
* **"does not match its published checksum"**: the download was damaged or changed on the way. xpando discarded it. Try again later, or download the installer from the [releases page](https://github.com/ultimatejimmy/xpando/releases) yourself.
* **Nothing happens on macOS or Linux after the download**: open the file shown in your file manager.
* xpando needs the `curl` program to check for updates. Windows 10 and later, macOS and most Linux systems have it.
