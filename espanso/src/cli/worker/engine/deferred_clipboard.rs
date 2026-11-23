/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-2021 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Support for the `set_clipboard` variable.
//!
//! Rendering happens *before* injection, and a paste-based injection saves and
//! restores the user's clipboard around itself (see `ClipboardInjectorAdapter`).
//! Writing the clipboard at render time would therefore be overwritten by the
//! injected text and then by the restore. Instead, the variable only records the
//! text here ("pending"), and it is written once the injection has been
//! dispatched, which is after the restore guard has been dropped.
//!
//! Nothing in the engine crates is involved: the renderer and the dispatcher
//! are wrapped at the place where the worker wires them together. When no
//! `set_clipboard` variable is used, `pending` is always empty and both
//! wrappers just delegate.

use std::{cell::RefCell, collections::HashMap};

use anyhow::Result;
use espanso_clipboard::{Clipboard, ClipboardOperationOptions};
use espanso_engine::{
    dispatch::Dispatcher,
    event::{Event, EventType},
    process::Renderer,
};
use espanso_render::extension::set_clipboard::ClipboardWriter;
use log::{debug, error};

use super::dispatch::executor::clipboard_injector::ClipboardParamsProvider;

#[derive(Default)]
pub struct DeferredClipboard {
    pending: RefCell<Option<String>>,
}

impl DeferredClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    fn clear(&self) {
        self.pending.borrow_mut().take();
    }

    fn take(&self) -> Option<String> {
        self.pending.borrow_mut().take()
    }
}

impl ClipboardWriter for DeferredClipboard {
    fn set_text(&self, text: &str) -> Result<()> {
        // If more than one variable sets the clipboard, the last one wins
        *self.pending.borrow_mut() = Some(text.to_string());
        Ok(())
    }
}

/// Drops any pending text when a render starts, and when it fails or is aborted,
/// so that a failed expansion never touches the clipboard.
pub struct DeferredClipboardRenderer<'a> {
    inner: &'a dyn Renderer<'a>,
    deferred: &'a DeferredClipboard,
}

impl<'a> DeferredClipboardRenderer<'a> {
    pub fn new(inner: &'a dyn Renderer<'a>, deferred: &'a DeferredClipboard) -> Self {
        Self { inner, deferred }
    }
}

impl<'a> Renderer<'a> for DeferredClipboardRenderer<'a> {
    fn render(
        &'a self,
        match_id: i32,
        trigger: Option<&str>,
        trigger_args: HashMap<String, String>,
    ) -> Result<String> {
        self.deferred.clear();
        let result = self.inner.render(match_id, trigger, trigger_args);
        if result.is_err() {
            self.deferred.clear();
        }
        result
    }
}

/// Writes the pending text right after the expansion has been injected.
pub struct DeferredClipboardDispatcher<'a> {
    inner: &'a dyn Dispatcher,
    deferred: &'a DeferredClipboard,
    clipboard: &'a dyn Clipboard,
    params_provider: &'a dyn ClipboardParamsProvider,
}

impl<'a> DeferredClipboardDispatcher<'a> {
    pub fn new(
        inner: &'a dyn Dispatcher,
        deferred: &'a DeferredClipboard,
        clipboard: &'a dyn Clipboard,
        params_provider: &'a dyn ClipboardParamsProvider,
    ) -> Self {
        Self {
            inner,
            deferred,
            clipboard,
            params_provider,
        }
    }

    fn flush(&self) {
        if let Some(text) = self.deferred.take() {
            let params = self.params_provider.get();
            if !params.restore_clipboard {
                // Without the restore guard nothing waited for the target application
                // to consume a pasted expansion, so give it the same grace period.
                std::thread::sleep(std::time::Duration::from_millis(
                    params.restore_clipboard_delay as u64,
                ));
            }

            debug!("setting clipboard from set_clipboard variable");
            if let Err(error) = self.clipboard.set_text(
                &text,
                &ClipboardOperationOptions {
                    use_xclip_backend: params.x11_use_xclip_backend,
                },
            ) {
                error!("unable to set clipboard content (set_clipboard variable): {error}");
            }
        }
    }
}

impl Dispatcher for DeferredClipboardDispatcher<'_> {
    fn dispatch(&self, event: Event) {
        let is_expansion_injection = matches!(
            event.etype,
            EventType::TextInject(_)
                | EventType::HtmlInject(_)
                | EventType::MarkdownInject(_)
                | EventType::ImageInject(_)
        );

        self.inner.dispatch(event);

        if is_expansion_injection {
            self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, path::Path};

    use espanso_engine::event::effect::TextInjectRequest;

    use super::*;
    use crate::cli::worker::engine::dispatch::executor::clipboard_injector::ClipboardParams;

    #[derive(Default)]
    struct MockClipboard {
        log: RefCell<Vec<String>>,
        fail: bool,
    }

    impl Clipboard for MockClipboard {
        fn get_text(&self, _: &ClipboardOperationOptions) -> Option<String> {
            None
        }
        fn set_text(&self, text: &str, _: &ClipboardOperationOptions) -> Result<()> {
            if self.fail {
                anyhow::bail!("no clipboard");
            }
            self.log.borrow_mut().push(format!("clipboard:{text}"));
            Ok(())
        }
        fn set_image(&self, _: &Path, _: &ClipboardOperationOptions) -> Result<()> {
            Ok(())
        }
        fn set_html(&self, _: &str, _: Option<&str>, _: &ClipboardOperationOptions) -> Result<()> {
            Ok(())
        }
    }

    struct MockParams;
    impl ClipboardParamsProvider for MockParams {
        fn get(&self) -> ClipboardParams {
            ClipboardParams {
                pre_paste_delay: 0,
                paste_shortcut_event_delay: 0,
                paste_shortcut: None,
                disable_x11_fast_inject: false,
                restore_clipboard: true,
                restore_clipboard_delay: 0,
                x11_use_xclip_backend: false,
                x11_use_xdotool_backend: false,
            }
        }
    }

    struct LoggingDispatcher<'a>(&'a RefCell<Vec<String>>);
    impl Dispatcher for LoggingDispatcher<'_> {
        fn dispatch(&self, event: Event) {
            let name = match event.etype {
                EventType::TextInject(r) => format!("inject:{}", r.text),
                EventType::MatchInjected => "matchinjected".to_string(),
                _ => "other".to_string(),
            };
            self.0.borrow_mut().push(name);
        }
    }

    struct MockRenderer<'a> {
        deferred: &'a DeferredClipboard,
        set: Option<&'static str>,
        fail: bool,
    }
    impl<'a> Renderer<'a> for MockRenderer<'a> {
        fn render(&'a self, _: i32, _: Option<&str>, _: HashMap<String, String>) -> Result<String> {
            if let Some(text) = self.set {
                self.deferred.set_text(text)?;
            }
            if self.fail {
                anyhow::bail!("boom");
            }
            Ok("body".to_string())
        }
    }

    fn text_inject() -> Event {
        Event::caused_by(
            0,
            EventType::TextInject(TextInjectRequest {
                text: "body".to_string(),
                ..Default::default()
            }),
        )
    }

    #[test]
    fn writes_after_injection_and_only_once() {
        let deferred = DeferredClipboard::new();
        let clipboard = MockClipboard::default();
        let dispatched = RefCell::new(Vec::new());
        let inner = LoggingDispatcher(&dispatched);
        let dispatcher =
            DeferredClipboardDispatcher::new(&inner, &deferred, &clipboard, &MockParams);

        deferred.set_text("first").unwrap();
        deferred.set_text("last").unwrap();

        dispatcher.dispatch(Event::caused_by(0, EventType::MatchInjected));
        assert!(clipboard.log.borrow().is_empty());

        dispatcher.dispatch(text_inject());
        assert_eq!(*clipboard.log.borrow(), vec!["clipboard:last".to_string()]);

        dispatcher.dispatch(text_inject());
        assert_eq!(clipboard.log.borrow().len(), 1);
        assert_eq!(dispatched.borrow().len(), 3);
    }

    #[test]
    fn backend_error_does_not_panic() {
        let deferred = DeferredClipboard::new();
        let clipboard = MockClipboard {
            fail: true,
            ..Default::default()
        };
        let dispatched = RefCell::new(Vec::new());
        let inner = LoggingDispatcher(&dispatched);
        let dispatcher =
            DeferredClipboardDispatcher::new(&inner, &deferred, &clipboard, &MockParams);
        deferred.set_text("x").unwrap();
        dispatcher.dispatch(text_inject());
        assert!(deferred.take().is_none());
    }

    #[test]
    fn failed_render_discards_pending_text() {
        let deferred = DeferredClipboard::new();
        let inner = MockRenderer {
            deferred: &deferred,
            set: Some("lost"),
            fail: true,
        };
        let renderer = DeferredClipboardRenderer::new(&inner, &deferred);
        assert!(renderer.render(1, None, HashMap::new()).is_err());
        assert!(deferred.take().is_none());
    }

    #[test]
    fn successful_render_keeps_pending_and_next_render_resets_it() {
        let deferred = DeferredClipboard::new();
        let with = MockRenderer {
            deferred: &deferred,
            set: Some("kept"),
            fail: false,
        };
        let without = MockRenderer {
            deferred: &deferred,
            set: None,
            fail: false,
        };
        let renderer_with = DeferredClipboardRenderer::new(&with, &deferred);
        let renderer_without = DeferredClipboardRenderer::new(&without, &deferred);

        renderer_with.render(1, None, HashMap::new()).unwrap();
        assert_eq!(deferred.pending.borrow().as_deref(), Some("kept"));

        renderer_without.render(2, None, HashMap::new()).unwrap();
        assert!(deferred.take().is_none());
    }
}
