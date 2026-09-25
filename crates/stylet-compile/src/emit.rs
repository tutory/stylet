//! Writes the syntax tree as CSS, keeping nesting native.

use crate::custom_media::CustomMedia;
use crate::text::{self, Context};
use crate::{Diagnostic, Options};
use stylet_syntax::SyntaxKind::*;
use stylet_syntax::ast::{AtRule, Declaration, Item, Rule};
use stylet_syntax::{SyntaxElement, SyntaxNode, TextRange};

/// At-rules whose block contains rules or, inside a style rule, declarations.
const GROUP_RULES: &[&str] = &[
    "media",
    "supports",
    "container",
    "layer",
    "scope",
    "starting-style",
    "document",
];

/// At-rules whose block contains declarations only.
const DECLARATION_RULES: &[&str] = &[
    "font-face",
    "page",
    "property",
    "counter-style",
    "font-palette-values",
    "font-feature-values",
    "position-try",
    "view-transition",
];

/// At-rules that are invalid inside a style rule.
const TOP_LEVEL_ONLY: &[&str] = &[
    "charset",
    "namespace",
    "custom-media",
    "font-face",
    "keyframes",
    "property",
    "counter-style",
    "font-palette-values",
    "font-feature-values",
    "page",
    "position-try",
    "view-transition",
];

/// What the enclosing block allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Root,
    Style,
    /// `@media` and friends; declarations are only allowed inside a style rule.
    Group {
        in_style: bool,
    },
    /// `@font-face` and friends.
    Declarations,
    /// `@keyframes`: keyframe selectors only.
    Keyframes,
}

impl Ctx {
    fn in_style(self) -> bool {
        matches!(self, Self::Style | Self::Group { in_style: true })
    }
}

pub struct Emitter<'a> {
    options: &'a Options,
    custom_media: Option<CustomMedia>,
    out: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> Emitter<'a> {
    pub fn new(options: &'a Options, custom_media: Option<CustomMedia>) -> Self {
        Self {
            options,
            custom_media,
            out: String::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn finish(mut self) -> (String, Vec<Diagnostic>) {
        if !self.options.minify && !self.out.is_empty() {
            self.out.push('\n');
        }
        (self.out, self.diagnostics)
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(message, range));
    }

    pub fn root(&mut self, root: &SyntaxNode) {
        self.items(root, Ctx::Root, 0);
    }

    fn items(&mut self, parent: &SyntaxNode, ctx: Ctx, depth: usize) {
        // Root items are separated by a blank line when either side has a block.
        let mut prev_had_block: Option<bool> = None;
        for element in parent.children_with_tokens() {
            let start = self.out.len();
            let has_block = match &element {
                SyntaxElement::Token(t) if t.kind() == BLOCK_COMMENT => {
                    if self.options.minify && !t.text().starts_with("/*!") {
                        continue;
                    }
                    self.line(depth);
                    self.out += t.text();
                    false
                }
                SyntaxElement::Node(n) => match Item::cast(n.clone()) {
                    Some(item) => self.item(item, ctx, depth),
                    None => continue,
                },
                SyntaxElement::Token(_) => continue,
            };
            if self.out.len() == start {
                continue;
            }
            if depth == 0
                && !self.options.minify
                && prev_had_block.is_some_and(|prev| prev || has_block)
            {
                self.out.insert(start, '\n');
            }
            prev_had_block = Some(has_block);
        }
    }

    /// Starts a new output line at `depth` (pretty mode only).
    fn line(&mut self, depth: usize) {
        if self.options.minify {
            return;
        }
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        for _ in 0..depth {
            self.out += "  ";
        }
    }

    /// Emits one item; returns whether it has a block.
    fn item(&mut self, item: Item, ctx: Ctx, depth: usize) -> bool {
        match item {
            Item::Declaration(decl) => {
                self.declaration(&decl, ctx, depth);
                false
            }
            Item::Rule(rule) => {
                self.rule(&rule, ctx, depth);
                true
            }
            Item::AtRule(rule) => self.at_rule(&rule, ctx, depth),
            Item::Import(import) => {
                self.error(
                    import.syntax().text_range(),
                    "`@import` isn't supported when compiling a single string",
                );
                false
            }
            Item::Placeholder(p) => {
                self.error(
                    p.syntax().text_range(),
                    "placeholders aren't implemented yet",
                );
                false
            }
            Item::Extend(e) => {
                self.error(e.syntax().text_range(), "`@extend` isn't implemented yet");
                false
            }
        }
    }

    fn declaration(&mut self, decl: &Declaration, ctx: Ctx, depth: usize) {
        let allowed = ctx.in_style() || ctx == Ctx::Declarations;
        if !allowed {
            let message = match ctx {
                Ctx::Keyframes => "`@keyframes` may only contain keyframe selectors",
                _ => "declarations must be inside a rule",
            };
            self.error(decl.syntax().text_range(), message);
            return;
        }
        let Some(property) = decl.property() else {
            return;
        };
        let value = decl
            .value()
            .map(|v| text::serialize(v.syntax(), Context::Value, self.options.minify))
            .unwrap_or_default();
        self.line(depth);
        self.out += property.text();
        self.out += if self.options.minify { ":" } else { ": " };
        self.out += &value;
        self.out.push(';');
    }

    fn rule(&mut self, rule: &Rule, ctx: Ctx, depth: usize) {
        let Some(selector) = rule.selector() else {
            return;
        };
        if ctx == Ctx::Declarations {
            self.error(
                selector.syntax().text_range(),
                "style rules aren't allowed here",
            );
            return;
        }
        let has_amp = selector
            .syntax()
            .descendants_with_tokens()
            .any(|e| e.kind() == AMP);
        if has_amp && !ctx.in_style() && ctx != Ctx::Keyframes {
            self.error(selector.syntax().text_range(), "`&` needs a parent rule");
        }
        let inner = if ctx == Ctx::Keyframes {
            Ctx::Declarations
        } else {
            Ctx::Style
        };
        let text = text::serialize(selector.syntax(), Context::Selector, self.options.minify);
        if let Some(block) = rule.block() {
            self.block(&text, block.syntax(), inner, depth, true);
        }
    }

    /// Returns whether the at-rule has a block.
    fn at_rule(&mut self, rule: &AtRule, ctx: Ctx, depth: usize) -> bool {
        let name = rule.name().to_ascii_lowercase();
        let base = name.strip_prefix("-webkit-").unwrap_or(&name);
        let range = rule.syntax().text_range();

        if ctx.in_style() && TOP_LEVEL_ONLY.contains(&base) {
            self.error(
                range,
                format!("`@{name}` can't be nested inside a style rule"),
            );
            return false;
        }
        if base == "custom-media" {
            if ctx != Ctx::Root {
                self.error(range, "`@custom-media` must be at the top level");
            }
            if self.custom_media.is_some() {
                return false;
            }
        }

        let mut prelude = rule
            .prelude()
            .map(|p| text::pieces(p.syntax()))
            .unwrap_or_default();
        if base == "media"
            && let Some(media) = &self.custom_media
        {
            match media.expand(&prelude) {
                Ok(expanded) => prelude = expanded,
                Err(message) => self.error(range, message),
            }
        }
        let prelude = text::join(&prelude, Context::Prelude, self.options.minify);
        let head = if prelude.is_empty() {
            format!("@{}", rule.name())
        } else {
            format!("@{} {prelude}", rule.name())
        };

        let Some(block) = rule.block() else {
            self.line(depth);
            self.out += &head;
            self.out.push(';');
            return false;
        };
        let (inner, drop_empty) = if GROUP_RULES.contains(&base) {
            (
                Ctx::Group {
                    in_style: ctx.in_style(),
                },
                true,
            )
        } else if DECLARATION_RULES.contains(&base) {
            (Ctx::Declarations, false)
        } else if base == "keyframes" {
            (Ctx::Keyframes, false)
        } else {
            (ctx, false)
        };
        self.block(&head, block.syntax(), inner, depth, drop_empty);
        true
    }

    /// Writes `head { … }`. With `drop_empty`, nothing is written for an empty block.
    fn block(&mut self, head: &str, block: &SyntaxNode, ctx: Ctx, depth: usize, drop_empty: bool) {
        let start = self.out.len();
        self.line(depth);
        self.out += head;
        self.out += if self.options.minify { "{" } else { " {" };
        let body = self.out.len();
        self.items(block, ctx, depth + 1);
        if self.out.len() == body {
            if drop_empty {
                self.out.truncate(start);
                return;
            }
        } else {
            self.line(depth);
        }
        self.out.push('}');
    }
}
