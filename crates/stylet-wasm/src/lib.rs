//! WebAssembly bindings for the stylet playground.
//!
//! Only single sources are supported: there is no file system, so `@import`
//! reports an error.

use serde::{Deserialize, Serialize};
use stylet_compile::{Options, Severity, compile_str};
use stylet_resolve::LineIndex;
use stylet_syntax::TextRange;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct Diagnostic {
    severity: &'static str,
    message: String,
    /// Byte offsets into the source.
    start: u32,
    end: u32,
    /// 1-based line and column (UTF-16).
    line: u32,
    column: u32,
}

impl Diagnostic {
    fn new(index: &LineIndex, severity: &'static str, message: String, range: TextRange) -> Self {
        let at = index.line_col(range.start());
        Self {
            severity,
            message,
            start: range.start().into(),
            end: range.end().into(),
            line: at.line + 1,
            column: at.col + 1,
        }
    }
}

#[derive(Serialize)]
struct CompileResult {
    css: String,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
struct FormatResult {
    /// `None` when the source has syntax errors.
    code: Option<String>,
    diagnostics: Vec<Diagnostic>,
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    serde_wasm_bindgen::to_value(value).unwrap_or(JsValue::NULL)
}

/// Compiles `src` to CSS: `{ css, diagnostics: [{ severity, message, start, end, line, column }] }`.
#[wasm_bindgen]
pub fn compile(src: &str, minify: bool, resolve_custom_media: bool) -> JsValue {
    let options = Options {
        minify,
        resolve_custom_media,
        ..Options::default()
    };
    let out = compile_str(src, &options);
    let index = LineIndex::new(src);
    let diagnostics = out
        .diagnostics
        .into_iter()
        .map(|d| {
            let severity = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let is_import = src
                .get(usize::from(d.range.start())..)
                .is_some_and(|rest| rest.starts_with("@import"));
            let message = if is_import && d.message.starts_with("can't find") {
                "`@import` isn't available in the playground".to_string()
            } else {
                d.message
            };
            Diagnostic::new(&index, severity, message, d.range)
        })
        .collect();
    to_js(&CompileResult {
        css: out.css,
        diagnostics,
    })
}

/// Formatter options; missing fields use the defaults. `indent` is the number
/// of spaces, 0 for tabs.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct FormatOptions {
    indent: Option<u8>,
    sort_properties: Option<bool>,
    nested_blocks_last: Option<bool>,
    align_strings: Option<bool>,
    blank_lines_around_blocks: Option<bool>,
    blank_lines_around_imports: Option<bool>,
    selector_per_line: Option<bool>,
    normalize_spacing: Option<bool>,
    single_quotes: Option<bool>,
    leading_zero: Option<bool>,
}

/// Formats `src`: `{ code, diagnostics }`. `options` is an object like
/// `{ sortProperties: false, nestedBlocksLast: true }`.
#[wasm_bindgen]
pub fn format(src: &str, options: JsValue) -> JsValue {
    let o: FormatOptions = serde_wasm_bindgen::from_value(options).unwrap_or_default();
    let d = stylet_fmt::Options::default();
    let options = stylet_fmt::Options {
        indent: match o.indent {
            None => d.indent,
            Some(0) => stylet_fmt::Indent::Tabs,
            Some(n) => stylet_fmt::Indent::Spaces(n),
        },
        sort_properties: o.sort_properties.unwrap_or(d.sort_properties),
        nested_blocks_last: o.nested_blocks_last.unwrap_or(d.nested_blocks_last),
        align_strings: o.align_strings.unwrap_or(d.align_strings),
        blank_lines_around_blocks: o
            .blank_lines_around_blocks
            .unwrap_or(d.blank_lines_around_blocks),
        blank_lines_around_imports: o
            .blank_lines_around_imports
            .unwrap_or(d.blank_lines_around_imports),
        selector_per_line: o.selector_per_line.unwrap_or(d.selector_per_line),
        normalize_spacing: o.normalize_spacing.unwrap_or(d.normalize_spacing),
        single_quotes: o.single_quotes.unwrap_or(d.single_quotes),
        leading_zero: o.leading_zero.unwrap_or(d.leading_zero),
    };
    let index = LineIndex::new(src);
    let result = match stylet_fmt::format(src, &options) {
        Ok(code) => FormatResult {
            code: Some(code),
            diagnostics: Vec::new(),
        },
        Err(errors) => FormatResult {
            code: None,
            diagnostics: errors
                .into_iter()
                .map(|e| Diagnostic::new(&index, "error", e.message().to_string(), e.range()))
                .collect(),
        },
    };
    to_js(&result)
}

/// The stylet version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
