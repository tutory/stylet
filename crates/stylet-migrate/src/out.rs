//! Output tree and stylet rendering.

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Comment(String),
    /// Literal CSS from `@css`.
    Raw(String),
    Import {
        path: String,
        layer: Option<String>,
    },
    Extend(String),
    Rule {
        selectors: Vec<String>,
        body: Vec<Out>,
    },
    Declaration {
        name: String,
        value: String,
        comment: Option<String>,
    },
    AtRule {
        name: String,
        prelude: String,
        body: Option<Vec<Out>>,
    },
    /// A global variable turned custom property; consecutive ones share a `:root` rule.
    RootVar {
        name: String,
        value: String,
    },
}

pub fn render(outs: &[Out]) -> String {
    let mut s = String::new();
    block(outs, 0, &mut s);
    s
}

fn line(s: &mut String, depth: usize, text: &str) {
    for _ in 0..depth {
        s.push_str("  ");
    }
    s.push_str(text);
    s.push('\n');
}

fn block(outs: &[Out], depth: usize, s: &mut String) {
    let mut i = 0;
    while i < outs.len() {
        match &outs[i] {
            Out::RootVar { .. } => {
                line(s, depth, ":root {");
                while let Some(Out::RootVar { name, value }) = outs.get(i) {
                    line(s, depth + 1, &format!("{name}: {value}"));
                    i += 1;
                }
                line(s, depth, "}");
                continue;
            }
            Out::Comment(text) => {
                for l in text.lines() {
                    line(s, depth, l.trim_end());
                }
            }
            Out::Raw(css) => {
                for l in css.lines() {
                    line(s, depth, l.trim());
                }
            }
            Out::Import { path, layer } => {
                let layer = layer
                    .as_ref()
                    .map(|l| format!(" layer({l})"))
                    .unwrap_or_default();
                line(s, depth, &format!("@import '{path}'{layer}"));
            }
            Out::Extend(targets) => line(s, depth, &format!("@extend {targets}")),
            Out::Rule { selectors, body } => {
                let last = selectors.len().saturating_sub(1);
                for (i, selector) in selectors.iter().enumerate() {
                    let suffix = if i == last { " {" } else { "," };
                    line(s, depth, &format!("{selector}{suffix}"));
                }
                block(body, depth + 1, s);
                line(s, depth, "}");
            }
            Out::Declaration {
                name,
                value,
                comment,
            } => {
                let comment = comment
                    .as_ref()
                    .map(|c| format!(" {c}"))
                    .unwrap_or_default();
                let value = if value.is_empty() {
                    String::new()
                } else {
                    format!(" {value}")
                };
                line(s, depth, &format!("{name}:{value}{comment}"));
            }
            Out::AtRule {
                name,
                prelude,
                body,
            } => {
                let head = if prelude.is_empty() {
                    format!("@{name}")
                } else {
                    format!("@{name} {prelude}")
                };
                match body {
                    None => line(s, depth, &head),
                    Some(body) => {
                        line(s, depth, &format!("{head} {{"));
                        block(body, depth + 1, s);
                        line(s, depth, "}");
                    }
                }
            }
        }
        i += 1;
    }
}
