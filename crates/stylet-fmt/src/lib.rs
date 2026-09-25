//! Formatter for stylet.
//!
//! One statement per line, `{` on the selector line, `prop: value`, at most one
//! blank line between statements (kept from the source), comments preserved.
//! Line breaks inside selectors, values and preludes are kept as continuation
//! lines. Semicolons are removed.

mod sort;

use stylet_syntax::SyntaxKind::*;
use stylet_syntax::{ErrorKind, SyntaxElement, SyntaxError, SyntaxNode, SyntaxToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Spaces(u8),
    Tabs,
}

impl Default for Indent {
    fn default() -> Self {
        Self::Spaces(2)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub indent: Indent,
    /// Sort runs of declarations (see the `sort` module for the order).
    pub sort_properties: bool,
}

/// Formats `src`. Fails with the syntax errors if there are any besides semicolons.
pub fn format(src: &str, options: &Options) -> Result<String, Vec<SyntaxError>> {
    let parse = stylet_syntax::parse(src);
    let errors: Vec<_> = parse
        .errors()
        .iter()
        .filter(|e| e.kind() != ErrorKind::Semicolon)
        .cloned()
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut printer = Printer {
        options,
        out: String::new(),
    };
    printer.items(&parse.syntax(), 0, false);
    let mut out = printer.out.trim_start_matches('\n').to_string();
    out.truncate(out.trim_end().len());
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// A statement or comment in a block, with the comments trailing it on its line.
struct Entry {
    element: SyntaxElement,
    trailing: Vec<SyntaxToken>,
    blank_before: bool,
}

impl Entry {
    fn declaration_property(&self) -> Option<String> {
        let node = self.element.as_node().filter(|n| n.kind() == DECLARATION)?;
        let property = node.children().find(|n| n.kind() == PROPERTY)?;
        Some(property.text().to_string())
    }
}

struct Printer<'a> {
    options: &'a Options,
    out: String,
}

impl Printer<'_> {
    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            match self.options.indent {
                Indent::Spaces(n) => self.out.extend(std::iter::repeat_n(' ', n as usize)),
                Indent::Tabs => self.out.push('\t'),
            }
        }
    }

    fn newline(&mut self, depth: usize) {
        self.out
            .truncate(self.out.trim_end_matches([' ', '\t']).len());
        self.out.push('\n');
        self.indent(depth);
    }

    /// Items of the root or a block. `in_block`: the `{` line is still open, so
    /// a comment before the first line break trails it.
    fn items(&mut self, parent: &SyntaxNode, depth: usize, in_block: bool) {
        let mut entries = self.entries(parent, in_block);
        if self.options.sort_properties {
            sort_declarations(&mut entries);
        }
        for (i, entry) in entries.iter().enumerate() {
            if entry.blank_before && i > 0 {
                self.out.push('\n');
            }
            self.newline(depth);
            match &entry.element {
                SyntaxElement::Token(comment) => self.out += comment.text(),
                SyntaxElement::Node(node) => self.statement(node, depth),
            }
            for comment in &entry.trailing {
                self.out.push(' ');
                self.out += comment.text();
            }
        }
    }

    fn entries(&mut self, parent: &SyntaxNode, in_block: bool) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut newlines = 0;
        let mut on_open_line = in_block;
        for element in parent.children_with_tokens() {
            match element.kind() {
                NEWLINE => {
                    newlines += 1;
                    on_open_line = false;
                }
                WHITESPACE | L_BRACE | R_BRACE | EOF | ERROR => {}
                LINE_COMMENT | BLOCK_COMMENT => {
                    let comment = element.as_token().expect("comment is a token").clone();
                    let multiline = comment.text().contains('\n');
                    if on_open_line && !multiline {
                        self.out.push(' ');
                        self.out += comment.text();
                    } else if newlines == 0
                        && !multiline
                        && let Some(last) = entries.last_mut()
                    {
                        last.trailing.push(comment);
                    } else {
                        entries.push(Entry {
                            element: SyntaxElement::Token(comment),
                            trailing: Vec::new(),
                            blank_before: newlines > 1,
                        });
                        newlines = 0;
                    }
                }
                _ => {
                    entries.push(Entry {
                        element,
                        trailing: Vec::new(),
                        blank_before: newlines > 1,
                    });
                    newlines = 0;
                    on_open_line = false;
                }
            }
        }
        entries
    }

    fn statement(&mut self, node: &SyntaxNode, depth: usize) {
        for element in node.children_with_tokens() {
            match element {
                SyntaxElement::Node(child) => match child.kind() {
                    SELECTOR => self.tokens(&child, depth),
                    PROPERTY => self.out += &child.text().to_string(),
                    VALUE | PRELUDE => {
                        if !self.out.ends_with(' ') {
                            self.out.push(' ');
                        }
                        if node.kind() == EXTEND {
                            self.extend_targets(&child);
                        } else {
                            self.tokens(&child, depth + 1);
                        }
                    }
                    BLOCK => self.block(&child, depth),
                    _ => {}
                },
                SyntaxElement::Token(token) => match token.kind() {
                    AT_KEYWORD if node.kind() == EXTEND => self.out += "@extend",
                    AT_KEYWORD | PLACEHOLDER_NAME => self.out += token.text(),
                    COLON => self.out.push(':'),
                    LINE_COMMENT | BLOCK_COMMENT => {
                        self.space();
                        self.out += token.text();
                        if token.kind() == LINE_COMMENT {
                            self.newline(depth);
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    fn space(&mut self) {
        if !self.out.ends_with([' ', '\n', '\t']) {
            self.out.push(' ');
        }
    }

    fn block(&mut self, block: &SyntaxNode, depth: usize) {
        self.space();
        self.out.push('{');
        let before = self.out.len();
        self.items(block, depth + 1, true);
        if self.out.len() == before {
            self.out.push('}');
        } else {
            self.newline(depth);
            self.out.push('}');
        }
    }

    fn extend_targets(&mut self, prelude: &SyntaxNode) {
        let targets: Vec<_> = prelude
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == PLACEHOLDER_NAME)
            .map(|t| t.text().to_string())
            .collect();
        self.out += &targets.join(", ");
    }

    /// Prints the tokens of a selector, value or prelude: whitespace collapses to
    /// one space, line breaks become continuation lines at `continuation` depth,
    /// comments are kept.
    fn tokens(&mut self, node: &SyntaxNode, continuation: usize) {
        let mut pending: Option<Gap> = None;
        for token in node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if token.parent().is_some_and(|p| p.kind() == ERROR) {
                continue;
            }
            let kind = token.kind();
            match kind {
                WHITESPACE => {
                    pending.get_or_insert(Gap::Space);
                }
                NEWLINE => pending = Some(Gap::Newline),
                LINE_COMMENT | BLOCK_COMMENT => {
                    self.gap(pending.take().unwrap_or(Gap::Space), continuation);
                    self.out += token.text();
                    pending = (kind == LINE_COMMENT).then_some(Gap::Newline);
                }
                _ => {
                    if let Some(gap) = pending.take() {
                        let closing = matches!(kind, R_PAREN | R_BRACK);
                        self.gap(
                            gap,
                            if closing {
                                continuation.saturating_sub(1)
                            } else {
                                continuation
                            },
                        );
                    }
                    self.out += token.text();
                }
            }
        }
    }

    fn gap(&mut self, gap: Gap, continuation: usize) {
        match gap {
            Gap::Space => self.space(),
            Gap::Newline => self.newline(continuation),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Gap {
    Space,
    Newline,
}

/// Sorts each run of adjacent declarations; blank lines and other statements end a run.
fn sort_declarations(entries: &mut [Entry]) {
    let mut start = 0;
    while start < entries.len() {
        let mut end = start;
        while end < entries.len()
            && entries[end].declaration_property().is_some()
            && (end == start || !entries[end].blank_before)
        {
            end += 1;
        }
        if end - start > 1 {
            let blank = entries[start].blank_before;
            let run = &mut entries[start..end];
            run.sort_by_cached_key(|e| sort::key(&e.declaration_property().unwrap_or_default()));
            for (i, entry) in run.iter_mut().enumerate() {
                entry.blank_before = i == 0 && blank;
            }
        }
        start = end.max(start + 1);
    }
}
