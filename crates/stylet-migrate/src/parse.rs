//! Indentation-based Stylus parser: source → statement tree. Expressions are
//! kept as raw text and parsed during evaluation.

/// A statement with the 1-based line it starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub line: u32,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `// …` or `/* … */`, verbatim.
    Comment(String),
    Import {
        path: String,
        require: bool,
    },
    Extend(String),
    /// Selector lines (may contain `{interpolation}`).
    Rule {
        selectors: Vec<String>,
        body: Vec<Stmt>,
    },
    Property {
        name: String,
        value: String,
        comment: Option<String>,
    },
    Assign {
        name: String,
        op: AssignOp,
        value: String,
    },
    /// `name =` followed by an indented block of statements.
    AssignBlock {
        name: String,
        body: Vec<Stmt>,
    },
    /// Mixin or function definition.
    Def {
        name: String,
        params: String,
        body: Vec<Stmt>,
    },
    /// `name(args)` as a statement.
    Call {
        name: String,
        args: String,
    },
    /// `if` / `else if` / `else` / `unless` chain.
    If {
        branches: Vec<Branch>,
    },
    For {
        vars: Vec<String>,
        iterable: String,
        body: Vec<Stmt>,
    },
    Return(String),
    /// `@media …` and friends; `body` is `None` for statement at-rules.
    AtRule {
        name: String,
        prelude: String,
        body: Option<Vec<Stmt>>,
    },
    /// `@css { … }`: literal CSS.
    Css(String),
    /// Any other line: an expression, e.g. a function's return value.
    Expr(String),
    /// Something the parser doesn't understand.
    Unknown {
        text: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    /// `?=`: only if undefined.
    Default,
    /// `+=`, `-=`, `*=`, `/=`.
    Compound(char),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// `None` for `else`.
    pub condition: Option<String>,
    /// `unless`.
    pub negate: bool,
    pub body: Vec<Stmt>,
}

/// A logical line: physical lines joined while brackets are open.
#[derive(Debug, Clone)]
struct Line {
    number: u32,
    indent: usize,
    text: String,
    /// Trailing `// …` comment.
    comment: Option<String>,
}

struct Node {
    line: Line,
    children: Vec<Node>,
}

pub fn parse(src: &str) -> Vec<Stmt> {
    let lines = logical_lines(src);
    let nodes = tree(lines);
    statements(nodes)
}

fn logical_lines(src: &str) -> Vec<Line> {
    let physical: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < physical.len() {
        let raw = physical[i].trim_end();
        let number = i as u32 + 1;
        i += 1;
        let trimmed = raw.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = indent_width(raw);

        // Block comments spanning lines.
        if trimmed.starts_with("/*") {
            let mut text = trimmed.to_string();
            while !text.contains("*/") && i < physical.len() {
                text.push('\n');
                text += physical[i].trim_end();
                i += 1;
            }
            out.push(Line {
                number,
                indent,
                text,
                comment: None,
            });
            continue;
        }

        // `@css { … }` spanning lines: keep verbatim up to the matching brace.
        if trimmed.starts_with("@css") && trimmed.contains('{') {
            let mut text = trimmed.to_string();
            while brace_balance(&text) > 0 && i < physical.len() {
                text.push('\n');
                text += physical[i];
                i += 1;
            }
            out.push(Line {
                number,
                indent,
                text,
                comment: None,
            });
            continue;
        }

        let (mut code, mut comment) = split_comment(trimmed);
        if code.is_empty() {
            out.push(Line {
                number,
                indent,
                text: comment.unwrap_or_default(),
                comment: None,
            });
            continue;
        }
        // A block comment starting after code and ending on a later line.
        if comment.is_none()
            && let Some(start) = unclosed_block_comment(&code)
        {
            let mut text = code[start..].to_string();
            code = code[..start].trim_end().to_string();
            while !text.contains("*/") && i < physical.len() {
                text.push('\n');
                text += physical[i].trim_end();
                i += 1;
            }
            comment = Some(text);
        }
        // Join continuation lines while brackets are open (hashes, calls) or
        // the line ends with a comma (selector lists, multi-part values).
        while (open_brackets(&code) > 0 || code.ends_with(',')) && i < physical.len() {
            let (next, next_comment) = split_comment(physical[i].trim());
            i += 1;
            if next.is_empty() {
                continue;
            }
            code.push(' ');
            code += &next;
            if next_comment.is_some() {
                comment = next_comment;
            }
        }
        out.push(Line {
            number,
            indent,
            text: code,
            comment,
        });
    }
    out
}

fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum()
}

/// Splits off a trailing `// comment` outside strings and `url(…)`.
fn split_comment(line: &str) -> (String, Option<String>) {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut in_url = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'(' if line[..i].to_ascii_lowercase().ends_with("url") => in_url = true,
                b')' => in_url = false,
                b'/' if !in_url && bytes.get(i + 1) == Some(&b'/') => {
                    let code = line[..i].trim_end().to_string();
                    return (code, Some(line[i..].to_string()));
                }
                _ => {}
            },
        }
        i += 1;
    }
    (line.to_string(), None)
}

/// Byte offset of a `/*` without a matching `*/`, outside strings.
fn unclosed_block_comment(code: &str) -> Option<usize> {
    let start = code.find("/*")?;
    (!code[start..].contains("*/")).then_some(start)
}

/// Open `(`, `[` and hash `{` outside strings.
fn open_brackets(text: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut prev_significant = ' ';
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                // A hash literal: `{` after `=`, `:`, `,` or `(`.
                '{' if matches!(prev_significant, '=' | ':' | ',' | '(') => depth += 1,
                '}' if depth > 0 => depth -= 1,
                _ => {}
            },
        }
        if !c.is_whitespace() {
            prev_significant = c;
        }
    }
    depth
}

fn brace_balance(text: &str) -> i32 {
    text.chars().fold(0, |d, c| match c {
        '{' => d + 1,
        '}' => d - 1,
        _ => d,
    })
}

fn tree(lines: Vec<Line>) -> Vec<Node> {
    fn build(lines: &[Line], i: &mut usize, indent: usize, out: &mut Vec<Node>) {
        while *i < lines.len() && lines[*i].indent >= indent {
            let line = lines[*i].clone();
            *i += 1;
            let mut children = Vec::new();
            if *i < lines.len() && lines[*i].indent > line.indent {
                let child_indent = lines[*i].indent;
                build(lines, i, child_indent, &mut children);
            }
            out.push(Node { line, children });
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let indent = lines[i].indent;
        build(&lines, &mut i, indent, &mut out);
    }
    out
}

fn statements(nodes: Vec<Node>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut pending_selectors: Vec<(u32, String)> = Vec::new();
    let mut nodes = nodes.into_iter().peekable();
    while let Some(node) = nodes.next() {
        let line = node.line.number;
        let text = node.line.text.clone();
        let has_children = !node.children.is_empty();

        // Selector lines without a block belong to the next rule.
        if !has_children && is_selector_continuation(&text, nodes.peek()) {
            pending_selectors.push((line, text));
            continue;
        }
        if !pending_selectors.is_empty() && !(has_children && looks_like_rule(&text)) {
            for (line, text) in pending_selectors.drain(..) {
                out.push(unknown(line, text, "selector without a block"));
            }
        }

        // `else` branches attach to the preceding `if`.
        if let Some(branch) = else_branch(&text) {
            let body = statements(node.children);
            if let Some(Stmt {
                kind: StmtKind::If { branches },
                ..
            }) = out.last_mut()
            {
                branches.push(Branch { body, ..branch });
            } else {
                out.push(unknown(line, text, "`else` without `if`"));
            }
            continue;
        }

        let kind = classify(node, &mut pending_selectors);
        out.push(Stmt { line, kind });
    }
    for (line, text) in pending_selectors {
        out.push(unknown(line, text, "selector without a block"));
    }
    out
}

fn unknown(line: u32, text: String, reason: &str) -> Stmt {
    Stmt {
        line,
        kind: StmtKind::Unknown {
            text,
            reason: reason.into(),
        },
    }
}

fn classify(node: Node, pending_selectors: &mut Vec<(u32, String)>) -> StmtKind {
    let text = node.line.text.as_str();
    let has_children = !node.children.is_empty();

    if text.starts_with("//") || text.starts_with("/*") {
        return StmtKind::Comment(text.to_string());
    }
    if let Some(rest) = keyword(text, "@import").or_else(|| keyword(text, "@require")) {
        return StmtKind::Import {
            path: rest.to_string(),
            require: text.starts_with("@require"),
        };
    }
    if let Some(rest) = keyword(text, "@extends").or_else(|| keyword(text, "@extend")) {
        return StmtKind::Extend(rest.to_string());
    }
    if text.starts_with("@css") {
        let start = text.find('{').map_or(text.len(), |i| i + 1);
        let end = text.rfind('}').unwrap_or(text.len()).max(start);
        return StmtKind::Css(text[start..end].to_string());
    }
    if let Some(rest) = text.strip_prefix('@')
        && rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '-')
    {
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(rest.len());
        let name = rest[..name_len].to_string();
        let prelude = rest[name_len..].trim().to_string();
        let body = has_children.then(|| statements(node.children));
        return StmtKind::AtRule {
            name,
            prelude,
            body,
        };
    }
    if let Some(condition) = keyword(text, "if") {
        return StmtKind::If {
            branches: vec![Branch {
                condition: Some(condition.to_string()),
                negate: false,
                body: statements(node.children),
            }],
        };
    }
    if let Some(condition) = keyword(text, "unless") {
        return StmtKind::If {
            branches: vec![Branch {
                condition: Some(condition.to_string()),
                negate: true,
                body: statements(node.children),
            }],
        };
    }
    if let Some(rest) = keyword(text, "for")
        && let Some((vars, iterable)) = rest.split_once(" in ")
    {
        return StmtKind::For {
            vars: vars.split(',').map(|v| v.trim().to_string()).collect(),
            iterable: iterable.trim().to_string(),
            body: statements(node.children),
        };
    }
    if let Some(expr) = keyword(text, "return") {
        return StmtKind::Return(expr.to_string());
    }
    if let Some((name, AssignOp::Set, "")) = assignment(text)
        && has_children
    {
        return StmtKind::AssignBlock {
            name,
            body: statements(node.children),
        };
    }
    if let Some((name, op, value)) = assignment(text) {
        return StmtKind::Assign {
            name,
            op,
            value: value.to_string(),
        };
    }
    if let Some((name, args)) = call(text) {
        return if has_children {
            StmtKind::Def {
                name,
                params: args,
                body: statements(node.children),
            }
        } else {
            StmtKind::Call { name, args }
        };
    }
    // `prop:` with the value on the following, indented lines.
    if has_children
        && let Some((name, value)) = property(text)
        && value.is_empty()
        && node.children.iter().all(|c| c.children.is_empty())
    {
        let value: Vec<_> = node.children.iter().map(|c| c.line.text.as_str()).collect();
        return StmtKind::Property {
            name,
            value: value.join(" "),
            comment: node.line.comment,
        };
    }
    if has_children {
        let mut selectors: Vec<String> = pending_selectors.drain(..).map(|(_, s)| s).collect();
        selectors.push(text.to_string());
        return StmtKind::Rule {
            selectors,
            body: statements(node.children),
        };
    }
    if let Some((name, value)) = property(text) {
        return StmtKind::Property {
            name,
            value,
            comment: node.line.comment,
        };
    }
    StmtKind::Expr(text.to_string())
}

/// `text` without the leading `word` if it is followed by whitespace, `(` or the end.
fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    if rest.is_empty() || rest.starts_with([' ', '\t', '(']) {
        Some(rest.trim())
    } else {
        None
    }
}

/// `else`, `else if cond`, `else unless cond`.
fn else_branch(text: &str) -> Option<Branch> {
    let rest = keyword(text, "else")?;
    let (condition, negate) = if let Some(c) = keyword(rest, "if") {
        (Some(c.to_string()), false)
    } else if let Some(c) = keyword(rest, "unless") {
        (Some(c.to_string()), true)
    } else if rest.is_empty() {
        (None, false)
    } else {
        return None;
    };
    Some(Branch {
        condition,
        negate,
        body: Vec::new(),
    })
}

fn ident_len(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '$'))
        .unwrap_or(text.len())
}

fn assignment(text: &str) -> Option<(String, AssignOp, &str)> {
    let len = ident_len(text);
    let name = &text[..len];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return None;
    }
    let rest = text[len..].trim_start();
    let (op, value) = if let Some(v) = rest.strip_prefix("?=") {
        (AssignOp::Default, v)
    } else if let Some(v) = rest.strip_prefix(":=") {
        (AssignOp::Default, v)
    } else if let Some(op) = ['+', '-', '*', '/']
        .into_iter()
        .find(|op| rest.starts_with(&format!("{op}=")))
    {
        (AssignOp::Compound(op), &rest[2..])
    } else if let Some(v) = rest.strip_prefix('=') {
        if v.starts_with('=') {
            return None;
        }
        (AssignOp::Set, v)
    } else {
        return None;
    };
    Some((name.to_string(), op, value.trim()))
}

/// `name(args)` spanning the whole line.
fn call(text: &str) -> Option<(String, String)> {
    let len = ident_len(text);
    let name = &text[..len];
    if name.is_empty() || name.starts_with('$') || !text[len..].starts_with('(') {
        return None;
    }
    let rest = &text[len..];
    let close = matching_paren(rest)?;
    if !rest[close + 1..].trim().is_empty() {
        return None;
    }
    Some((name.to_string(), rest[1..close].to_string()))
}

fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
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

/// `name: value`, where `name` may contain `{interpolation}`.
fn property(text: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ':' if depth == 0 => {
                let after = &text[i + 1..];
                if !(after.is_empty() || after.starts_with([' ', '\t'])) {
                    return None;
                }
                let name = text[..i].trim();
                let valid = !name.is_empty()
                    && name.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '{' | '}' | '$' | '@')
                    });
                return valid.then(|| (name.to_string(), after.trim().to_string()));
            }
            c if depth == 0 && c.is_whitespace() => return colonless_property(text),
            _ => {}
        }
    }
    None
}

/// Colonless `display block`: only for well-known property names and names
/// with a dash (which selectors rarely start with).
fn colonless_property(text: &str) -> Option<(String, String)> {
    const KNOWN: &[&str] = &[
        "display",
        "color",
        "margin",
        "padding",
        "width",
        "height",
        "background",
        "border",
        "position",
        "top",
        "left",
        "right",
        "bottom",
        "content",
        "cursor",
        "opacity",
        "overflow",
        "z-index",
        "float",
        "clear",
    ];
    let (name, value) = text.split_once(char::is_whitespace)?;
    let dashed = name.contains('-') && name.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let value = value.trim();
    let selector_like = value.starts_with(['.', '#', '&', '>', '+', '~', '[', ':', '*']);
    ((KNOWN.contains(&name) || dashed) && !selector_like)
        .then(|| (name.to_string(), value.to_string()))
}

fn looks_like_rule(text: &str) -> bool {
    property(text).is_none() && assignment(text).is_none() && call(text).is_none()
}

/// A line without a block that continues a selector list: the following
/// siblings are selectors, the last of them with a block.
fn is_selector_continuation(text: &str, next: Option<&Node>) -> bool {
    if text.starts_with("//") || text.starts_with("/*") || text.starts_with('@') {
        return false;
    }
    let Some(next) = next else { return false };
    if !looks_like_rule(text) || property(text).is_some() {
        return false;
    }
    let next_text = &next.line.text;
    looks_like_rule(next_text) && !next_text.starts_with('@') && keyword(next_text, "if").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<StmtKind> {
        parse(src).into_iter().map(|s| s.kind).collect()
    }

    #[test]
    fn rules_and_properties() {
        let stmts = kinds(".a\n.b\n  color: red // c\n  &:hover\n    x: y\n");
        let [StmtKind::Rule { selectors, body }] = &stmts[..] else {
            panic!("{stmts:?}")
        };
        assert_eq!(selectors, &[".a", ".b"]);
        assert_eq!(
            body[0].kind,
            StmtKind::Property {
                name: "color".into(),
                value: "red".into(),
                comment: Some("// c".into())
            }
        );
        assert!(
            matches!(&body[1].kind, StmtKind::Rule { selectors, .. } if selectors == &["&:hover"])
        );
    }

    #[test]
    fn definitions_and_control_flow() {
        let src = "\
flexify(h = center)
  display: flex
  if h == left
    a: b
  else if h == right
    c: d
  else
    e: f
x = 1
$y ?= 2px
for n in 1..3
  .l-{n}
    z: n
bold()
";
        let stmts = kinds(src);
        let StmtKind::Def { name, params, body } = &stmts[0] else {
            panic!()
        };
        assert_eq!((name.as_str(), params.as_str()), ("flexify", "h = center"));
        let StmtKind::If { branches } = &body[1].kind else {
            panic!("{body:?}")
        };
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[2].condition, None);
        assert!(matches!(
            &stmts[1],
            StmtKind::Assign {
                op: AssignOp::Set,
                ..
            }
        ));
        assert!(matches!(
            &stmts[2],
            StmtKind::Assign {
                op: AssignOp::Default,
                ..
            }
        ));
        assert!(matches!(&stmts[3], StmtKind::For { vars, .. } if vars == &["n"]));
        assert!(matches!(&stmts[4], StmtKind::Call { name, .. } if name == "bold"));
    }

    #[test]
    fn multiline_constructs() {
        let src = "$h = {\n  a: 1,\n  b: 2\n}\n/* multi\n   line */\n@css {\n  .x { y: z; }\n}\n@media (max-width: phone)\n  .a\n    b: c\n";
        let stmts = kinds(src);
        assert!(matches!(&stmts[0], StmtKind::Assign { value, .. } if value == "{ a: 1, b: 2 }"));
        assert!(matches!(&stmts[1], StmtKind::Comment(c) if c.contains("line */")));
        assert!(matches!(&stmts[2], StmtKind::Css(css) if css.contains(".x { y: z; }")));
        assert!(
            matches!(&stmts[3], StmtKind::AtRule { name, body: Some(_), .. } if name == "media")
        );
    }

    #[test]
    fn comments_in_urls_and_strings() {
        assert_eq!(
            split_comment("a: url(http://x) // c").1.as_deref(),
            Some("// c")
        );
        assert_eq!(split_comment("a: '//' b").1, None);
    }
}
