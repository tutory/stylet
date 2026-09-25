//! Compares two CSS files semantically: `verify <expected.css> <actual.css>`.
//! Nesting is flattened, `:is()` lists are expanded, and values are normalized,
//! then the files are compared as multisets of (context, selector, property, value).

use std::collections::BTreeMap;

#[derive(Debug)]
enum Node {
    Rule {
        prelude: String,
        children: Vec<Node>,
    },
    Decl {
        prop: String,
        value: String,
    },
}

fn parse(css: &str) -> Vec<Node> {
    let chars: Vec<char> = css.chars().collect();
    let mut i = 0;
    block(&chars, &mut i)
}

fn block(c: &[char], i: &mut usize) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut buf = String::new();
    let mut parens = 0;
    while *i < c.len() {
        let ch = c[*i];
        match ch {
            '/' if c.get(*i + 1) == Some(&'*') => {
                *i += 2;
                while *i + 1 < c.len() && !(c[*i] == '*' && c[*i + 1] == '/') {
                    *i += 1;
                }
                *i += 2;
                continue;
            }
            '"' | '\'' => {
                buf.push(ch);
                *i += 1;
                while *i < c.len() && c[*i] != ch {
                    if c[*i] == '\\' {
                        buf.push(c[*i]);
                        *i += 1;
                    }
                    if *i < c.len() {
                        buf.push(c[*i]);
                    }
                    *i += 1;
                }
                buf.push(ch);
            }
            '\\' => {
                buf.push(ch);
                if let Some(&next) = c.get(*i + 1) {
                    buf.push(next);
                    *i += 1;
                }
            }
            '(' => {
                parens += 1;
                buf.push(ch);
            }
            ')' => {
                parens -= 1;
                buf.push(ch);
            }
            _ if parens > 0 => buf.push(ch),
            '{' => {
                *i += 1;
                let children = block(c, i);
                nodes.push(Node::Rule {
                    prelude: buf.trim().to_string(),
                    children,
                });
                buf.clear();
                continue;
            }
            '}' => {
                push_decl(&mut nodes, &buf);
                *i += 1;
                return nodes;
            }
            ';' => {
                push_decl(&mut nodes, &buf);
                buf.clear();
            }
            _ => buf.push(ch),
        }
        *i += 1;
    }
    push_decl(&mut nodes, &buf);
    nodes
}

fn push_decl(nodes: &mut Vec<Node>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if text.starts_with('@') {
        nodes.push(Node::Rule {
            prelude: text.to_string(),
            children: Vec::new(),
        });
        return;
    }
    if let Some((prop, value)) = text.split_once(':') {
        nodes.push(Node::Decl {
            prop: prop.trim().to_string(),
            value: value.trim().to_string(),
        });
    }
}

fn split_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut quote = None;
    for (i, ch) in s.char_indices() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(s[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    out.push(s[start..].trim().to_string());
    out
}

/// Expands the first top-level `:is(a, b)` into separate selectors.
fn expand_is(sel: &str) -> Vec<String> {
    let Some(start) = sel.find(":is(") else {
        return vec![sel.to_string()];
    };
    let open = start + 3;
    let mut depth = 0;
    let mut close = None;
    for (i, ch) in sel[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![sel.to_string()];
    };
    let (before, inner, after) = (&sel[..start], &sel[open + 1..close], &sel[close + 1..]);
    split_list(inner)
        .into_iter()
        .flat_map(|alt| expand_is(&format!("{before}{alt}{after}")))
        .collect()
}

fn nest(parents: &[String], child: &str) -> Vec<String> {
    let children = split_list(child);
    if parents.is_empty() {
        return children;
    }
    let mut out = Vec::new();
    for p in parents {
        for c in &children {
            out.push(if c.contains('&') {
                c.replace('&', p)
            } else {
                format!("{p} {c}")
            });
        }
    }
    out
}

fn norm_space(s: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

fn norm_selector(s: &str) -> String {
    let mut s = norm_space(s);
    for op in [">", "+", "~", ","] {
        s = s
            .replace(&format!(" {op} "), op)
            .replace(&format!(" {op}"), op)
            .replace(&format!("{op} "), op);
    }
    s.replace('"', "'")
}

/// Rounds long decimals (Stylus prints full precision).
fn round_numbers(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '.' && chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            let mut digits = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    digits.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            digits.truncate(3);
            out += digits.trim_end_matches('0');
            if out.ends_with('.') {
                out.pop();
            }
        }
    }
    out
}

/// Evaluates `calc()` of numbers with a common unit (or unitless factors).
fn eval_calc(s: &str) -> String {
    let mut out = s.to_string();
    for _ in 0..16 {
        let Some(start) = out.rfind("calc(") else {
            break;
        };
        let mut depth = 0;
        let mut end = None;
        for (i, ch) in out[start + 4..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + 4 + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        match calc_expr(&out[start + 5..end]) {
            Some((n, unit)) => {
                let mut text = format!("{:.3}", n);
                text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
                if text == "-0" {
                    text = "0".into();
                }
                out.replace_range(start..=end, &format!("{text}{unit}"));
            }
            None => break,
        }
    }
    out
}

fn calc_expr(s: &str) -> Option<(f64, String)> {
    let tokens: Vec<String> = s
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut pos = 0;
    let v = calc_sum(&tokens, &mut pos)?;
    (pos == tokens.len()).then_some(v)
}

fn calc_sum(t: &[String], pos: &mut usize) -> Option<(f64, String)> {
    let mut acc = calc_product(t, pos)?;
    while *pos < t.len() && (t[*pos] == "+" || t[*pos] == "-") {
        let op = t[*pos].clone();
        *pos += 1;
        let rhs = calc_product(t, pos)?;
        if acc.1 != rhs.1 && !(acc.0 == 0.0 || rhs.0 == 0.0) {
            return None;
        }
        let unit = if acc.1.is_empty() {
            rhs.1.clone()
        } else {
            acc.1.clone()
        };
        acc = (
            if op == "+" {
                acc.0 + rhs.0
            } else {
                acc.0 - rhs.0
            },
            unit,
        );
    }
    Some(acc)
}

fn calc_product(t: &[String], pos: &mut usize) -> Option<(f64, String)> {
    let mut acc = calc_atom(t, pos)?;
    while *pos < t.len() && (t[*pos] == "*" || t[*pos] == "/") {
        let op = t[*pos].clone();
        *pos += 1;
        let rhs = calc_atom(t, pos)?;
        acc = match (op.as_str(), acc.1.is_empty(), rhs.1.is_empty()) {
            ("*", _, true) => (acc.0 * rhs.0, acc.1),
            ("*", true, false) => (acc.0 * rhs.0, rhs.1),
            ("/", _, true) if rhs.0 != 0.0 => (acc.0 / rhs.0, acc.1),
            _ => return None,
        };
    }
    Some(acc)
}

fn calc_atom(t: &[String], pos: &mut usize) -> Option<(f64, String)> {
    let tok = t.get(*pos)?.clone();
    *pos += 1;
    if tok == "(" {
        let v = calc_sum(t, pos)?;
        if t.get(*pos)? != ")" {
            return None;
        }
        *pos += 1;
        return Some(v);
    }
    let split = tok
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(tok.len());
    let n: f64 = tok[..split].parse().ok()?;
    Some((n, tok[split..].to_string()))
}

/// `rgb(r,g,b)` → `#rrggbb`; `#rgba`/`#rrggbbaa` → `rgba(r,g,b,a)`.
fn norm_colors(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(start) = out.find("rgb(") {
        let Some(len) = out[start..].find(')') else {
            break;
        };
        let args: Vec<u8> = out[start + 4..start + len]
            .split([',', ' '])
            .filter(|a| !a.is_empty())
            .filter_map(|a| a.parse().ok())
            .collect();
        if args.len() != 3 {
            break;
        }
        out.replace_range(
            start..=start + len,
            &format!("#{:02x}{:02x}{:02x}", args[0], args[1], args[2]),
        );
    }
    out
}

fn norm_value(s: &str) -> String {
    let mut s = round_numbers(&eval_calc(&norm_colors(&norm_space(s))));
    s = s
        .replace(", ", ",")
        .replace(" ,", ",")
        .replace('"', "'")
        .replace(" !important", "!important");
    // 0.5 → .5, 1.0 → 1
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        if ch == '0'
            && chars.get(i + 1) == Some(&'.')
            && chars.get(i + 2).is_some_and(|c| c.is_ascii_digit())
            && !prev.is_ascii_digit()
            && prev != '.'
        {
            continue;
        }
        out.push(ch);
    }
    // lowercase hex colors, #abc → #aabbcc
    let mut result = String::new();
    let mut chars = out.chars().peekable();
    while let Some(ch) = chars.next() {
        result.push(ch);
        if ch == '#' {
            let mut hex = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() {
                    hex.push(c.to_ascii_lowercase());
                    chars.next();
                } else {
                    break;
                }
            }
            if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                hex = hex.chars().flat_map(|c| [c, c]).collect();
            }
            result += &hex;
        }
    }
    result
}

fn flatten(
    nodes: &[Node],
    parents: &[String],
    context: &str,
    out: &mut BTreeMap<(String, String, String, String), i32>,
    delta: i32,
) {
    for node in nodes {
        match node {
            Node::Decl { prop, value } => {
                for p in parents {
                    for sel in expand_is(p) {
                        let key = (
                            context.to_string(),
                            norm_selector(&sel),
                            prop.to_ascii_lowercase(),
                            norm_value(value),
                        );
                        *out.entry(key).or_default() += delta;
                    }
                }
            }
            Node::Rule { prelude, children } if prelude.starts_with('@') => {
                let name = prelude.split_whitespace().next().unwrap_or("");
                if matches!(
                    name,
                    "@media" | "@supports" | "@container" | "@layer" | "@scope" | "@document"
                ) {
                    let ctx = format!("{context} {}", norm_value(prelude));
                    flatten(children, parents, ctx.trim(), out, delta);
                } else if children.is_empty() {
                    let key = (
                        context.to_string(),
                        norm_value(prelude),
                        String::new(),
                        String::new(),
                    );
                    *out.entry(key).or_default() += delta;
                } else {
                    let ctx = format!("{context} {}", norm_value(prelude));
                    let own = vec![String::new()];
                    flatten(
                        children,
                        if name == "@keyframes" { &[] } else { &own },
                        ctx.trim(),
                        out,
                        delta,
                    );
                }
            }
            Node::Rule { prelude, children } => {
                let resolved = nest(parents, prelude);
                flatten(children, &resolved, context, out, delta);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let expected = std::fs::read_to_string(&args[1]).expect("expected css");
    let actual = std::fs::read_to_string(&args[2]).expect("actual css");
    let prefix = args.get(4).cloned().unwrap_or_else(|| "--s-".into());
    let mut expected_map = BTreeMap::new();
    flatten(&parse(&expected), &[], "", &mut expected_map, 1);
    let mut actual_map = BTreeMap::new();
    flatten(&parse(&actual), &[], "", &mut actual_map, 1);

    // Values of generated custom properties, to compare against literals.
    let mut generated: BTreeMap<String, String> = BTreeMap::new();
    for (_, sel, prop, value) in actual_map.keys() {
        if sel == ":root" && prop.starts_with(&prefix) {
            generated.insert(prop.clone(), value.clone());
        }
    }
    let substitute = |value: &str| {
        let mut v = value.to_string();
        for _ in 0..8 {
            let Some(start) = v.find(&format!("var({prefix}")) else {
                break;
            };
            let end = v[start..].find(')').map_or(v.len(), |e| start + e + 1);
            let name = v[start + 4..end - 1].to_ascii_lowercase();
            let Some(replacement) = generated.get(&name) else {
                break;
            };
            v.replace_range(start..end, replacement);
        }
        v
    };
    let mut diff: BTreeMap<(String, String, String, String), i32> = BTreeMap::new();
    for ((ctx, sel, prop, value), n) in expected_map {
        if prop == "stylet-removed-extend"
            || ["@-moz-", "@-webkit-", "@-o-", "@-ms-"]
                .iter()
                .any(|p| ctx.contains(p))
        {
            continue;
        }
        *diff.entry((ctx, sel, prop, value)).or_default() += n;
    }
    let total: i32 = diff.values().sum();
    for ((ctx, sel, prop, value), n) in actual_map {
        if prop.starts_with(&prefix) {
            continue;
        }
        *diff
            .entry((ctx, sel, prop, norm_value(&substitute(&value))))
            .or_default() -= n;
    }
    let missing: Vec<_> = diff.iter().filter(|(_, n)| **n > 0).collect();
    let extra: Vec<_> = diff.iter().filter(|(_, n)| **n < 0).collect();
    println!(
        "{total} declarations expected; {} missing, {} extra",
        missing.len(),
        extra.len()
    );
    let limit: usize = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(40);
    // Most common missing properties and selector heads.
    let mut by_prop: BTreeMap<&str, usize> = BTreeMap::new();
    for ((_, _, prop, _), _) in &missing {
        *by_prop.entry(prop.as_str()).or_default() += 1;
    }
    let mut top: Vec<_> = by_prop.into_iter().collect();
    top.sort_by_key(|t| std::cmp::Reverse(t.1));
    println!("missing by property: {:?}", &top[..top.len().min(12)]);
    let mut by_ctx: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for ((ctx, _, _, _), _) in &missing {
        by_ctx.entry(ctx.as_str()).or_default().0 += 1;
    }
    for ((ctx, _, _, _), _) in &extra {
        by_ctx.entry(ctx.as_str()).or_default().1 += 1;
    }
    let mut top: Vec<_> = by_ctx.into_iter().collect();
    top.sort_by_key(|t| std::cmp::Reverse(t.1.0 + t.1.1));
    for (ctx, (m, e)) in top.iter().take(12) {
        println!("  context {ctx:?}: {m} missing, {e} extra");
    }
    for (label, list) in [("missing", &missing), ("extra", &extra)] {
        for ((ctx, sel, prop, value), n) in list.iter().take(limit) {
            println!("{label} [{n}] {ctx} | {sel} | {prop}: {value}");
        }
    }
}
