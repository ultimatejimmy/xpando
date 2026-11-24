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

1. xpando downloads the update from xpando's releases on GitHub. It shows the progress.
2. It checks the update's **digital signature**. Every official xpando update is signed with a private key that only the xpando maintainers have; the matching public key is built into the app. If the signature is missing or does not match, even a single byte differs, xpando discards the download and stops. Nothing is installed.
3. **Windows:** it stops the background engine, starts the installer (it shows a small progress window and asks nothing) and closes xpando so the update can finish. xpando starts again afterwards.
4. **macOS and Linux (AppImage):** it replaces the app in place. Close xpando and start it again to use the new version.
5. **Linux, installed some other way (for example from the `.deb`):** xpando cannot replace itself, so it opens the release page in your browser. Download and install from there.

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
* **"did not pass its signature check"**: the download was damaged or changed on the way, or the release was not signed by xpando. xpando discarded it. Try again later, or download the installer from the [releases page](https://github.com/ultimatejimmy/xpando/releases) yourself.
* **"Version ... was installed" but you still see the old one on macOS or Linux**: close xpando completely and start it again.
* xpando needs the `curl` program to check for updates. Windows 10 and later, macOS and most Linux systems have it.
