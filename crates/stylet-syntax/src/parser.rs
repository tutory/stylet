//! Statement-oriented parser producing a lossless rowan tree.
//!
//! A statement ends at a newline, unless the value continues (open brackets, a
//! trailing comma, a `{` on the next line, or a more indented string line), or
//! at a `{` / `}` on the same line.

use crate::SyntaxError;
use crate::SyntaxKind::{self, *};
use crate::lexer::Token;
use rowan::{GreenNode, GreenNodeBuilder, TextRange};

/// At-rules whose prelude always runs until the opening `{`, across lines.
const BLOCK_AT_RULES: &[&str] = &[
    "media",
    "supports",
    "container",
    "scope",
    "starting-style",
    "font-face",
    "keyframes",
    "page",
    "counter-style",
    "property",
    "font-feature-values",
    "document",
];

pub fn parse(
    src: &str,
    tokens: Vec<Token>,
    errors: Vec<SyntaxError>,
) -> (GreenNode, Vec<SyntaxError>) {
    let mut p = Parser {
        src,
        tokens,
        pos: 0,
        builder: GreenNodeBuilder::new(),
        errors,
    };
    p.builder.start_node(ROOT.into());
    p.items(0);
    p.bump(); // EOF
    p.builder.finish_node();
    let mut errors = p.errors;
    errors.sort_by_key(|e| e.range().start());
    (p.builder.finish(), errors)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminator {
    Newline,
    /// A `{` at the given token index.
    LBrace(usize),
    RBrace,
    Eof,
}

/// Extent of one statement, excluding trailing trivia.
struct Statement {
    /// Index one past the last significant token.
    end: usize,
    terminator: Terminator,
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

impl Parser<'_> {
    fn kind_at(&self, i: usize) -> SyntaxKind {
        self.tokens.get(i).map_or(EOF, |t| t.kind)
    }

    fn current(&self) -> SyntaxKind {
        self.kind_at(self.pos)
    }

    fn text_at(&self, i: usize) -> &str {
        &self.src[self.tokens[i].range]
    }

    fn range_of(&self, start: usize, end: usize) -> TextRange {
        let first = self.tokens[start].range;
        let last = self.tokens[end.max(start + 1) - 1].range;
        first.cover(last)
    }

    fn bump(&mut self) {
        let t = self.tokens[self.pos];
        self.builder.token(t.kind.into(), &self.src[t.range]);
        self.pos += 1;
    }

    /// Bumps tokens up to `end`, flagging semicolons.
    fn bump_until(&mut self, end: usize) {
        while self.pos < end {
            if self.current() == SEMICOLON {
                self.error_at(
                    self.pos,
                    "stylet doesn't use semicolons; end the line instead",
                );
                self.builder.start_node(ERROR.into());
                self.bump();
                self.builder.finish_node();
            } else {
                self.bump();
            }
        }
    }

    fn error_at(&mut self, i: usize, message: impl Into<String>) {
        let range = self.tokens[i].range;
        self.errors.push(SyntaxError::new(message, range));
    }

    fn error_range(&mut self, start: usize, end: usize, message: impl Into<String>) {
        let range = self.range_of(start, end);
        self.errors.push(SyntaxError::new(message, range));
    }

    fn next_significant(&self, mut i: usize) -> usize {
        while self.kind_at(i).is_trivia() {
            i += 1;
        }
        i
    }

    /// Items of the root (`depth == 0`) or of a block, until `}` or EOF.
    fn items(&mut self, depth: usize) {
        loop {
            match self.current() {
                EOF => return,
                kind if kind.is_trivia() => self.bump(),
                R_BRACE if depth > 0 => return,
                R_BRACE => {
                    self.error_at(self.pos, "unmatched `}`");
                    self.builder.start_node(ERROR.into());
                    self.bump();
                    self.builder.finish_node();
                }
                _ => self.statement(depth),
            }
        }
    }

    fn statement(&mut self, depth: usize) {
        let start = self.pos;
        let at_name =
            (self.current() == AT_KEYWORD).then(|| self.text_at(start)[1..].to_ascii_lowercase());
        let until_brace = at_name
            .as_deref()
            .is_some_and(|n| BLOCK_AT_RULES.contains(&n));
        let stmt = self.scan(start, until_brace);

        match at_name.as_deref() {
            Some("import") => return self.keyword_statement(IMPORT, stmt, depth),
            Some("extend" | "extends") => return self.keyword_statement(EXTEND, stmt, depth),
            Some(_) => return self.at_rule(stmt, depth),
            None => {}
        }

        if let Terminator::LBrace(_) = stmt.terminator {
            if self.current() == PLACEHOLDER_NAME {
                return self.placeholder(stmt, depth);
            }
            return self.rule(stmt, depth);
        }

        let colon = self.next_significant(start + 1);
        if self.current() == IDENT && colon < stmt.end && self.kind_at(colon) == COLON {
            return self.declaration(stmt, colon);
        }

        self.error_range(
            start,
            stmt.end,
            "expected a declaration (`property: value`), a rule (`selector { … }`) or an at-rule",
        );
        self.builder.start_node(ERROR.into());
        self.bump_until(stmt.end);
        self.builder.finish_node();
    }

    fn scan(&self, start: usize, until_brace: bool) -> Statement {
        let indent = self.line_indent(start);
        let mut depth = 0usize;
        let mut last_sig: Option<usize> = None;
        let mut i = start;
        let terminator = loop {
            match self.kind_at(i) {
                EOF => break Terminator::Eof,
                L_BRACE => break Terminator::LBrace(i),
                R_BRACE => break Terminator::RBrace,
                L_PAREN | L_BRACK => depth += 1,
                R_PAREN | R_BRACK => depth = depth.saturating_sub(1),
                NEWLINE if depth == 0 && !until_brace && !self.continues(i, last_sig, indent) => {
                    break Terminator::Newline;
                }
                _ => {}
            }
            if !self.kind_at(i).is_trivia() {
                last_sig = Some(i);
            }
            i += 1;
        };
        Statement {
            end: last_sig.map_or(start, |i| i + 1),
            terminator,
        }
    }

    /// Whether the statement continues after the newline at `newline`.
    fn continues(&self, newline: usize, last_sig: Option<usize>, indent: usize) -> bool {
        if last_sig.is_some_and(|i| self.kind_at(i) == COMMA) {
            return true;
        }
        let next = self.next_significant(newline);
        match self.kind_at(next) {
            L_BRACE => true,
            STRING => {
                let blank_line = self.tokens[newline + 1..next]
                    .iter()
                    .any(|t| t.kind == NEWLINE);
                !blank_line && self.line_indent(next) > indent
            }
            _ => false,
        }
    }

    /// Indentation width of the line containing token `i`.
    fn line_indent(&self, i: usize) -> usize {
        let mut line_start = i;
        while line_start > 0 && self.kind_at(line_start - 1) != NEWLINE {
            line_start -= 1;
        }
        match self.kind_at(line_start) {
            WHITESPACE => self.text_at(line_start).chars().count(),
            _ => 0,
        }
    }

    fn block(&mut self, depth: usize) {
        self.builder.start_node(BLOCK.into());
        self.bump(); // {
        self.items(depth + 1);
        if self.current() == R_BRACE {
            self.bump();
        } else {
            self.error_at(self.pos, "expected `}`");
        }
        self.builder.finish_node();
    }

    /// Bumps trivia between a prelude and its `{`, then parses the block.
    fn block_after(&mut self, stmt: &Statement, depth: usize) {
        if let Terminator::LBrace(brace) = stmt.terminator {
            while self.pos < brace {
                self.bump();
            }
            self.block(depth);
        }
    }

    fn rule(&mut self, stmt: Statement, depth: usize) {
        self.builder.start_node(RULE.into());
        if stmt.end > self.pos {
            self.builder.start_node(SELECTOR.into());
            self.bump_until(stmt.end);
            self.builder.finish_node();
        } else {
            self.error_at(self.pos, "expected a selector before `{`");
        }
        self.block_after(&stmt, depth);
        self.builder.finish_node();
    }

    fn placeholder(&mut self, stmt: Statement, depth: usize) {
        let start = self.pos;
        if depth > 0 {
            self.error_at(start, "placeholders must be defined at the top level");
        }
        if self.next_significant(start + 1) < stmt.end {
            self.error_range(
                start,
                stmt.end,
                "a placeholder selector can't be combined with other selectors",
            );
        }
        self.builder.start_node(PLACEHOLDER.into());
        self.bump_until(stmt.end);
        self.block_after(&stmt, depth);
        self.builder.finish_node();
    }

    fn declaration(&mut self, stmt: Statement, colon: usize) {
        let custom = self.text_at(self.pos).starts_with("--");
        self.builder.start_node(DECLARATION.into());
        self.builder.start_node(PROPERTY.into());
        self.bump();
        self.builder.finish_node();
        while self.pos <= colon {
            self.bump();
        }
        let value = self.next_significant(self.pos);
        if value < stmt.end {
            while self.pos < value {
                self.bump();
            }
            self.builder.start_node(VALUE.into());
            self.bump_until(stmt.end);
            self.builder.finish_node();
        } else if !custom {
            self.error_at(colon, "expected a value after `:`");
        }
        self.builder.finish_node();
    }

    fn at_rule(&mut self, stmt: Statement, depth: usize) {
        self.builder.start_node(AT_RULE.into());
        self.bump(); // @keyword
        self.prelude(stmt.end);
        self.block_after(&stmt, depth);
        self.builder.finish_node();
    }

    /// `@import` and `@extend`: statements without a block.
    fn keyword_statement(&mut self, kind: SyntaxKind, stmt: Statement, depth: usize) {
        let keyword = self.pos;
        let first = self.next_significant(keyword + 1);
        if first >= stmt.end {
            let what = if kind == IMPORT {
                "a path"
            } else {
                "a placeholder"
            };
            self.error_at(
                keyword,
                format!("expected {what} after `{}`", self.text_at(keyword)),
            );
        } else if kind == IMPORT && !matches!(self.kind_at(first), STRING | URL) {
            self.error_at(first, "expected a quoted path");
        } else if kind == EXTEND {
            self.check_extend_targets(first, stmt.end);
        }
        self.builder.start_node(kind.into());
        self.bump();
        self.prelude(stmt.end);
        self.builder.finish_node();
        if let Terminator::LBrace(brace) = stmt.terminator {
            self.error_at(
                brace,
                format!("`{}` can't have a block", self.text_at(keyword)),
            );
            self.builder.start_node(ERROR.into());
            self.block_after(&stmt, depth);
            self.builder.finish_node();
        }
    }

    /// `@extend $a, $b`: a comma-separated list of placeholders.
    fn check_extend_targets(&mut self, first: usize, end: usize) {
        let mut expect_name = true;
        let mut i = first;
        while i < end {
            let kind = self.kind_at(i);
            if !kind.is_trivia() {
                match (expect_name, kind) {
                    (true, PLACEHOLDER_NAME) | (false, COMMA) => expect_name = !expect_name,
                    (true, _) => {
                        self.error_at(i, "only placeholders (`$name`) can be extended");
                        return;
                    }
                    (false, _) => {
                        self.error_at(i, "expected `,` between placeholders");
                        return;
                    }
                }
            }
            i += 1;
        }
    }

    fn prelude(&mut self, end: usize) {
        let first = self.next_significant(self.pos);
        if first >= end {
            return;
        }
        while self.pos < first {
            self.bump();
        }
        self.builder.start_node(PRELUDE.into());
        self.bump_until(end);
        self.builder.finish_node();
    }
}
