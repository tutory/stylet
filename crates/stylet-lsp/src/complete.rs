//! Completion contexts and the CSS property table.
//!
//! The context is read from the text before the cursor (half-typed code rarely
//! parses into anything useful); only "is this inside a block" asks the parse.

use std::sync::OnceLock;

/// A CSS property from `properties.tsv` (generated from `@webref/css`).
pub struct Property {
    pub name: &'static str,
    pub syntax: &'static str,
    pub keywords: Vec<&'static str>,
    pub href: &'static str,
}

pub fn properties() -> &'static [Property] {
    static TABLE: OnceLock<Vec<Property>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("properties.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|line| {
                let mut cols = line.split('\t');
                Some(Property {
                    name: cols.next()?,
                    syntax: cols.next()?,
                    keywords: cols.next()?.split_whitespace().collect(),
                    href: cols.next().unwrap_or(""),
                })
            })
            .collect()
    })
}

pub fn property(name: &str) -> Option<&'static Property> {
    properties().iter().find(|p| p.name == name)
}

/// Keywords every property accepts.
pub const GLOBAL_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

#[derive(Debug, PartialEq, Eq)]
pub enum Context {
    /// A declaration's property name.
    Property,
    /// A value of `property`; `in_var` right after `var(`.
    Value {
        property: String,
        in_var: bool,
    },
    /// The placeholder after `@extend`.
    Extend,
    /// A `--name` in the prelude of `@media` & co.
    CustomMedia,
    None,
}

/// The completion context at `at`, and the byte offset where the word being
/// typed starts (the range completions replace).
pub fn context(text: &str, at: usize, in_block: bool) -> (Context, usize) {
    let before = &text[..at];
    let word_len = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '$'))
        .map(char::len_utf8)
        .sum::<usize>();
    let start = at - word_len;
    let statement = before[statement_start(before)..start].trim_start();

    if let Some(rest) = statement.strip_prefix('@') {
        let keyword: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        let after = &rest[keyword.len()..];
        let context = match keyword.as_str() {
            "extend" | "extends" if after.trim().is_empty() && !after.is_empty() => Context::Extend,
            "media" | "custom-media" | "import" | "container" | "supports"
                if text[start..at].starts_with("--") || after.ends_with('(') =>
            {
                Context::CustomMedia
            }
            _ => Context::None,
        };
        return (context, start);
    }
    if !in_block {
        return (Context::None, start);
    }
    if statement.is_empty() {
        return (Context::Property, start);
    }
    let Some(colon) = statement.find(':') else {
        return (Context::None, start);
    };
    let property = statement[..colon].trim_end();
    let is_name = !property.is_empty()
        && property
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
    if !is_name {
        return (Context::None, start);
    }
    let in_var = statement.trim_end().ends_with("var(");
    (
        Context::Value {
            property: property.to_ascii_lowercase(),
            in_var,
        },
        start,
    )
}

/// Start of the statement containing the end of `before`: after the last `{`,
/// `}` or line break outside brackets that doesn't continue a declaration.
fn statement_start(before: &str) -> usize {
    let mut start = 0;
    let mut depth = 0usize;
    for (i, c) in before.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '{' | '}' | ';' => {
                depth = 0;
                start = i + 1;
            }
            '\n' if depth == 0 => {
                let continued = before[start..i].trim_end().ends_with(',')
                    || before[i + 1..].trim_start().starts_with(['\'', '"']);
                if !continued {
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Context {
        let at = text.find('|').unwrap();
        let text = text.replace('|', "");
        context(&text, at, text[..at].contains('{')).0
    }

    fn value(property: &str, in_var: bool) -> Context {
        Context::Value {
            property: property.into(),
            in_var,
        }
    }

    #[test]
    fn contexts() {
        assert_eq!(at(".a {\n  col|\n}"), Context::Property);
        assert_eq!(at(".a { |"), Context::Property);
        assert_eq!(at(".a {\n  color: re|\n}"), value("color", false));
        assert_eq!(at(".a {\n  color: var(--|"), value("color", true));
        assert_eq!(at(".a {\n  color: var(|"), value("color", true));
        assert_eq!(
            at(".a {\n  transition: a 1s,\n    b|"),
            value("transition", false)
        );
        assert_eq!(
            at(".a {\n  margin: calc(1px +\n    |"),
            value("margin", false)
        );
        assert_eq!(at(".a {\n  @extend $bu|"), Context::Extend);
        assert_eq!(at(".a {\n  @extend |"), Context::Extend);
        assert_eq!(at("@media (--ph|"), Context::CustomMedia);
        assert_eq!(at("@media (|"), Context::CustomMedia);
        assert_eq!(at(".a {\n  &:ho|"), Context::None);
        assert_eq!(at(".a|"), Context::None);
        assert_eq!(at("@imp|"), Context::None);
    }

    #[test]
    fn table() {
        let display = property("display").unwrap();
        assert!(display.keywords.contains(&"flex"));
        assert!(display.href.starts_with("https://"));
        assert!(properties().len() > 500);
    }
}
