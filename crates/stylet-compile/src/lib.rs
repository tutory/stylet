//! Compiles stylet to CSS.
//!
//! Nesting is emitted as native CSS nesting; values, selectors and unknown
//! at-rules pass through unchanged apart from whitespace and comments.

mod custom_media;
mod emit;
mod text;

use stylet_syntax::TextRange;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Strip whitespace and comments (except `/*! … */`).
    pub minify: bool,
    /// Substitute `(--name)` media conditions and drop `@custom-media` definitions.
    pub resolve_custom_media: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub range: TextRange,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, range: TextRange) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            range,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub css: String,
    /// Syntax and compile diagnostics, ordered by position.
    pub diagnostics: Vec<Diagnostic>,
}

impl Output {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Compiles a single source string. `@import` is not available.
pub fn compile_str(src: &str, options: &Options) -> Output {
    let parse = stylet_syntax::parse(src);
    let mut diagnostics: Vec<_> = parse
        .errors()
        .iter()
        .map(|e| Diagnostic::error(e.message(), e.range()))
        .collect();

    let root = parse.syntax();
    let custom_media = options.resolve_custom_media.then(|| {
        let mut errors = Vec::new();
        let media = custom_media::collect(&root, &mut errors);
        diagnostics.extend(
            errors
                .into_iter()
                .map(|(range, message)| Diagnostic::error(message, range)),
        );
        media
    });

    let mut emitter = emit::Emitter::new(options, custom_media);
    emitter.root(&root);
    let (css, compile_diagnostics) = emitter.finish();
    diagnostics.extend(compile_diagnostics);
    diagnostics.sort_by_key(|d| d.range.start());
    Output { css, diagnostics }
}
