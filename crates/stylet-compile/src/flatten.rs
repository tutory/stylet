//! Compiles native nesting away, for browsers without CSS nesting.
//!
//! Works on the emitter's (nested) output: nested selectors are combined like
//! Stylus did (`.a, .b { .c {} }` → `.a .c, .b .c`), group rules inside style
//! rules (`@media`, `@supports`, `@layer`, …) move outwards and wrap the
//! resolved rules, and declarations after nested rules become a rule of their
//! own so the order is kept. Source mappings follow the moved text.

use crate::extend::split_list;
use crate::source_map::Mapping;
use std::collections::HashMap;

/// A statement of the nested CSS, with the output offset it starts at.
#[derive(Debug)]
enum Node {
    Rule {
        head: String,
        at: usize,
        children: Vec<Node>,
    },
    AtRule {
        head: String,
        at: usize,
        children: Option<Vec<Node>>,
    },
    Declaration {
        text: String,
        at: usize,
    },
    Comment {
        text: String,
        at: usize,
    },
}

/// Flat output.
#[derive(Debug)]
enum Flat {
    Rule {
        selectors: Vec<String>,
        at: usize,
        body: Vec<Flat>,
    },
    AtRule {
        head: String,
        at: usize,
        body: Option<Vec<Flat>>,
    },
    Declaration {
        text: String,
        at: usize,
    },
    Comment {
        text: String,
        at: usize,
    },
}

pub fn flatten(css: &str, mappings: &[Mapping], minify: bool) -> (String, Vec<Mapping>) {
    let chars: Vec<(usize, char)> = css.char_indices().collect();
    let mut i = 0;
    let nodes = parse_block(css, &chars, &mut i);
    let mut flat = Vec::new();
    walk(&nodes, None, &mut flat);

    let mut out = Writer {
        css: String::new(),
        minify,
        moves: Vec::new(),
    };
    out.items(&flat, 0, true);
    if !minify && !out.css.is_empty() {
        out.css.push('\n');
    }

    // Mappings at the old offset of each moved piece now point to its new offset.
    let mut by_offset: HashMap<usize, Vec<Mapping>> = HashMap::new();
    for m in mappings {
        by_offset.entry(m.out).or_default().push(*m);
    }
    let mut moved = Vec::new();
    for (new, old) in &out.moves {
        if let Some(list) = by_offset.get(old) {
            moved.extend(list.iter().map(|m| Mapping { out: *new, ..*m }));
        }
    }
    moved.sort_by_key(|m| m.out);
    (out.css, moved)
}

fn parse_block(css: &str, chars: &[(usize, char)], i: &mut usize) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut start: Option<usize> = None;
    let mut parens = 0usize;
    while *i < chars.len() {
        let (offset, c) = chars[*i];
        let begin = |start: &mut Option<usize>| {
            if start.is_none() {
                *start = Some(offset);
            }
        };
        match c {
            '/' if chars.get(*i + 1).is_some_and(|(_, n)| *n == '*') => {
                let text_start = offset;
                *i += 2;
                while *i + 1 < chars.len() && !(chars[*i].1 == '*' && chars[*i + 1].1 == '/') {
                    *i += 1;
                }
                *i += 2;
                let end = chars.get(*i).map_or(css.len(), |(o, _)| *o);
                if start.is_none() {
                    nodes.push(Node::Comment {
                        text: css[text_start..end].to_string(),
                        at: text_start,
                    });
                }
                continue;
            }
            '"' | '\'' => {
                begin(&mut start);
                *i += 1;
                while *i < chars.len() && chars[*i].1 != c {
                    if chars[*i].1 == '\\' {
                        *i += 1;
                    }
                    *i += 1;
                }
            }
            '\\' => {
                begin(&mut start);
                *i += 1;
            }
            '(' => {
                begin(&mut start);
                parens += 1;
            }
            ')' => parens = parens.saturating_sub(1),
            _ if parens > 0 => {}
            '{' => {
                let at = start.take().unwrap_or(offset);
                let head = css[at..offset].trim().to_string();
                *i += 1;
                let children = parse_block(css, chars, i);
                nodes.push(if head.starts_with('@') {
                    Node::AtRule {
                        head,
                        at,
                        children: Some(children),
                    }
                } else {
                    Node::Rule { head, at, children }
                });
                continue;
            }
            '}' | ';' => {
                if let Some(at) = start.take() {
                    let text = css[at..offset].trim().to_string();
                    nodes.push(if text.starts_with('@') {
                        Node::AtRule {
                            head: text,
                            at,
                            children: None,
                        }
                    } else {
                        Node::Declaration { text, at }
                    });
                }
                *i += 1;
                if c == '}' {
                    return nodes;
                }
                continue;
            }
            c if c.is_whitespace() => {}
            _ => begin(&mut start),
        }
        *i += 1;
    }
    nodes
}

/// `parent × child`: `&` is replaced, otherwise the child is a descendant.
fn combine(parents: &[String], child: &str) -> Vec<String> {
    let children = split_list(child);
    let mut out = Vec::new();
    for parent in parents {
        for child in &children {
            out.push(if child.contains('&') {
                child.replace('&', parent)
            } else {
                format!("{parent} {child}")
            });
        }
    }
    out
}

fn walk(nodes: &[Node], parents: Option<&[String]>, out: &mut Vec<Flat>) {
    // Index of the rule collecting this level's declarations, while nothing else follows it.
    let mut open: Option<usize> = None;
    for node in nodes {
        match node {
            Node::Declaration { text, at } => match parents {
                Some(selectors) => {
                    let index = match open {
                        Some(index) => index,
                        None => {
                            out.push(Flat::Rule {
                                selectors: selectors.to_vec(),
                                at: *at,
                                body: Vec::new(),
                            });
                            out.len() - 1
                        }
                    };
                    if let Flat::Rule { body, .. } = &mut out[index] {
                        body.push(Flat::Declaration {
                            text: text.clone(),
                            at: *at,
                        });
                    }
                    open = Some(index);
                }
                None => out.push(Flat::Declaration {
                    text: text.clone(),
                    at: *at,
                }),
            },
            Node::Comment { text, at } => {
                let comment = Flat::Comment {
                    text: text.clone(),
                    at: *at,
                };
                match open.and_then(|index| out.get_mut(index)) {
                    Some(Flat::Rule { body, .. }) => body.push(comment),
                    _ => out.push(comment),
                }
            }
            Node::Rule { head, at, children } => {
                open = None;
                let selectors = match parents {
                    Some(parents) => combine(parents, head),
                    None => split_list(head).into_iter().map(str::to_string).collect(),
                };
                let before = out.len();
                walk(children, Some(&selectors), out);
                if out.len() == before {
                    // Keep empty rules (e.g. keyframe selectors) visible.
                    out.push(Flat::Rule {
                        selectors,
                        at: *at,
                        body: Vec::new(),
                    });
                }
            }
            Node::AtRule { head, at, children } => {
                open = None;
                let body = children.as_ref().map(|children| {
                    let mut inner = Vec::new();
                    walk(children, parents, &mut inner);
                    inner
                });
                out.push(Flat::AtRule {
                    head: head.clone(),
                    at: *at,
                    body,
                });
            }
        }
    }
}

struct Writer {
    css: String,
    minify: bool,
    /// (new offset, old offset) of every written piece.
    moves: Vec<(usize, usize)>,
}

impl Writer {
    fn line(&mut self, depth: usize) {
        if self.minify {
            return;
        }
        if !self.css.is_empty() {
            self.css.push('\n');
        }
        for _ in 0..depth {
            self.css += "  ";
        }
    }

    fn mark(&mut self, old: usize) {
        self.moves.push((self.css.len(), old));
    }

    fn items(&mut self, items: &[Flat], depth: usize, top: bool) {
        let mut prev_block: Option<bool> = None;
        for item in items {
            let block = matches!(item, Flat::Rule { .. } | Flat::AtRule { body: Some(_), .. });
            if top && !self.minify && prev_block.is_some_and(|p| p || block) {
                self.css.push('\n');
            }
            prev_block = Some(block);
            match item {
                Flat::Declaration { text, at } => {
                    self.line(depth);
                    self.mark(*at);
                    self.css += text;
                    self.css.push(';');
                }
                Flat::Comment { text, at } => {
                    self.line(depth);
                    self.mark(*at);
                    self.css += text;
                }
                Flat::Rule {
                    selectors,
                    at,
                    body,
                } => {
                    let separator = if self.minify { "," } else { ", " };
                    self.block(&selectors.join(separator), *at, body, depth);
                }
                Flat::AtRule { head, at, body } => match body {
                    Some(body) => self.block(head, *at, body, depth),
                    None => {
                        self.line(depth);
                        self.mark(*at);
                        self.css += head;
                        self.css.push(';');
                    }
                },
            }
        }
    }

    fn block(&mut self, head: &str, at: usize, body: &[Flat], depth: usize) {
        self.line(depth);
        self.mark(at);
        self.css += head;
        self.css += if self.minify { "{" } else { " {" };
        let start = self.css.len();
        self.items(body, depth + 1, false);
        if self.css.len() > start {
            self.line(depth);
        }
        self.css.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(css: &str) -> String {
        flatten(css, &[], false).0
    }

    #[test]
    fn flattens() {
        let css = ".a, .b {\n  color: red;\n  &:hover {\n    color: blue;\n  }\n  > .c {\n    x: 1;\n  }\n  @media (width < 600px) {\n    padding: 0;\n    .d {\n      y: 2;\n    }\n  }\n  margin: 0;\n}\n";
        assert_eq!(
            flat(css),
            ".a, .b {\n  color: red;\n}\n\n.a:hover, .b:hover {\n  color: blue;\n}\n\n.a > .c, .b > .c {\n  x: 1;\n}\n\n@media (width < 600px) {\n  .a, .b {\n    padding: 0;\n  }\n  .a .d, .b .d {\n    y: 2;\n  }\n}\n\n.a, .b {\n  margin: 0;\n}\n"
        );
    }

    #[test]
    fn keeps_root_at_rules() {
        let css = "@charset \"utf-8\";\n@font-face {\n  font-family: X;\n}\n@keyframes k {\n  from {\n    x: 1;\n  }\n}\n@layer base {\n  a {\n    b: c;\n  }\n}\n";
        assert_eq!(
            flat(css),
            "@charset \"utf-8\";\n\n@font-face {\n  font-family: X;\n}\n\n@keyframes k {\n  from {\n    x: 1;\n  }\n}\n\n@layer base {\n  a {\n    b: c;\n  }\n}\n"
        );
    }

    #[test]
    fn minified() {
        let (css, _) = flatten(".a{b:c;.d{e:f;}}", &[], true);
        assert_eq!(css, ".a{b:c;}.a .d{e:f;}");
    }
}
