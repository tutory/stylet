//! Typed views over the untyped syntax tree.

use crate::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl $name {
            pub fn cast(node: SyntaxNode) -> Option<Self> {
                (node.kind() == $kind).then(|| Self(node))
            }

            pub fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

ast_node!(Root, ROOT);
ast_node!(Block, BLOCK);
ast_node!(Rule, RULE);
ast_node!(Selector, SELECTOR);
ast_node!(Placeholder, PLACEHOLDER);
ast_node!(Declaration, DECLARATION);
ast_node!(Value, VALUE);
ast_node!(AtRule, AT_RULE);
ast_node!(Prelude, PRELUDE);
ast_node!(Import, IMPORT);
ast_node!(Extend, EXTEND);

/// A statement in the root or in a block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Rule(Rule),
    Placeholder(Placeholder),
    Declaration(Declaration),
    AtRule(AtRule),
    Import(Import),
    Extend(Extend),
}

impl Item {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            RULE => Self::Rule(Rule(node)),
            PLACEHOLDER => Self::Placeholder(Placeholder(node)),
            DECLARATION => Self::Declaration(Declaration(node)),
            AT_RULE => Self::AtRule(AtRule(node)),
            IMPORT => Self::Import(Import(node)),
            EXTEND => Self::Extend(Extend(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Rule(n) => n.syntax(),
            Self::Placeholder(n) => n.syntax(),
            Self::Declaration(n) => n.syntax(),
            Self::AtRule(n) => n.syntax(),
            Self::Import(n) => n.syntax(),
            Self::Extend(n) => n.syntax(),
        }
    }
}

fn child<N>(node: &SyntaxNode, cast: fn(SyntaxNode) -> Option<N>) -> Option<N> {
    node.children().find_map(cast)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
}

/// Significant tokens of a node, in order.
fn significant_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
}

impl Root {
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
}

impl Block {
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
}

impl Rule {
    pub fn selector(&self) -> Option<Selector> {
        child(&self.0, Selector::cast)
    }

    pub fn block(&self) -> Option<Block> {
        child(&self.0, Block::cast)
    }
}

impl Placeholder {
    /// The `$name` token.
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, PLACEHOLDER_NAME)
    }

    pub fn block(&self) -> Option<Block> {
        child(&self.0, Block::cast)
    }
}

impl Declaration {
    pub fn property(&self) -> Option<SyntaxToken> {
        self.0
            .children()
            .find(|n| n.kind() == PROPERTY)
            .and_then(|n| n.first_token())
    }

    /// Whether this declares a custom property (`--name`).
    pub fn is_custom(&self) -> bool {
        self.property().is_some_and(|p| p.text().starts_with("--"))
    }

    pub fn value(&self) -> Option<Value> {
        child(&self.0, Value::cast)
    }
}

impl AtRule {
    /// Name without the `@`, e.g. `media`.
    pub fn name(&self) -> String {
        token(&self.0, AT_KEYWORD).map_or_else(String::new, |t| t.text()[1..].to_string())
    }

    pub fn prelude(&self) -> Option<Prelude> {
        child(&self.0, Prelude::cast)
    }

    pub fn block(&self) -> Option<Block> {
        child(&self.0, Block::cast)
    }
}

/// Cascade layer of `@import '…' layer` / `layer(name)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportLayer {
    Anonymous,
    Named(String),
}

impl Import {
    /// The path token: a `STRING` or `URL`.
    pub fn path_token(&self) -> Option<SyntaxToken> {
        significant_tokens(&self.0)
            .nth(1)
            .filter(|t| matches!(t.kind(), STRING | URL))
    }

    /// The path without quotes or `url(…)`.
    pub fn path(&self) -> Option<String> {
        let token = self.path_token()?;
        let text = token.text();
        Some(match token.kind() {
            URL => text[4..text.len() - 1].trim().to_string(),
            _ => unquote(text).to_string(),
        })
    }

    pub fn layer(&self) -> Option<ImportLayer> {
        let tokens: Vec<_> = significant_tokens(&self.0).skip(2).collect();
        let first = tokens.first()?;
        if first.kind() != IDENT || !first.text().eq_ignore_ascii_case("layer") {
            return None;
        }
        if tokens.get(1).is_none_or(|t| t.kind() != L_PAREN) {
            return Some(ImportLayer::Anonymous);
        }
        let name: String = tokens[2..]
            .iter()
            .take_while(|t| t.kind() != R_PAREN)
            .map(|t| t.text().to_string())
            .collect();
        Some(ImportLayer::Named(name))
    }
}

impl Extend {
    /// The extended `$name` tokens.
    pub fn targets(&self) -> impl Iterator<Item = SyntaxToken> {
        significant_tokens(&self.0).filter(|t| t.kind() == PLACEHOLDER_NAME)
    }
}

/// Strips matching quotes from a string token's text.
pub fn unquote(text: &str) -> &str {
    let mut chars = text.chars();
    match (chars.next(), chars.next_back()) {
        (Some(q @ ('"' | '\'')), Some(e)) if q == e && text.len() >= 2 => &text[1..text.len() - 1],
        (Some('"' | '\''), _) => &text[1..],
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(src: &str) -> Vec<Item> {
        let parse = crate::parse(src);
        assert!(parse.errors().is_empty(), "{:?}", parse.errors());
        parse.root().items().collect()
    }

    #[test]
    fn import() {
        let paths: Vec<_> =
            items("@import 'a'\n@import \"b\" layer(x.y)\n@import url( c.css ) layer")
                .into_iter()
                .map(|i| match i {
                    Item::Import(i) => (i.path().unwrap(), i.layer()),
                    other => panic!("{other:?}"),
                })
                .collect();
        assert_eq!(
            paths,
            [
                ("a".into(), None),
                ("b".into(), Some(ImportLayer::Named("x.y".into()))),
                ("c.css".into(), Some(ImportLayer::Anonymous)),
            ]
        );
    }

    #[test]
    fn rule_contents() {
        let [Item::Rule(rule)] =
            &items(".a {\n  @extends $x, $y\n  --v:\n  @media print { color: red }\n}")[..]
        else {
            panic!()
        };
        assert_eq!(rule.selector().unwrap().syntax().to_string(), ".a");
        let inner: Vec<_> = rule.block().unwrap().items().collect();
        let Item::Extend(extend) = &inner[0] else {
            panic!()
        };
        let targets: Vec<_> = extend.targets().map(|t| t.text().to_string()).collect();
        assert_eq!(targets, ["$x", "$y"]);
        let Item::Declaration(decl) = &inner[1] else {
            panic!()
        };
        assert!(decl.is_custom() && decl.value().is_none());
        let Item::AtRule(media) = &inner[2] else {
            panic!()
        };
        assert_eq!(media.name(), "media");
        assert_eq!(media.prelude().unwrap().syntax().to_string(), "print");
    }

    #[test]
    fn unquote_strings() {
        assert_eq!(unquote("'a'"), "a");
        assert_eq!(unquote("\"a"), "a");
        assert_eq!(unquote("a"), "a");
        assert_eq!(unquote("'"), "");
    }
}
