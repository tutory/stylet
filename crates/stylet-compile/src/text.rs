//! Serializes selectors, values and preludes: comments are dropped and
//! whitespace runs collapse to one space (or disappear where safe when minifying).

use stylet_syntax::SyntaxKind::{self, *};
use stylet_syntax::SyntaxNode;

/// A significant token; `None` in a piece list marks a run of trivia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub kind: SyntaxKind,
    pub text: String,
}

pub type Pieces = Vec<Option<Piece>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Selector,
    Value,
    Prelude,
}

impl Context {
    /// Tokens around which whitespace can be removed when minifying.
    fn is_tight(self, kind: SyntaxKind) -> bool {
        match self {
            Self::Selector => matches!(kind, COMMA | GT | PLUS | TILDE),
            Self::Value | Self::Prelude => kind == COMMA,
        }
    }
}

/// Significant tokens of `node`, skipping error nodes (e.g. stray semicolons).
pub fn pieces(node: &SyntaxNode) -> Pieces {
    let mut out = Vec::new();
    for token in node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if token.parent().is_some_and(|p| p.kind() == ERROR) {
            continue;
        }
        if token.kind().is_trivia() {
            if matches!(out.last(), Some(Some(_))) {
                out.push(None);
            }
        } else {
            out.push(Some(Piece {
                kind: token.kind(),
                text: token.text().to_string(),
            }));
        }
    }
    if matches!(out.last(), Some(None)) {
        out.pop();
    }
    out
}

pub fn serialize(node: &SyntaxNode, cx: Context, minify: bool) -> String {
    join(&pieces(node), cx, minify)
}

pub fn join(pieces: &[Option<Piece>], cx: Context, minify: bool) -> String {
    let mut out = String::new();
    let mut prev: Option<SyntaxKind> = None;
    let mut space = false;
    for piece in pieces {
        let Some(piece) = piece else {
            space = true;
            continue;
        };
        if space && prev.is_some() {
            let tight = minify && (cx.is_tight(piece.kind) || prev.is_some_and(|k| cx.is_tight(k)));
            if !tight {
                out.push(' ');
            }
        }
        out += &piece.text;
        prev = Some(piece.kind);
        space = false;
    }
    out
}
