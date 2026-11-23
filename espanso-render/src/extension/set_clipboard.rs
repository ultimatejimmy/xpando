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

use crate::{Extension, ExtensionOutput, ExtensionResult, Number, Params, Value};
use thiserror::Error;

/// Destination of the `set_clipboard` variable. Mirrors `ClipboardProvider`
/// (the read side used by the `clipboard` variable) so the extension stays
/// unit-testable without a real clipboard.
///
/// The implementation decides *when* the text actually reaches the clipboard:
/// the real one defers the write until the expansion has been injected.
pub trait ClipboardWriter {
    fn set_text(&self, text: &str) -> anyhow::Result<()>;
}

pub struct SetClipboardExtension<'a> {
    writer: &'a dyn ClipboardWriter,
}

impl<'a> SetClipboardExtension<'a> {
    pub fn new(writer: &'a dyn ClipboardWriter) -> Self {
        Self { writer }
    }
}

impl Extension for SetClipboardExtension<'_> {
    fn name(&self) -> &'static str {
        "set_clipboard"
    }

    fn calculate(
        &self,
        _: &crate::Context,
        _: &crate::Scope,
        params: &Params,
    ) -> crate::ExtensionResult {
        // Same scalar coercion as the echo extension: YAML scalars arrive unquoted
        let text = match params.get("text") {
            Some(Value::String(value)) => value.clone(),
            Some(Value::Number(Number::Integer(value))) => value.to_string(),
            Some(Value::Number(Number::Float(value))) => value.to_string(),
            Some(Value::Bool(value)) => value.to_string(),
            _ => {
                return ExtensionResult::Error(
                    SetClipboardExtensionError::MissingTextParameter.into(),
                )
            }
        };

        match self.writer.set_text(&text) {
            Ok(()) => ExtensionResult::Success(ExtensionOutput::Single(String::new())),
            Err(err) => ExtensionResult::Error(err),
        }
    }
}

#[derive(Error, Debug)]
pub enum SetClipboardExtensionError {
    #[error("missing 'text' parameter")]
    MissingTextParameter,
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap};

    use super::*;
    use crate::{Context, RenderOptions, RenderResult, Renderer, Template, Variable};

    #[derive(Default)]
    struct MockClipboardWriter {
        fail: bool,
        written: RefCell<Vec<String>>,
    }

    impl ClipboardWriter for MockClipboardWriter {
        fn set_text(&self, text: &str) -> anyhow::Result<()> {
            if self.fail {
                return Err(anyhow::anyhow!("clipboard unavailable"));
            }
            self.written.borrow_mut().push(text.to_string());
            Ok(())
        }
    }

    fn run(writer: &MockClipboardWriter, params: &[(&str, Value)]) -> ExtensionResult {
        let params = params
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect::<Params>();
        SetClipboardExtension::new(writer).calculate(
            &Context::default(),
            &HashMap::default(),
            &params,
        )
    }

    fn text(value: &str) -> Value {
        Value::String(value.to_string())
    }

    #[test]
    fn writes_text_and_returns_empty_output() {
        let writer = MockClipboardWriter::default();
        let result = run(&writer, &[("text", text("hello"))]);
        assert_eq!(
            result.into_success().unwrap(),
            ExtensionOutput::Single(String::new())
        );
        assert_eq!(*writer.written.borrow(), vec!["hello".to_string()]);
    }

    #[test]
    fn empty_text_is_written() {
        let writer = MockClipboardWriter::default();
        assert!(run(&writer, &[("text", text(""))]).into_success().is_ok());
        assert_eq!(*writer.written.borrow(), vec![String::new()]);
    }

    #[test]
    fn multiline_text_is_preserved() {
        let writer = MockClipboardWriter::default();
        run(&writer, &[("text", text("a\r\nb\n\nc"))])
            .into_success()
            .unwrap();
        assert_eq!(*writer.written.borrow(), vec!["a\r\nb\n\nc".to_string()]);
    }

    #[test]
    fn scalars_are_coerced() {
        let writer = MockClipboardWriter::default();
        run(&writer, &[("text", Value::Number(Number::Integer(42)))])
            .into_success()
            .unwrap();
        run(&writer, &[("text", Value::Bool(true))])
            .into_success()
            .unwrap();
        assert_eq!(
            *writer.written.borrow(),
            vec!["42".to_string(), "true".to_string()]
        );
    }

    #[test]
    fn missing_text_param_is_an_error_and_writes_nothing() {
        let writer = MockClipboardWriter::default();
        assert!(matches!(run(&writer, &[]), ExtensionResult::Error(_)));
        assert!(writer.written.borrow().is_empty());
    }

    #[test]
    fn invalid_text_param_type_is_an_error() {
        let writer = MockClipboardWriter::default();
        assert!(matches!(
            run(&writer, &[("text", Value::Array(vec![]))]),
            ExtensionResult::Error(_)
        ));
        assert!(matches!(
            run(&writer, &[("text", Value::Null)]),
            ExtensionResult::Error(_)
        ));
        assert!(writer.written.borrow().is_empty());
    }

    #[test]
    fn writer_error_propagates() {
        let writer = MockClipboardWriter {
            fail: true,
            ..Default::default()
        };
        assert!(matches!(
            run(&writer, &[("text", text("x"))]),
            ExtensionResult::Error(_)
        ));
    }

    fn var(name: &str, var_type: &str, key: &str, value: &str, inject: bool) -> Variable {
        Variable {
            name: name.to_string(),
            var_type: var_type.to_string(),
            params: Params::from_iter([(key.to_string(), text(value))]),
            inject_vars: inject,
            ..Default::default()
        }
    }

    #[test]
    fn renders_template_with_out_and_set_clipboard() {
        let writer = MockClipboardWriter::default();
        let set_clipboard = SetClipboardExtension::new(&writer);
        let echo = crate::extension::echo::EchoExtension::new();
        let renderer = crate::create(vec![&echo, &set_clipboard]);

        let template = Template {
            ids: vec!["tdc".to_string()],
            body: "{{out}}".to_string(),
            vars: vec![
                var("d", "echo", "echo", "2026-11-23", false),
                var("out", "echo", "echo", "{{d}}_x", true),
                var("sc", "set_clipboard", "text", "{{out}}", true),
            ],
        };

        let res = renderer.render(&template, &Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(s) if s == "2026-11-23_x"));
        assert_eq!(*writer.written.borrow(), vec!["2026-11-23_x".to_string()]);
    }

    #[test]
    fn render_fails_and_writes_nothing_on_missing_param() {
        let writer = MockClipboardWriter::default();
        let set_clipboard = SetClipboardExtension::new(&writer);
        let renderer = crate::create(vec![&set_clipboard]);
        let template = Template {
            ids: vec!["t".to_string()],
            body: "{{sc}}".to_string(),
            vars: vec![Variable {
                name: "sc".to_string(),
                var_type: "set_clipboard".to_string(),
                ..Default::default()
            }],
        };
        let res = renderer.render(&template, &Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Error(_)));
        assert!(writer.written.borrow().is_empty());
    }
}
