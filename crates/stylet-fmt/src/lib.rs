//! Formatter for stylet.
//!
//! One statement per line, `{` on the selector line, `prop: value`, at most one
//! blank line between statements (kept from the source), comments preserved.
//! Line breaks inside selectors, values and preludes are kept as continuation
//! lines. Semicolons are removed.

mod sort;

use stylet_syntax::SyntaxKind::{self, *};
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

#[derive(Debug, Clone)]
pub struct Options {
    pub indent: Indent,
    /// Sort runs of declarations (see the `sort` module for the order).
    pub sort_properties: bool,
    /// Move nested blocks (rules, placeholders, at-rules with a block) below the
    /// declarations of their block. Comments directly above a block move with
    /// it; `@extend` and `@import` stay first.
    pub nested_blocks_last: bool,
    /// Align continuation lines that start with a string (`grid-template-areas`)
    /// to the first string's quote.
    pub align_strings: bool,
    /// A blank line before and after every block (rules, at-rules with a block).
    pub blank_lines_around_blocks: bool,
    /// A blank line before and after each group of `@import`s.
    pub blank_lines_around_imports: bool,
    /// One selector per line in selector lists.
    pub selector_per_line: bool,
    /// `a, b` after commas, nothing before; no spaces just inside parentheses.
    pub normalize_spacing: bool,
    /// Strings in single quotes (unless they contain one).
    pub single_quotes: bool,
    /// Keep the leading zero of fractions (`0.5`); `false` writes `.5`.
    pub leading_zero: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            indent: Indent::default(),
            sort_properties: true,
            nested_blocks_last: false,
            align_strings: true,
            blank_lines_around_blocks: true,
            blank_lines_around_imports: true,
            selector_per_line: true,
            normalize_spacing: true,
            single_quotes: true,
            leading_zero: false,
        }
    }
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
        // Move blocks first so that sorting sees the final runs of declarations.
        if self.options.nested_blocks_last && in_block {
            entries = nested_blocks_last(entries);
        }
        if self.options.sort_properties {
            sort_declarations(&mut entries);
        }
        self.force_blank_lines(&mut entries);
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

    /// Blank lines around blocks and `@import` groups. A block's blank line goes
    /// above the comments directly attached to it.
    fn force_blank_lines(&self, entries: &mut [Entry]) {
        let kind = |e: &Entry| e.element.as_node().map(|n| n.kind());
        let is_block = |e: &Entry| {
            e.element.as_node().is_some_and(|n| match n.kind() {
                RULE | PLACEHOLDER => true,
                AT_RULE => n.children().any(|c| c.kind() == BLOCK),
                _ => false,
            })
        };
        let is_import = |e: &Entry| kind(e) == Some(IMPORT);
        let mut blank = vec![false; entries.len()];
        for i in 0..entries.len() {
            let grouped = (self.options.blank_lines_around_blocks && is_block(&entries[i]))
                || (self.options.blank_lines_around_imports
                    && is_import(&entries[i])
                    && !entries.get(i.wrapping_sub(1)).is_some_and(is_import));
            if grouped {
                // Start of the block including comments directly above it.
                let mut start = i;
                while start > 0
                    && entries[start - 1].element.as_token().is_some()
                    && !entries[start].blank_before
                {
                    start -= 1;
                }
                blank[start] = true;
            }
            let ends_group = (self.options.blank_lines_around_blocks && is_block(&entries[i]))
                || (self.options.blank_lines_around_imports
                    && is_import(&entries[i])
                    && !entries.get(i + 1).is_some_and(is_import));
            if ends_group && i + 1 < entries.len() {
                blank[i + 1] = true;
            }
        }
        for (entry, blank) in entries.iter_mut().zip(blank) {
            entry.blank_before |= blank;
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
                    SELECTOR => self.tokens(&child, depth, Cx::Selector),
                    PROPERTY => self.out += &child.text().to_string(),
                    VALUE | PRELUDE => {
                        if !self.out.ends_with(' ') {
                            self.out.push(' ');
                        }
                        if node.kind() == EXTEND {
                            self.extend_targets(&child);
                        } else {
                            let cx = if child.kind() == VALUE {
                                Cx::Value
                            } else {
                                Cx::Prelude
                            };
                            self.tokens(&child, depth + 1, cx);
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
    /// comments are kept. With `align`, continuation lines starting with a
    /// string are aligned to the first string of the value.
    fn tokens(&mut self, node: &SyntaxNode, continuation: usize, cx: Cx) {
        let align = cx == Cx::Value && self.options.align_strings;
        let spacing = self.options.normalize_spacing;
        let mut pending: Option<Gap> = None;
        let mut prev: Option<SyntaxKind> = None;
        let mut parens = 0usize;
        // Whitespace up to the first string's column on its line.
        let mut string_column: Option<String> = None;
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
                    if spacing {
                        // No space before `,` or `)` and after `(`; one after `,`.
                        let tight = matches!(kind, COMMA | R_PAREN) || prev == Some(L_PAREN);
                        match pending {
                            Some(Gap::Space) if tight => pending = None,
                            None if prev == Some(COMMA) && !tight => pending = Some(Gap::Space),
                            _ => {}
                        }
                    }
                    if cx == Cx::Selector
                        && self.options.selector_per_line
                        && prev == Some(COMMA)
                        && parens == 0
                    {
                        pending = Some(Gap::Newline);
                    }
                    match (pending.take(), &string_column) {
                        (Some(Gap::Newline), Some(column)) if align && kind == STRING => {
                            let column = column.clone();
                            self.out
                                .truncate(self.out.trim_end_matches([' ', '\t']).len());
                            self.out.push('\n');
                            self.out += &column;
                        }
                        (Some(gap), _) => {
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
                        (None, _) => {}
                    }
                    if align && kind == STRING && string_column.is_none() {
                        let line = &self.out[self.out.rfind('\n').map_or(0, |i| i + 1)..];
                        let column: String = line
                            .chars()
                            .map(|c| if c == '\t' { '\t' } else { ' ' })
                            .collect();
                        string_column = Some(column);
                    }
                    match kind {
                        L_PAREN => parens += 1,
                        R_PAREN => parens = parens.saturating_sub(1),
                        _ => {}
                    }
                    let text = token.text();
                    match kind {
                        STRING if self.options.single_quotes => self.out += &single_quoted(text),
                        NUMBER if !self.options.leading_zero => {
                            self.out += &without_leading_zero(text)
                        }
                        _ => self.out += text,
                    }
                    prev = Some(kind);
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

/// What a token run belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cx {
    Selector,
    Value,
    Prelude,
}

/// `"x"` → `'x'` unless the content has a `'`.
fn single_quoted(text: &str) -> String {
    let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return text.to_string();
    };
    if inner.contains('\'') || inner.ends_with('\\') {
        return text.to_string();
    }
    format!("'{}'", inner.replace("\\\"", "\""))
}

/// `0.5em` → `.5em`, `-0.5` → `-.5`.
fn without_leading_zero(text: &str) -> String {
    let (sign, rest) = match text.strip_prefix(['-', '+']) {
        Some(rest) => (&text[..1], rest),
        None => ("", text),
    };
    match rest.strip_prefix("0.") {
        Some(fraction) if fraction.starts_with(|c: char| c.is_ascii_digit()) => {
            format!("{sign}.{fraction}")
        }
        _ => text.to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
enum Gap {
    Space,
    Newline,
}

/// Stable partition: `@extend`/`@import` first, then declarations and the
/// rest, then nested blocks (with the comments directly above them).
fn nested_blocks_last(entries: Vec<Entry>) -> Vec<Entry> {
    let is_block = |e: &Entry| {
        e.element.as_node().is_some_and(|n| match n.kind() {
            RULE | PLACEHOLDER => true,
            AT_RULE => n.children().any(|c| c.kind() == BLOCK),
            _ => false,
        })
    };
    let is_head = |e: &Entry| {
        e.element
            .as_node()
            .is_some_and(|n| matches!(n.kind(), EXTEND | IMPORT))
    };
    let (mut head, mut blocks, mut rest) = (Vec::new(), Vec::new(), Vec::new());
    let mut comments: Vec<Entry> = Vec::new();
    for entry in entries {
        if entry.element.as_token().is_some() {
            if entry.blank_before {
                rest.append(&mut comments);
            }
            comments.push(entry);
            continue;
        }
        let target = if is_head(&entry) {
            &mut head
        } else if is_block(&entry) && !entry.blank_before {
            &mut blocks
        } else if is_block(&entry) {
            rest.append(&mut comments);
            &mut blocks
        } else {
            &mut rest
        };
        target.append(&mut comments);
        target.push(entry);
    }
    rest.append(&mut comments);
    if !blocks.is_empty() && !rest.is_empty() {
        blocks[0].blank_before = true;
    }
    let mut out = head;
    out.append(&mut rest);
    out.append(&mut blocks);
    if let Some(first) = out.first_mut() {
        first.blank_before = false;
    }
    out
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
            let keys: Vec<_> = run
                .iter()
                .map(|e| sort::key(&e.declaration_property().unwrap_or_default()))
                .collect();
            // A shorthand after its longhand overrides it; keep such runs as they are.
            let unsafe_order = (0..keys.len())
                .any(|i| (i + 1..keys.len()).any(|j| sort::overrides(&keys[i], &keys[j])));
            if unsafe_order {
                start = end;
                continue;
            }
            run.sort_by_cached_key(|e| sort::key(&e.declaration_property().unwrap_or_default()));
            for (i, entry) in run.iter_mut().enumerate() {
                entry.blank_before = i == 0 && blank;
            }
        }
        start = end.max(start + 1);
    }
}
