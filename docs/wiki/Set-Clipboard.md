# Set Clipboard

> **xpando's own engine only.** A file that uses this macro is not understood by stock espanso, which will report an error for it.

**Set clipboard** puts text on the clipboard *after* a phrase has been typed. It is offered in [[Advanced Mode]] under **Insert macro**.

## Example

A phrase `:tdc` that types a date plus the clipboard, and also leaves that same text on the clipboard:

```yaml
- trigger: ":tdc"
  replace: "{{out}}"
  vars:
    - name: d
      type: date
      params: { format: "%Y%m%d" }
    - name: clip
      type: clipboard
    - name: out
      type: echo
      params: { echo: "{{d}}_{{clip}}" }
    - name: sc
      type: set_clipboard
      params: { text: "{{out}}" }
```

## How it behaves

* It has one setting, **text**, which can use earlier macros such as `{{out}}`.
* It adds nothing to the typed text. It only sets the clipboard.
* The clipboard is set **after** the phrase has been inserted. xpando normally restores your old clipboard after pasting; this macro deliberately replaces it afterwards, so its text is what remains.
* If the phrase fails (for example a form is cancelled), nothing is written.
* If several Set clipboard macros run, the last one wins.
* Put it after the macros it uses.
* The phrase text must contain at least one `{{...}}` reference (as `{{out}}` above), otherwise the macros never run.

## Replaces the PowerShell trick

Before this, phrases used a command macro running `Set-Clipboard`. That was slow and Windows-only. Set clipboard works the same on every system. Existing phrases keep working; nothing is converted automatically yet.

## Status

It has been tested with automated tests, but not yet by hand on every system (macOS and Linux in particular). Report problems in [Issues](https://github.com/ultimatejimmy/xpando/issues).
