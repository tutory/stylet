//! Stylus expressions: tokenizer and precedence-climbing parser.

use crate::value::{Color, Number};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(Number),
    Color(Color),
    Str(String, char),
    Ident(String),
    /// CSS text kept literally: unquoted `url(…)`, `calc(…)`, `!important`, unicode ranges.
    Raw(String),
    /// `@prop` property lookup.
    PropertyLookup(String),
    Call {
        name: String,
        args: Vec<Arg>,
    },
    Binary {
        op: &'static str,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: &'static str,
        expr: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    List {
        items: Vec<Expr>,
        comma: bool,
    },
    Paren(Box<Expr>),
    Hash(Vec<(String, Expr)>),
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
    },
    Member {
        expr: Box<Expr>,
        key: String,
    },
    /// `(expr)em`: Stylus sets the unit of a parenthesized number.
    Unit {
        expr: Box<Expr>,
        unit: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(Number),
    Color(Color),
    Str(String, char),
    Ident(String),
    Raw(String),
    AtProp(String),
    Op(&'static str),
    Open(char),
    Close(char),
    Comma,
    Colon,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    space_before: bool,
}

/// Functions whose arguments Stylus leaves untouched.
const LITERAL_FUNCTIONS: &[&str] = &["calc", "-webkit-calc", "-moz-calc"];

const OPERATORS: &[&str] = &[
    "...", "**", "==", "!=", "<=", ">=", "&&", "||", "..", "+", "-", "*", "/", "%", "<", ">", "!",
    "?", "=",
];

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut space = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        let start = i;
        let tok = if c == '"' || c == '\'' {
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    s.push(chars[i]);
                    i += 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err("unterminated string".into());
            }
            i += 1;
            Tok::Str(s, c)
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()))
        {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' && !chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                    break;
                }
                i += 1;
            }
            let number_end = i;
            if i < chars.len() && chars[i] == '%' {
                i += 1;
            } else {
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let digits: String = chars[start..number_end].iter().collect();
            let n: f64 = digits.parse().map_err(|_| format!("bad number `{text}`"))?;
            Tok::Number(Number {
                n,
                unit: chars[number_end..i].iter().collect(),
                text: Some(text),
            })
        } else if c == '#' {
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            match Color::parse_hex(&text) {
                Some(color) => Tok::Color(color),
                None => Tok::Raw(text),
            }
        } else if c == '$'
            || is_ident_start(c)
            || (c == '-'
                && chars
                    .get(i + 1)
                    .is_some_and(|&n| is_ident_start(n) || n == '-'))
        {
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            // Unicode ranges (`U+0025-00FF`).
            if (name == "U" || name == "u") && chars.get(i) == Some(&'+') {
                while i < chars.len()
                    && (chars[i].is_ascii_hexdigit() || matches!(chars[i], '+' | '-' | '?'))
                {
                    i += 1;
                }
                Tok::Raw(chars[start..i].iter().collect())
            } else if chars.get(i) == Some(&'(')
                && (name.eq_ignore_ascii_case("url") || LITERAL_FUNCTIONS.contains(&name.as_str()))
            {
                let rest: String = chars[i + 1..].iter().collect();
                let trimmed = rest.trim_start();
                if name.eq_ignore_ascii_case("url") && trimmed.starts_with(['"', '\'']) {
                    Tok::Ident(name)
                } else {
                    let close = matching_close(&chars, i).ok_or("unclosed `(`")?;
                    i = close + 1;
                    Tok::Raw(chars[start..i].iter().collect())
                }
            } else {
                Tok::Ident(name)
            }
        } else if c == '@' {
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            Tok::AtProp(chars[start + 1..i].iter().collect())
        } else if c == '!'
            && chars[i + 1..]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase()
                .starts_with("important")
        {
            i += "!important".len();
            Tok::Raw("!important".into())
        } else if matches!(c, '(' | '[' | '{') {
            i += 1;
            Tok::Open(c)
        } else if matches!(c, ')' | ']' | '}') {
            i += 1;
            Tok::Close(c)
        } else if c == ',' {
            i += 1;
            Tok::Comma
        } else if c == ':' {
            i += 1;
            Tok::Colon
        } else if let Some(op) = OPERATORS
            .iter()
            .find(|op| chars[i..].iter().take(op.len()).copied().eq(op.chars()))
        {
            i += op.len();
            Tok::Op(op)
        } else {
            return Err(format!("unexpected `{c}`"));
        };
        tokens.push(Token {
            tok,
            space_before: space,
        });
        space = false;
    }
    Ok(tokens)
}

fn matching_close(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, &c) in chars.iter().enumerate().skip(open) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

fn is_unit(name: &str) -> bool {
    const UNITS: &[&str] = &[
        "px", "em", "rem", "ex", "ch", "vw", "vh", "vmin", "vmax", "mm", "cm", "in", "pt", "pc",
        "q", "deg", "rad", "grad", "turn", "s", "ms", "fr", "dpi", "dppx", "svh", "lvh", "dvh",
        "cqw", "cqh",
    ];
    UNITS.contains(&name)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

pub fn parse(src: &str) -> Result<Expr, String> {
    let tokens = tokenize(src)?;
    let mut p = Parser { tokens, pos: 0 };
    let expr = p.comma_list()?;
    match p.peek() {
        None => Ok(expr),
        Some(t) => Err(format!("unexpected {:?}", t.tok)),
    }
}

/// Parses a parameter list: `a, b = 1, rest...`.
pub fn parse_params(src: &str) -> Result<Vec<(String, Option<Expr>, bool)>, String> {
    let mut params = Vec::new();
    for part in split_top_level(src) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, default) = match part.split_once('=') {
            Some((name, default)) => (name.trim(), Some(parse(default.trim())?)),
            None => (part, None),
        };
        let (name, rest) = match name.strip_suffix("...") {
            Some(name) => (name.trim(), true),
            None => (name, false),
        };
        params.push((name.to_string(), default, rest));
    }
    Ok(params)
}

/// Splits at commas outside brackets and strings.
pub fn split_top_level(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in src.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(&src[start..i]);
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    out.push(&src[start..]);
    out
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_tok(&self) -> Option<&Tok> {
        self.peek().map(|t| &t.tok)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek_tok() == Some(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok) -> Result<(), String> {
        if self.eat(&tok) {
            Ok(())
        } else {
            Err(format!("expected {tok:?}, found {:?}", self.peek_tok()))
        }
    }

    fn peek_word(&self, word: &str) -> bool {
        matches!(self.peek_tok(), Some(Tok::Ident(w)) if w == word)
    }

    fn comma_list(&mut self) -> Result<Expr, String> {
        let first = self.space_list()?;
        if self.peek_tok() != Some(&Tok::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(&Tok::Comma) {
            if matches!(self.peek_tok(), None | Some(Tok::Close(_))) {
                break;
            }
            items.push(self.space_list()?);
        }
        Ok(Expr::List { items, comma: true })
    }

    fn starts_operand(&self) -> bool {
        match self.peek() {
            None => false,
            Some(t) => match &t.tok {
                Tok::Comma | Tok::Colon | Tok::Close(_) => false,
                Tok::Op("-" | "+") => {
                    // `a -b`: a new negative item; `a - b`: handled by the binary parser.
                    let next_space = self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|n| n.space_before);
                    t.space_before && !next_space
                }
                Tok::Op("!") => true,
                Tok::Op(_) => false,
                Tok::Ident(w) => !matches!(
                    w.as_str(),
                    "and" | "or" | "is" | "isnt" | "in" | "not" | "if" | "unless"
                ),
                _ => true,
            },
        }
    }

    fn space_list(&mut self) -> Result<Expr, String> {
        let first = self.ternary()?;
        let mut items = vec![first];
        while self.starts_operand() {
            items.push(self.ternary()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Expr::List {
                items,
                comma: false,
            }
        })
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let cond = self.or()?;
        if self.eat(&Tok::Op("?")) {
            let then = self.or()?;
            self.expect(Tok::Colon)?;
            let otherwise = self.or()?;
            return Ok(Expr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            });
        }
        Ok(cond)
    }

    fn binary_loop(
        &mut self,
        ops: &[&'static str],
        words: &[(&str, &'static str)],
        next: fn(&mut Self) -> Result<Expr, String>,
    ) -> Result<Expr, String> {
        let mut lhs = next(self)?;
        loop {
            let op = match self.peek() {
                Some(Token {
                    tok: Tok::Op(op),
                    space_before,
                }) if ops.contains(op) => {
                    // `a -b` is a list, not a subtraction.
                    if matches!(*op, "-" | "+") {
                        let next_space = self
                            .tokens
                            .get(self.pos + 1)
                            .is_some_and(|n| n.space_before);
                        if *space_before && !next_space {
                            break;
                        }
                    }
                    *op
                }
                Some(Token {
                    tok: Tok::Ident(w), ..
                }) => match words.iter().find(|(word, _)| w == word) {
                    Some((_, op)) => op,
                    None => break,
                },
                _ => break,
            };
            self.pos += 1;
            let mut op = op;
            if op == "==" && self.peek_word("not") {
                // `is not`
                self.pos += 1;
                op = "!=";
            }
            let rhs = next(self)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn or(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["||"], &[("or", "||")], Self::and)
    }

    fn and(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["&&"], &[("and", "&&")], Self::equality)
    }

    fn equality(&mut self) -> Result<Expr, String> {
        self.binary_loop(
            &["==", "!="],
            &[("is", "=="), ("isnt", "!="), ("in", "in")],
            Self::relational,
        )
    }

    fn relational(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["<", ">", "<=", ">="], &[], Self::range)
    }

    fn range(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["..", "..."], &[], Self::additive)
    }

    fn additive(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["+", "-"], &[], Self::multiplicative)
    }

    fn multiplicative(&mut self) -> Result<Expr, String> {
        self.binary_loop(&["*", "/", "%", "**"], &[], Self::unary)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek_tok() {
            Some(Tok::Op(op @ ("-" | "+" | "!"))) => {
                let op = *op;
                self.pos += 1;
                let expr = self.unary()?;
                Ok(Expr::Unary {
                    op,
                    expr: Box::new(expr),
                })
            }
            Some(Tok::Ident(w)) if w == "not" => {
                self.pos += 1;
                let expr = self.unary()?;
                Ok(Expr::Unary {
                    op: "!",
                    expr: Box::new(expr),
                })
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;
        if let Expr::Paren(_) = expr
            && let Some(Token {
                tok: Tok::Ident(unit),
                space_before: false,
            }) = self.peek()
            && is_unit(unit)
        {
            let unit = unit.clone();
            self.pos += 1;
            expr = Expr::Unit {
                expr: Box::new(expr),
                unit,
            };
        }
        loop {
            match self.peek() {
                Some(Token {
                    tok: Tok::Open('['),
                    space_before: false,
                }) => {
                    self.pos += 1;
                    let index = self.comma_list()?;
                    self.expect(Tok::Close(']'))?;
                    expr = Expr::Index {
                        expr: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Some(Token {
                    tok: Tok::Raw(r),
                    space_before: false,
                }) if r.starts_with('.') => break,
                _ => break,
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let token = self.next().ok_or("unexpected end of expression")?;
        Ok(match token.tok {
            Tok::Number(n) => Expr::Number(n),
            Tok::Color(c) => Expr::Color(c),
            Tok::Str(s, q) => Expr::Str(s, q),
            Tok::Raw(r) => Expr::Raw(r),
            Tok::AtProp(p) => Expr::PropertyLookup(p),
            Tok::Ident(name) => {
                if matches!(
                    self.peek(),
                    Some(Token {
                        tok: Tok::Open('('),
                        space_before: false
                    })
                ) {
                    self.pos += 1;
                    let args = self.args()?;
                    Expr::Call { name, args }
                } else {
                    Expr::Ident(name)
                }
            }
            Tok::Open('(') => {
                if self.eat(&Tok::Close(')')) {
                    return Ok(Expr::List {
                        items: Vec::new(),
                        comma: false,
                    });
                }
                let inner = self.comma_list()?;
                self.expect(Tok::Close(')'))?;
                Expr::Paren(Box::new(inner))
            }
            Tok::Open('{') => {
                let mut pairs = Vec::new();
                while !self.eat(&Tok::Close('}')) {
                    let key = match self.next().map(|t| t.tok) {
                        Some(Tok::Ident(k)) => k,
                        Some(Tok::Str(k, _)) => k,
                        Some(Tok::Number(n)) => n.css(),
                        other => return Err(format!("bad hash key {other:?}")),
                    };
                    self.expect(Tok::Colon)?;
                    let value = self.space_list()?;
                    pairs.push((key, value));
                    if !self.eat(&Tok::Comma) {
                        self.expect(Tok::Close('}'))?;
                        break;
                    }
                }
                Expr::Hash(pairs)
            }
            Tok::Op("-") => {
                let expr = self.unary()?;
                Expr::Unary {
                    op: "-",
                    expr: Box::new(expr),
                }
            }
            other => return Err(format!("unexpected {other:?}")),
        })
    }

    fn args(&mut self) -> Result<Vec<Arg>, String> {
        let mut args = Vec::new();
        while !self.eat(&Tok::Close(')')) {
            let name = match (
                self.peek_tok(),
                self.tokens.get(self.pos + 1).map(|t| &t.tok),
            ) {
                (Some(Tok::Ident(name)), Some(Tok::Colon)) => {
                    let name = name.clone();
                    self.pos += 2;
                    Some(name)
                }
                _ => None,
            };
            let value = self.space_list()?;
            args.push(Arg { name, value });
            if !self.eat(&Tok::Comma) {
                self.expect(Tok::Close(')'))?;
                break;
            }
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Expr {
        parse(src).unwrap_or_else(|e| panic!("{src}: {e}"))
    }

    #[test]
    fn lists_and_operators() {
        assert!(
            matches!(p("1px solid red"), Expr::List { comma: false, ref items } if items.len() == 3)
        );
        assert!(
            matches!(p("1px -2px"), Expr::List { comma: false, ref items } if items.len() == 2)
        );
        assert!(matches!(p("1px - 2px"), Expr::Binary { op: "-", .. }));
        assert!(matches!(p("a, b c"), Expr::List { comma: true, ref items } if items.len() == 2));
        assert!(matches!(p("x * 2 + 1"), Expr::Binary { op: "+", .. }));
        assert!(matches!(
            p("pos0 == top || (pos0 == bottom)"),
            Expr::Binary { op: "||", .. }
        ));
        assert!(matches!(p("a is not b"), Expr::Binary { op: "!=", .. }));
        assert!(matches!(p("c ? 1 : 2"), Expr::Ternary { .. }));
        assert!(matches!(p("1..3"), Expr::Binary { op: "..", .. }));
    }

    #[test]
    fn primaries() {
        assert!(matches!(p("calc(100% - x)"), Expr::Raw(ref r) if r == "calc(100% - x)"));
        assert!(matches!(p("url(a/b.png)"), Expr::Raw(ref r) if r == "url(a/b.png)"));
        assert!(matches!(p("url('a.png')"), Expr::Call { ref name, .. } if name == "url"));
        assert!(matches!(p("var(--a, x)"), Expr::Call { ref args, .. } if args.len() == 2));
        assert!(matches!(p("{ a: 1, b: 2px }"), Expr::Hash(ref pairs) if pairs.len() == 2));
        assert!(matches!(p("$h[key]"), Expr::Index { .. }));
        assert!(
            matches!(p("x !important"), Expr::List { ref items, .. } if items[1] == Expr::Raw("!important".into()))
        );
        assert!(matches!(p("U+F000-F022"), Expr::Raw(_)));
        assert!(matches!(p("#FFF"), Expr::Color(_)));
        assert!(
            matches!(p("f(color: red)"), Expr::Call { ref args, .. } if args[0].name.as_deref() == Some("color"))
        );
    }

    #[test]
    fn params() {
        let params = parse_params("pos0 = left, pos1 = false, args...").unwrap();
        assert_eq!(params.len(), 3);
        assert!(params[2].2);
    }
}
