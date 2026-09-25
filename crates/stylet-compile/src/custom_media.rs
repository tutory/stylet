//! Compile-time resolution of `@custom-media --name <query>` references `(--name)`.

use crate::text::{Piece, Pieces, pieces};
use std::collections::HashMap;
use stylet_syntax::SyntaxKind::*;
use stylet_syntax::ast::AtRule;

#[derive(Debug, Default)]
pub struct CustomMedia {
    raw: HashMap<String, Pieces>,
}

impl CustomMedia {
    /// Records a definition, returning an error message for malformed ones.
    pub fn define(&mut self, rule: &AtRule) -> Result<(), String> {
        let prelude = rule
            .prelude()
            .map(|p| pieces(p.syntax()))
            .unwrap_or_default();
        let (name, query) = match prelude.split_first() {
            Some((Some(name), rest)) if name.kind == IDENT && name.text.starts_with("--") => {
                (name.text.clone(), trim(rest))
            }
            _ => return Err("expected `@custom-media --name <media query>`".into()),
        };
        if query.is_empty() {
            return Err(format!("`{name}` needs a media query"));
        }
        self.raw.insert(name, query);
        Ok(())
    }

    /// Replaces every `(--name)` in a media prelude by its definition.
    pub fn expand(&self, prelude: &Pieces) -> Result<Pieces, String> {
        self.expand_with(prelude, &mut Vec::new())
    }

    fn expand_with(&self, prelude: &Pieces, visiting: &mut Vec<String>) -> Result<Pieces, String> {
        let significant = prelude.iter().flatten().count();
        let mut out = Pieces::new();
        let mut i = 0;
        while i < prelude.len() {
            let Some((name, end)) = reference_at(prelude, i) else {
                out.push(prelude[i].clone());
                i += 1;
                continue;
            };
            let raw = self
                .raw
                .get(&name)
                .ok_or_else(|| format!("unknown custom media `{name}`"))?;
            if visiting.contains(&name) {
                return Err(format!("custom media `{name}` references itself"));
            }
            visiting.push(name.clone());
            let def = self.expand_with(raw, visiting)?;
            visiting.pop();

            if significant == 3 {
                out.extend(def);
            } else if is_condition(&def) {
                if is_single_group(&def) {
                    out.extend(def);
                } else {
                    out.push(piece(L_PAREN, "("));
                    out.extend(def);
                    out.push(piece(R_PAREN, ")"));
                }
            } else {
                return Err(format!(
                    "custom media `{name}` contains a media type or list and can't be combined with other conditions"
                ));
            }
            i = end;
        }
        Ok(out)
    }
}

fn piece(kind: stylet_syntax::SyntaxKind, text: &str) -> Option<Piece> {
    Some(Piece {
        kind,
        text: text.into(),
    })
}

fn trim(pieces: &[Option<Piece>]) -> Pieces {
    let start = pieces
        .iter()
        .position(Option::is_some)
        .unwrap_or(pieces.len());
    pieces[start..].to_vec()
}

/// `( --name )` starting at `i`: returns the name and the index after `)`.
fn reference_at(pieces: &Pieces, i: usize) -> Option<(String, usize)> {
    let kind = |j: usize| pieces.get(j).and_then(|p| p.as_ref());
    let skip_space = |j: usize| {
        if matches!(pieces.get(j), Some(None)) {
            j + 1
        } else {
            j
        }
    };
    if kind(i)?.kind != L_PAREN {
        return None;
    }
    let name_at = skip_space(i + 1);
    let name = kind(name_at).filter(|p| p.kind == IDENT && p.text.starts_with("--"))?;
    let close = skip_space(name_at + 1);
    (kind(close)?.kind == R_PAREN).then(|| (name.text.clone(), close + 1))
}

/// A media condition (no media type, no top-level comma), e.g. `(a) and (b)` or `not (a)`.
fn is_condition(def: &Pieces) -> bool {
    let mut significant = def.iter().flatten();
    let starts_ok = match significant.next() {
        Some(p) if p.kind == L_PAREN => true,
        Some(p) if p.kind == IDENT && p.text.eq_ignore_ascii_case("not") => {
            significant.next().is_some_and(|p| p.kind == L_PAREN)
        }
        _ => false,
    };
    let mut depth = 0i32;
    let top_level_comma = def.iter().flatten().any(|p| {
        match p.kind {
            L_PAREN => depth += 1,
            R_PAREN => depth -= 1,
            _ => {}
        }
        p.kind == COMMA && depth == 0
    });
    starts_ok && !top_level_comma
}

/// Whether `def` is exactly one parenthesized group, e.g. `(width <= 600px)`.
fn is_single_group(def: &Pieces) -> bool {
    let significant: Vec<_> = def.iter().flatten().collect();
    if significant.first().is_none_or(|p| p.kind != L_PAREN) {
        return false;
    }
    let mut depth = 0;
    for (i, p) in significant.iter().enumerate() {
        match p.kind {
            L_PAREN => depth += 1,
            R_PAREN => {
                depth -= 1;
                if depth == 0 {
                    return i == significant.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}
