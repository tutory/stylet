//! Lexer and lossless parser for stylet.
//!
//! [`parse`] never fails: it always returns a tree covering the whole input, plus
//! any syntax errors. The tree is lossless, so `parse(src).syntax().to_string() == src`.

pub mod ast;
mod kind;
mod lexer;
mod parser;

pub use kind::{Stylet, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use rowan::{TextRange, TextSize};

use rowan::GreenNode;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A `;`. Harmless: the formatter removes it.
    Semicolon,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    message: String,
    range: TextRange,
    kind: ErrorKind,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>, range: TextRange) -> Self {
        Self {
            message: message.into(),
            range,
            kind: ErrorKind::Other,
        }
    }

    pub fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn range(&self) -> TextRange {
        self.range
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// Result of [`parse`].
#[derive(Debug, Clone)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn root(&self) -> ast::Root {
        ast::Root::cast(self.syntax()).expect("parser always produces a ROOT node")
    }

    /// Syntax errors, ordered by position.
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// Tree dump for tests and debugging.
    pub fn debug_tree(&self) -> String {
        let mut out = format!("{:#?}", self.syntax());
        out.truncate(out.trim_end().len());
        out.push('\n');
        for e in &self.errors {
            out += &format!("error {:?}: {}\n", e.range, e.message);
        }
        out
    }
}

pub fn parse(src: &str) -> Parse {
    let (tokens, errors) = lexer::tokenize(src);
    let (green, errors) = parser::parse(src, tokens, errors);
    Parse { green, errors }
}
