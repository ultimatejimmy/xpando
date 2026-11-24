# SignPath Foundation: free Windows code signing

Status: **not applied yet.** The conditions (https://signpath.org/terms) ask for a project that is
already released in the form that will be signed and has some public reputation, and xpando has no
release yet. Apply after the first public beta has been out for a while.

## What they require, and where xpando stands

| Condition | xpando |
| :-------- | :----- |
| OSI-approved license, no proprietary parts | GPL-3.0, all open |
| Actively maintained, already released | Not released yet. Do this first |
| Built from source in a verifiable way | GitHub Actions workflow in `release.yml` |
| Signs only the project's own code | xpando is a fork of espanso. The engine is upstream code, so the "forking" conditions apply. Mention this in the application and explain that xpando's engine carries xpando changes (`set_clipboard`, version marker) |
| Privacy | The only network use is the update check (to GitHub). It can be turned off in Settings. State this in the policy |
| Team and MFA | One person for now. Roles (author, reviewer, approver) are all the same person at first. Turn on two-factor sign-in for GitHub and SignPath |
| Manual approval of each signed release | Built into SignPath's flow |

## Text for the project pages once approved

Add a "Code signing policy" heading or link on the README and the download page, with:

> Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

and a short policy: the team roles and members, the privacy statement (xpando only contacts
GitHub to check for updates, and only if enabled), and a link to the privacy details.

## Draft application text

> **Project:** xpando, a text expander with a desktop app for managing phrases, a fork of espanso.
> **Repository:** https://github.com/ultimatejimmy/xpando
> **License:** GPL-3.0, no proprietary components, no dual licensing.
> **What it does:** expands short triggers into longer text anywhere you type. Everything stays on
> the user's computer. The only network request is an optional daily update check against GitHub
> Releases, which can be turned off in Settings.
> **Why signing:** Windows SmartScreen warns on unsigned installers, which discourages people from
> installing a tool that types into other programs.
> **Build:** installers are built by the GitHub Actions workflow `release.yml` from the tagged source.
> **Fork note:** based on espanso (GPL-3.0). xpando adds a desktop app and a small engine patch;
> upstream changes are merged regularly.
> **Maintainer:** (name and contact: filled in by the maintainer)

The application itself is a form at https://signpath.org/apply and needs the maintainer's own
contact details, so it must be submitted by the maintainer.

## macOS

There is no Apple Developer account, so macOS builds are not signed or notarized. The `.dmg` is
still built, and the wiki's Install page explains the first-open step (right-click, Open). This is
a deliberate choice, not an oversight.
