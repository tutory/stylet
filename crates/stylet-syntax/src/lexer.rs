//! Lossless tokenizer. Every byte of the input ends up in exactly one token.

use crate::SyntaxError;
use crate::SyntaxKind::{self, *};
use rowan::{TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub range: TextRange,
}

/// Splits `src` into tokens. The last token is always a zero-width [`EOF`].
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut lexer = Lexer {
        src,
        pos: 0,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    while lexer.pos < src.len() {
        let start = lexer.pos;
        let kind = lexer.next_kind();
        debug_assert!(lexer.pos > start, "lexer made no progress");
        lexer.tokens.push(Token {
            kind,
            range: range(start, lexer.pos),
        });
    }
    let end = TextSize::of(src);
    lexer.tokens.push(Token {
        kind: EOF,
        range: TextRange::empty(end),
    });
    (lexer.tokens, lexer.errors)
}

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new((start as u32).into(), (end as u32).into())
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    errors: Vec<SyntaxError>,
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    fn eat_while(&mut self, f: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&f) {
            self.bump();
        }
    }

    fn error(&mut self, start: usize, message: impl Into<String>) {
        self.errors
            .push(SyntaxError::new(message, range(start, self.pos)));
    }

    fn next_kind(&mut self) -> SyntaxKind {
        let start = self.pos;
        let Some(c) = self.peek() else { return EOF };
        match c {
            '\n' => {
                self.bump();
                NEWLINE
            }
            '\r' => {
                self.bump();
                if self.peek() == Some('\n') {
                    self.bump();
                }
                NEWLINE
            }
            c if is_space(c) => {
                self.eat_while(is_space);
                WHITESPACE
            }
            '/' if self.peek_nth(1) == Some('/') => {
                self.eat_while(|c| c != '\n' && c != '\r');
                LINE_COMMENT
            }
            '/' if self.peek_nth(1) == Some('*') => {
                self.bump();
                self.bump();
                match self.rest().find("*/") {
                    Some(i) => self.pos += i + 2,
                    None => {
                        self.pos = self.src.len();
                        self.error(start, "unterminated block comment");
                    }
                }
                BLOCK_COMMENT
            }
            '"' | '\'' => {
                self.string(c);
                STRING
            }
            '@' if self.starts_ident(1) => {
                self.bump();
                self.ident();
                AT_KEYWORD
            }
            '$' if self.starts_ident(1) => {
                self.bump();
                self.ident();
                PLACEHOLDER_NAME
            }
            '#' if self
                .peek_nth(1)
                .is_some_and(|c| is_name_char(c) || c == '\\') =>
            {
                self.bump();
                self.name();
                HASH
            }
            _ if self.starts_number() => {
                self.number();
                NUMBER
            }
            _ if self.starts_ident(0) => {
                self.ident();
                if self.src[start..self.pos].eq_ignore_ascii_case("url")
                    && self.peek() == Some('(')
                    && self.try_unquoted_url(start)
                {
                    return URL;
                }
                IDENT
            }
            _ => {
                self.bump();
                match c {
                    '{' => L_BRACE,
                    '}' => R_BRACE,
                    '(' => L_PAREN,
                    ')' => R_PAREN,
                    '[' => L_BRACK,
                    ']' => R_BRACK,
                    ':' => COLON,
                    ';' => SEMICOLON,
                    ',' => COMMA,
                    '.' => DOT,
                    '&' => AMP,
                    '>' => GT,
                    '+' => PLUS,
                    '~' => TILDE,
                    '*' => STAR,
                    '/' => SLASH,
                    '=' => EQ,
                    '!' => BANG,
                    '|' => PIPE,
                    _ => DELIM,
                }
            }
        }
    }

    fn string(&mut self, quote: char) {
        let start = self.pos;
        self.bump();
        loop {
            match self.peek() {
                None | Some('\n' | '\r') => {
                    self.error(start, "unterminated string");
                    return;
                }
                Some('\\') => {
                    self.bump();
                    self.bump();
                }
                Some(c) => {
                    self.bump();
                    if c == quote {
                        return;
                    }
                }
            }
        }
    }

    /// Whether an identifier starts `n` chars ahead.
    fn starts_ident(&self, n: usize) -> bool {
        match (self.peek_nth(n), self.peek_nth(n + 1)) {
            (Some('-'), Some(c)) => c == '-' || is_name_start(c) || c == '\\',
            (Some('\\'), Some(c)) => !matches!(c, '\n' | '\r'),
            (Some(c), _) => is_name_start(c),
            _ => false,
        }
    }

    fn ident(&mut self) {
        if self.peek() == Some('-') {
            self.bump();
        }
        self.name();
    }

    fn name(&mut self) {
        loop {
            match self.peek() {
                Some('\\') if !matches!(self.peek_nth(1), None | Some('\n' | '\r')) => {
                    self.bump();
                    self.bump();
                }
                Some(c) if is_name_char(c) => self.bump(),
                _ => return,
            }
        }
    }

    fn starts_number(&self) -> bool {
        let digit_at = |n| self.peek_nth(n).is_some_and(|c: char| c.is_ascii_digit());
        match self.peek() {
            Some('+' | '-') => digit_at(1) || (self.peek_nth(1) == Some('.') && digit_at(2)),
            Some('.') => digit_at(1),
            Some(c) => c.is_ascii_digit(),
            None => false,
        }
    }

    /// A number including its unit or `%`, e.g. `-1.5em`.
    fn number(&mut self) {
        if matches!(self.peek(), Some('+' | '-')) {
            self.bump();
        }
        self.eat_while(|c| c.is_ascii_digit());
        if self.peek() == Some('.') && self.peek_nth(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.eat_while(|c| c.is_ascii_digit());
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let exponent = match self.peek_nth(1) {
                Some('+' | '-') => self.peek_nth(2).is_some_and(|c| c.is_ascii_digit()),
                Some(c) => c.is_ascii_digit(),
                None => false,
            };
            if exponent {
                self.bump();
                self.bump();
                self.eat_while(|c| c.is_ascii_digit());
            }
        }
        if self.peek() == Some('%') {
            self.bump();
        } else if self.starts_ident(0) {
            self.ident();
        }
    }

    /// After `url` with `(` ahead: consumes `(…)` if the argument is unquoted.
    fn try_unquoted_url(&mut self, start: usize) -> bool {
        let after_paren = &self.rest()[1..];
        let arg = after_paren.trim_start_matches(is_space);
        if arg.starts_with(['"', '\'']) {
            return false;
        }
        match arg.find([')', '\n', '\r']) {
            Some(i) if arg[i..].starts_with(')') => {
                self.pos += 1 + (after_paren.len() - arg.len()) + i + 1;
            }
            _ => {
                self.eat_while(|c| c != '\n' && c != '\r');
                self.error(start, "unterminated url(");
            }
        }
        true
    }
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{feff}' | '\u{c}')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::{Expect, expect};

    fn check(src: &str, expect: Expect) {
        let (tokens, errors) = tokenize(src);
        let mut out = String::new();
        for t in &tokens {
            out += &format!("{:?} {:?}\n", t.kind, &src[t.range]);
        }
        for e in &errors {
            out += &format!("error {:?}: {}\n", e.range(), e.message());
        }
        expect.assert_eq(&out);
    }

    #[test]
    fn declaration() {
        check(
            ".a { color: #fff !important }",
            expect![[r##"
                DOT "."
                IDENT "a"
                WHITESPACE " "
                L_BRACE "{"
                WHITESPACE " "
                IDENT "color"
                COLON ":"
                WHITESPACE " "
                HASH "#fff"
                WHITESPACE " "
                BANG "!"
                IDENT "important"
                WHITESPACE " "
                R_BRACE "}"
                EOF ""
            "##]],
        );
    }

    #[test]
    fn numbers() {
        check(
            "1 -2px .5em +1.5% 1e3 2e -webkit-x --var 1-2",
            expect![[r##"
                NUMBER "1"
                WHITESPACE " "
                NUMBER "-2px"
                WHITESPACE " "
                NUMBER ".5em"
                WHITESPACE " "
                NUMBER "+1.5%"
                WHITESPACE " "
                NUMBER "1e3"
                WHITESPACE " "
                NUMBER "2e"
                WHITESPACE " "
                IDENT "-webkit-x"
                WHITESPACE " "
                IDENT "--var"
                WHITESPACE " "
                NUMBER "1"
                NUMBER "-2"
                EOF ""
            "##]],
        );
    }

    #[test]
    fn comments_and_urls() {
        check(
            "a // note\nurl(http://x.y/a.png) url( 'q' ) /* b */",
            expect![[r##"
                IDENT "a"
                WHITESPACE " "
                LINE_COMMENT "// note"
                NEWLINE "\n"
                URL "url(http://x.y/a.png)"
                WHITESPACE " "
                IDENT "url"
                L_PAREN "("
                WHITESPACE " "
                STRING "'q'"
                WHITESPACE " "
                R_PAREN ")"
                WHITESPACE " "
                BLOCK_COMMENT "/* b */"
                EOF ""
            "##]],
        );
    }

    #[test]
    fn at_placeholder_hash() {
        check(
            "@extend $btn\n@ $ #",
            expect![[r##"
                AT_KEYWORD "@extend"
                WHITESPACE " "
                PLACEHOLDER_NAME "$btn"
                NEWLINE "\n"
                DELIM "@"
                WHITESPACE " "
                DELIM "$"
                WHITESPACE " "
                DELIM "#"
                EOF ""
            "##]],
        );
    }

    #[test]
    fn errors() {
        check(
            "'abc\r\n/* x",
            expect![[r##"
                STRING "'abc"
                NEWLINE "\r\n"
                BLOCK_COMMENT "/* x"
                EOF ""
                error 0..4: unterminated string
                error 6..10: unterminated block comment
            "##]],
        );
    }
}
