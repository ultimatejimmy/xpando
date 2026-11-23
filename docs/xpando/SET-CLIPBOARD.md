# `set_clipboard` variable (M4b)

Built-in replacement for the PowerShell hack
(`type: shell`, `shell: powershell`, `cmd: "Set-Clipboard -Value $env:ESPANSO_OUT"`)
used by phrases converted from PhraseExpress `{#setclipboard}`.

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

Stock espanso does not know this type: files using it are xpando-only.

## How things work today (findings)

**Reader.** `espanso-render/src/extension/clipboard.rs`: `ClipboardExtension<'a>` holds a
`&dyn ClipboardProvider` (`get_text() -> Option<String>`); `None` becomes an extension error.
In `espanso/src/cli/worker/engine/mod.rs` a `ClipboardAdapter` (wrapping the real
`espanso_clipboard::Clipboard` plus per-config `ClipboardOperationOptions`) implements the trait,
and the extension is passed in the `Vec<&dyn Extension>` given to `espanso_render::create`.
The `RendererAdapter` (`.../middleware/render/mod.rs`) calls the renderer; the engine's
`RenderMiddleware` calls the adapter.

**Writer backends.** `Clipboard::set_text(&self, text, &ClipboardOperationOptions) -> Result<()>`:
Win32 (`win32/native.cpp`, `EmptyClipboard` + `SetClipboardData`), macOS (`cocoa/native.mm`
pasteboard), X11 (native, or `xclip` when `x11_use_xclip_backend`), Wayland (`wl-copy` via the
fallback backend, with a command timeout). All return `Err` on failure, none panic. The clipboard
object is created once at worker start (`.expect(...)`, so an unavailable backend already stops the
worker at startup, not per expansion).

**Order of events for one expansion** (single thread, `Engine::run`):
`processor.process(event)` runs all middleware, including *rendering* (variables evaluated here),
and returns a list of events; then each is dispatched in order. For plain text the chain is
`Rendered -> MatchInjected + TextInject` (also `HtmlInject`, `MarkdownInject -> HtmlInject`,
`ImageInject`, plus cursor-hint key events). `TextInjectExecutor` picks the injector:
event (typing) or clipboard (paste), by `force_mode`, the global `backend` mode, or the Auto
threshold.

**Clipboard restore.** Only the paste injector (`ClipboardInjectorAdapter`, in
`espanso/src/cli/worker/engine/dispatch/executor/clipboard_injector.rs`) touches the clipboard
during injection: it creates a `ClipboardRestoreGuard` (reads the current *text*), `set_text`s the
expansion, sends the paste shortcut, and when `inject_text` returns the guard's `Drop` sleeps
`restore_clipboard_delay` ms then `set_text`s the old content back (only if `restore_clipboard` is
on and the old content was text). Therefore:

* A write at *render time* is overwritten by the expansion and then by the restore.
* A write has to happen after `TextInjector::inject_text` returned (guard dropped).

## Design

* **Type** `set_clipboard`; **params** `text:` (string; numbers/bools are coerced like `echo`;
  `{{var}}` references to earlier variables work through the normal `inject_vars`, which is on by
  default). Missing/invalid `text` is a render error.
* **Output** is the empty string, so it composes with other variables and does not appear in the body.
* **Semantics.** Evaluating the variable only records the text ("pending"). It is written to the
  clipboard right after the expansion's injection event (text, html, markdown or image) has been
  dispatched, i.e. after the paste and after the clipboard restore, so the user's old clipboard is
  deliberately replaced. If several `set_clipboard` variables run in one expansion the last wins.
* **`force_mode: clipboard` / paste backend:** works the same, the write happens after the restore
  guard. With `restore_clipboard: false` no guard waits for the target app to consume the paste, so
  the write waits `restore_clipboard_delay` first (otherwise the paste could read the new text).
* **Failure.** If rendering fails or is aborted (form cancelled, a later variable errors) the pending
  text is dropped and nothing is written. A backend error at write time is logged
  (`unable to set clipboard content ...`); the expansion has already been typed, nothing crashes.
* **Concurrency.** Evaluation order inside a template follows espanso's dependency order: local
  variables run in declaration order, so `set_clipboard` should be declared after the variables it
  uses. The worker is single-threaded for render + dispatch; no locking is needed.
* **Limitation (existing renderer behaviour).** Variables are only evaluated when the body contains
  a `{{...}}` reference; a body without any placeholder would never run `set_clipboard`.

## Implementation

No change in `espanso-engine` and no change in `espanso-clipboard`. The "deferred effect" lives in
the worker wiring (`espanso/src/cli/worker/engine/deferred_clipboard.rs`):

* `DeferredClipboard` implements `ClipboardWriter` (the new `espanso-render` trait) by storing the text.
* `DeferredClipboardRenderer` wraps the engine `Renderer`: clears pending at render start and on error.
* `DeferredClipboardDispatcher` wraps the engine `Dispatcher`: after dispatching an expansion injection
  event, writes the pending text with the real clipboard.

Not changing the engine keeps the diff to upstream tiny and all existing event ordering identical when
the variable is unused (pending is always empty and both wrappers delegate).

## Verification status

* Unit tests: extension (`espanso-render`), wrappers (`espanso`), an end-to-end render of
  `{{out}}` + `set_clipboard`.
* Not verified: real clipboard on any OS (the daemon was deliberately not started); macOS and Linux
  backends were only read, not built. Manual matrix still to run: paste vs type, restore on/off,
  empty and multi-line text, image on the clipboard beforehand, X11 native vs xclip, Wayland.
