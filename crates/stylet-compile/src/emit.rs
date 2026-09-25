//! Writes the syntax tree as CSS, keeping nesting native and inlining imports.

use crate::custom_media::CustomMedia;
use crate::extend;
use crate::source_map::Mapping;
use crate::text::{self, Context, Piece, Pieces};
use crate::url::{self, Rewrite};
use crate::{Diagnostic, Options};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use stylet_resolve::{FileId, FileSystem, Loader};
use stylet_syntax::SyntaxKind::*;
use stylet_syntax::ast::{
    AtRule, Declaration, Extend, Import, ImportLayer, Item, Placeholder, Rule, unquote,
};
use stylet_syntax::{SyntaxElement, SyntaxNode, TextRange, TextSize};

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

/// At-rules that make styles conditional; `@extend` can't cross them.
const CONDITIONAL_RULES: &[&str] = &[
    "media",
    "supports",
    "container",
    "scope",
    "starting-style",
    "document",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ScopeKind {
    Style,
    Placeholder(String),
    /// Lowercase at-rule name without vendor prefix.
    AtRule(String),
}

/// An enclosing block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Scope {
    kind: ScopeKind,
    head: String,
}

/// Who extends a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Extender {
    Selector(String),
    /// A placeholder, or rules nested in it (`nested` holds their selectors).
    Placeholder {
        name: String,
        nested: Vec<String>,
    },
}

struct PlaceholderDef {
    name: String,
    /// Position in emission order (see `Emitter::clock`).
    at: u64,
    /// Where it is defined; the same definition may be emitted more than once.
    origin: (FileId, TextRange),
    /// Output range of the whole rule, including the separator before it.
    item: Range<usize>,
    /// Output range of the head, replaced by the extenders' selectors.
    head: Range<usize>,
}

struct ExtendUse {
    target: String,
    extender: Extender,
    file: FileId,
    range: TextRange,
    /// Position in emission order (see `Emitter::clock`).
    at: u64,
}

pub struct Emitter<'a, F> {
    loader: &'a mut Loader<F>,
    options: &'a Options,
    out_dir: PathBuf,
    custom_media: Option<CustomMedia>,
    /// File being emitted and the chain of files importing it.
    stack: Vec<FileId>,
    /// Enclosing blocks, e.g. `.a` and `@media print`.
    scope: Vec<Scope>,
    /// Imports already emitted, per file and scope.
    imported: HashSet<(FileId, Vec<Scope>)>,
    /// Output offset where the current item (including its separator) starts.
    item_start: usize,
    /// Counts emitted items; unlike output offsets it never goes back when
    /// empty rules are dropped.
    clock: u64,
    placeholders: Vec<PlaceholderDef>,
    /// Names of all defined placeholders, including empty ones.
    placeholder_names: HashSet<String>,
    extends: Vec<ExtendUse>,
    pub out: String,
    pub mappings: Vec<Mapping>,
    pub diagnostics: Vec<Diagnostic>,
    /// Every file read: imports and inlined assets.
    pub dependencies: Vec<PathBuf>,
    /// Style files that were emitted, in order.
    pub sources: Vec<FileId>,
}

impl<'a, F: FileSystem> Emitter<'a, F> {
    pub fn new(loader: &'a mut Loader<F>, options: &'a Options, out_dir: PathBuf) -> Self {
        Self {
            loader,
            options,
            out_dir,
            custom_media: options.resolve_custom_media.then(CustomMedia::default),
            stack: Vec::new(),
            scope: Vec::new(),
            imported: HashSet::new(),
            item_start: 0,
            clock: 0,
            placeholders: Vec::new(),
            placeholder_names: HashSet::new(),
            extends: Vec::new(),
            out: String::new(),
            mappings: Vec::new(),
            diagnostics: Vec::new(),
            dependencies: Vec::new(),
            sources: Vec::new(),
        }
    }

    pub fn finish(&mut self) {
        self.resolve_placeholders();
        if !self.options.minify && !self.out.is_empty() {
            self.out.push('\n');
        }
    }

    fn file(&self) -> FileId {
        *self.stack.last().expect("emitting outside of a file")
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        let file = self.file();
        self.diagnostics
            .push(Diagnostic::error(message, Some(file), range));
    }

    /// Records that the output at the current position comes from `offset`.
    fn map(&mut self, offset: TextSize) {
        if !self.options.source_map {
            return;
        }
        let file = self.file();
        let src = self.loader.file(file).line_index.line_col(offset);
        self.mappings.push(Mapping {
            out: self.out.len(),
            file,
            src,
        });
    }

    fn truncate(&mut self, len: usize) {
        self.out.truncate(len);
        while self.mappings.last().is_some_and(|m| m.out >= len) {
            self.mappings.pop();
        }
    }

    pub fn entry(&mut self, file: FileId) {
        self.dependencies.push(self.loader.file(file).path.clone());
        self.emit_file(file, Ctx::Root, 0);
    }

    fn emit_file(&mut self, file: FileId, ctx: Ctx, depth: usize) {
        if !self.sources.contains(&file) {
            self.sources.push(file);
        }
        self.stack.push(file);
        let source = self.loader.file(file);
        let root = source.parse.as_ref().map(|p| p.syntax());
        match root {
            Some(root) => self.items(&root, ctx, depth),
            None => {
                let css = source.text.trim().to_string();
                if !css.is_empty() {
                    self.line(depth);
                    self.map(0.into());
                    self.out += &css;
                }
            }
        }
        self.stack.pop();
    }

    fn items(&mut self, parent: &SyntaxNode, ctx: Ctx, depth: usize) {
        // Root items are separated by a blank line when either side has a block.
        let mut prev_had_block: Option<bool> = None;
        for element in parent.children_with_tokens() {
            let (item, has_block) = match element {
                SyntaxElement::Token(t) if t.kind() == BLOCK_COMMENT => {
                    if self.options.minify && !t.text().starts_with("/*!") {
                        continue;
                    }
                    (Err(t), false)
                }
                SyntaxElement::Node(n) => match Item::cast(n) {
                    Some(item) => {
                        let has_block = match &item {
                            Item::Rule(_) | Item::Placeholder(_) | Item::Import(_) => true,
                            Item::AtRule(rule) => rule.block().is_some(),
                            Item::Declaration(_) | Item::Extend(_) => false,
                        };
                        (Ok(item), has_block)
                    }
                    None => continue,
                },
                SyntaxElement::Token(_) => continue,
            };
            let start = self.out.len();
            if depth == 0
                && !self.options.minify
                && prev_had_block.is_some_and(|prev| prev || has_block)
            {
                self.out.push('\n');
            }
            let before_item = self.out.len();
            self.item_start = start;
            self.clock += 1;
            match item {
                Ok(item) => self.item(item, ctx, depth),
                Err(comment) => {
                    self.line(depth);
                    self.map(comment.text_range().start());
                    self.out += comment.text();
                }
            }
            if self.out.len() == before_item {
                self.truncate(start);
            } else {
                prev_had_block = Some(has_block);
            }
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

    fn item(&mut self, item: Item, ctx: Ctx, depth: usize) {
        match item {
            Item::Declaration(decl) => self.declaration(&decl, ctx, depth),
            Item::Rule(rule) => self.rule(&rule, ctx, depth),
            Item::AtRule(rule) => self.at_rule(&rule, ctx, depth),
            Item::Import(import) => self.import(&import, ctx, depth),
            Item::Placeholder(p) => self.placeholder(&p, ctx, depth),
            Item::Extend(e) => self.extend(&e, ctx),
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
        let value = match decl.value() {
            Some(v) => {
                let pieces = self.rewrite_urls(text::pieces(v.syntax()), v.syntax().text_range());
                text::join(&pieces, Context::Value, self.options.minify)
            }
            None => String::new(),
        };
        self.line(depth);
        self.map(property.text_range().start());
        self.out += property.text();
        self.out += if self.options.minify { ":" } else { ": " };
        self.out += &value;
        self.out.push(';');
    }

    fn rule(&mut self, rule: &Rule, ctx: Ctx, depth: usize) {
        let Some(selector) = rule.selector() else {
            return;
        };
        let range = selector.syntax().text_range();
        if ctx == Ctx::Declarations {
            self.error(range, "style rules aren't allowed here");
            return;
        }
        let has_amp = selector
            .syntax()
            .descendants_with_tokens()
            .any(|e| e.kind() == AMP);
        if has_amp && !ctx.in_style() && ctx != Ctx::Keyframes {
            self.error(range, "`&` needs a parent rule");
        }
        let inner = if ctx == Ctx::Keyframes {
            Ctx::Declarations
        } else {
            Ctx::Style
        };
        let head = text::serialize(selector.syntax(), Context::Selector, self.options.minify);
        if let Some(block) = rule.block() {
            self.block(ScopeKind::Style, head, range.start(), depth, true, |this| {
                this.items(block.syntax(), inner, depth + 1)
            });
        }
    }

    fn placeholder(&mut self, placeholder: &Placeholder, ctx: Ctx, depth: usize) {
        let range = placeholder.syntax().text_range();
        let (Some(name), Some(block)) = (placeholder.name(), placeholder.block()) else {
            return;
        };
        let name = name.text().to_string();
        // Only cascade layers may enclose a placeholder: inside style rules or
        // conditional rules, extenders' selectors would change meaning.
        let only_layers = self
            .scope
            .iter()
            .all(|s| matches!(&s.kind, ScopeKind::AtRule(name) if name == "layer"));
        if ctx.in_style() || !only_layers {
            return self.error(
                range,
                "placeholders must be defined at the top level of a file (or of a `@layer`)",
            );
        }
        let origin = (self.file(), range);
        if self
            .placeholders
            .iter()
            .any(|p| p.name == name && p.origin != origin)
        {
            return self.error(range, format!("placeholder `{name}` is already defined"));
        }
        self.placeholder_names.insert(name.clone());
        let item_start = self.item_start;
        let at = self.clock;
        let kind = ScopeKind::Placeholder(name.clone());
        let head = self.block(kind, name.clone(), range.start(), depth, true, |this| {
            this.items(block.syntax(), Ctx::Style, depth + 1)
        });
        if let Some(head) = head {
            self.placeholders.push(PlaceholderDef {
                name,
                at,
                origin,
                item: item_start..self.out.len(),
                head,
            });
        }
    }

    fn extend(&mut self, extend: &Extend, ctx: Ctx) {
        let range = extend.syntax().text_range();
        if !ctx.in_style() {
            return self.error(range, "`@extend` must be inside a rule");
        }
        let mut selector: Option<String> = None;
        let mut placeholder: Option<(String, Vec<String>)> = None;
        for scope in &self.scope {
            match &scope.kind {
                ScopeKind::AtRule(name) if CONDITIONAL_RULES.contains(&name.as_str()) => {
                    let message = format!("`@extend` can't be used inside `@{name}`");
                    return self.error(range, message);
                }
                ScopeKind::AtRule(_) => {}
                ScopeKind::Placeholder(name) => placeholder = Some((name.clone(), Vec::new())),
                ScopeKind::Style if placeholder.is_some() => {
                    if let Some((_, nested)) = &mut placeholder {
                        nested.push(scope.head.clone());
                    }
                }
                ScopeKind::Style => {
                    let separator = if self.options.minify { "," } else { ", " };
                    selector = Some(match selector {
                        Some(parent) => extend::nest(&parent, &scope.head, separator),
                        None => scope.head.clone(),
                    });
                }
            }
        }
        let extender = match (placeholder, selector) {
            (Some((name, nested)), _) => Extender::Placeholder { name, nested },
            (None, Some(selector)) => Extender::Selector(selector),
            (None, None) => return self.error(range, "`@extend` must be inside a rule"),
        };
        let file = self.file();
        let at = self.clock;
        for target in extend.targets() {
            self.extends.push(ExtendUse {
                target: target.text().to_string(),
                extender: extender.clone(),
                file,
                range,
                at,
            });
        }
    }

    /// Fills in each placeholder's selector list, or removes unused placeholders.
    fn resolve_placeholders(&mut self) {
        for use_ in &self.extends {
            if !self.placeholder_names.contains(&use_.target) {
                self.diagnostics.push(Diagnostic::error(
                    format!("unknown placeholder `{}`", use_.target),
                    Some(use_.file),
                    use_.range,
                ));
            }
        }
        let separator = if self.options.minify { "," } else { ", " };
        let edits = self
            .placeholders
            .iter()
            .map(|def| {
                let selectors = self.selectors_of(&def.name, Some(def.at), &mut Vec::new());
                if selectors.is_empty() {
                    (def.item.clone(), String::new())
                } else {
                    (def.head.clone(), selectors.join(separator))
                }
            })
            .collect();
        extend::apply_edits(&mut self.out, &mut self.mappings, edits);
        let leading = self.out.len() - self.out.trim_start_matches('\n').len();
        if leading > 0 {
            extend::apply_edits(
                &mut self.out,
                &mut self.mappings,
                vec![(0..leading, String::new())],
            );
        }
    }

    /// Selectors extending placeholder `name`, directly or through other placeholders.
    ///
    /// With `copy_at` (the output offset of one emitted copy), like Stylus an
    /// `@extend` only reaches copies emitted before it — or the first copy if
    /// none precedes it.
    fn selectors_of(
        &self,
        name: &str,
        copy_at: Option<u64>,
        visiting: &mut Vec<String>,
    ) -> Vec<String> {
        visiting.push(name.to_string());
        let mut out: Vec<String> = Vec::new();
        let copies: Vec<u64> = self
            .placeholders
            .iter()
            .filter(|p| p.name == name)
            .map(|p| p.at)
            .collect();
        let reaches = |use_: &ExtendUse| match copy_at {
            None => true,
            Some(at) => {
                at < use_.at
                    || (copies.iter().all(|&c| c >= use_.at) && copies.iter().min() == Some(&at))
            }
        };
        for use_ in self
            .extends
            .iter()
            .filter(|u| u.target == name && reaches(u))
        {
            let found = match &use_.extender {
                Extender::Selector(selector) => vec![selector.clone()],
                Extender::Placeholder { name: p, nested } if !visiting.contains(p) => {
                    let separator = if self.options.minify { "," } else { ", " };
                    self.selectors_of(p, None, visiting)
                        .into_iter()
                        .map(|base| {
                            nested.iter().fold(base, |parent, child| {
                                extend::nest(&parent, child, separator)
                            })
                        })
                        .collect()
                }
                Extender::Placeholder { .. } => Vec::new(),
            };
            for selector in found {
                if !out.contains(&selector) {
                    out.push(selector);
                }
            }
        }
        visiting.pop();
        out
    }

    fn at_rule(&mut self, rule: &AtRule, ctx: Ctx, depth: usize) {
        let name = rule.name().to_ascii_lowercase();
        let base = name.strip_prefix("-webkit-").unwrap_or(&name);
        let range = rule.syntax().text_range();

        if ctx.in_style() && TOP_LEVEL_ONLY.contains(&base) {
            return self.error(
                range,
                format!("`@{name}` can't be nested inside a style rule"),
            );
        }
        if base == "custom-media" && ctx != Ctx::Root {
            return self.error(range, "`@custom-media` must be at the top level");
        }
        if base == "custom-media"
            && let Some(media) = &mut self.custom_media
        {
            if let Err(message) = media.define(rule) {
                self.error(range, message);
            }
            return;
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
            self.map(range.start());
            self.out += &head;
            self.out.push(';');
            return;
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
        let kind = ScopeKind::AtRule(base.to_string());
        self.block(kind, head, range.start(), depth, drop_empty, |this| {
            this.items(block.syntax(), inner, depth + 1)
        });
    }

    fn import(&mut self, import: &Import, ctx: Ctx, depth: usize) {
        let range = import.syntax().text_range();
        let Some(spec) = import.path() else { return };
        let from = self.loader.file(self.file()).path.clone();
        let file = match self
            .loader
            .resolve(&spec, &from)
            .and_then(|path| self.loader.load(&path))
        {
            Ok(file) => file,
            Err(e) => return self.error(range, e.to_string()),
        };
        if let Some(i) = self.stack.iter().position(|&f| f == file) {
            let mut chain: Vec<_> = self.stack[i..]
                .iter()
                .map(|&f| self.loader.file(f).path.display().to_string())
                .collect();
            chain.push(self.loader.file(file).path.display().to_string());
            return self.error(range, format!("import cycle: {}", chain.join(" → ")));
        }
        let path = self.loader.file(file).path.clone();
        if !self.dependencies.contains(&path) {
            self.dependencies.push(path);
        }

        let layer = import.layer().map(|layer| match layer {
            ImportLayer::Anonymous => "@layer".to_string(),
            ImportLayer::Named(name) => format!("@layer {name}"),
        });
        let mut key = self.scope.clone();
        key.extend(layer.clone().map(|head| Scope {
            kind: ScopeKind::AtRule("layer".into()),
            head,
        }));
        if !self.imported.insert((file, key)) {
            return;
        }
        match layer {
            Some(head) => {
                let inner = Ctx::Group {
                    in_style: ctx.in_style(),
                };
                let kind = ScopeKind::AtRule("layer".into());
                self.block(kind, head, range.start(), depth, true, |this| {
                    this.emit_file(file, inner, depth + 1)
                });
            }
            None => self.emit_file(file, ctx, depth),
        }
    }

    /// Writes `head { … }` and returns the head's output range. With `drop_empty`,
    /// nothing is written for an empty block and `None` is returned.
    fn block(
        &mut self,
        kind: ScopeKind,
        head: String,
        at: TextSize,
        depth: usize,
        drop_empty: bool,
        body: impl FnOnce(&mut Self),
    ) -> Option<Range<usize>> {
        let start = self.out.len();
        self.line(depth);
        self.map(at);
        let head_range = self.out.len()..self.out.len() + head.len();
        self.out += &head;
        self.out += if self.options.minify { "{" } else { " {" };
        let body_start = self.out.len();
        self.scope.push(Scope { kind, head });
        body(self);
        self.scope.pop();
        if self.out.len() == body_start {
            if drop_empty {
                self.truncate(start);
                return None;
            }
        } else {
            self.line(depth);
        }
        self.out.push('}');
        Some(head_range)
    }

    /// Rebases relative `url(…)`s and inlines `?inline` ones.
    fn rewrite_urls(&mut self, pieces: Pieces, range: TextRange) -> Pieces {
        let file_dir = self
            .loader
            .file(self.file())
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let mut out = Pieces::with_capacity(pieces.len());
        let mut i = 0;
        while i < pieces.len() {
            let Some((url, quote, end)) = url_at(&pieces, i) else {
                out.push(pieces[i].clone());
                i += 1;
                continue;
            };
            let replacement = match url::rewrite(&url, &file_dir, &self.out_dir) {
                Rewrite::Keep => None,
                Rewrite::Rebased(url) => Some(url),
                Rewrite::Inline(path) => match self.loader.fs().read(&path) {
                    Ok(bytes) => {
                        self.dependencies.push(path.clone());
                        Some(url::data_uri(&path, &bytes))
                    }
                    Err(e) => {
                        self.error(range, format!("can't inline {}: {e}", path.display()));
                        None
                    }
                },
            };
            match replacement {
                Some(url) => out.push(Some(Piece {
                    kind: URL,
                    text: format!("url({quote}{url}{quote})"),
                })),
                None => out.extend(pieces[i..end].iter().cloned()),
            }
            i = end;
        }
        out
    }
}

/// A `url(x)` token or `url("x")` function at `i`: the URL, its quote and the end index.
fn url_at(pieces: &Pieces, i: usize) -> Option<(String, &'static str, usize)> {
    let piece = pieces[i].as_ref()?;
    if piece.kind == URL {
        let inner = piece.text[4..piece.text.len() - 1].trim();
        return Some((inner.to_string(), "", i + 1));
    }
    if piece.kind != IDENT || !piece.text.eq_ignore_ascii_case("url") {
        return None;
    }
    let significant = |j: usize| pieces.get(j).and_then(|p| p.as_ref());
    let skip_space = |j: usize| {
        if matches!(pieces.get(j), Some(None)) {
            j + 1
        } else {
            j
        }
    };
    if significant(i + 1)?.kind != L_PAREN {
        return None;
    }
    let string_at = skip_space(i + 2);
    let string = significant(string_at).filter(|p| p.kind == STRING)?;
    let close = skip_space(string_at + 1);
    if significant(close)?.kind != R_PAREN {
        return None;
    }
    let quote = if string.text.starts_with('\'') {
        "'"
    } else {
        "\""
    };
    Some((unquote(&string.text).to_string(), quote, close + 1))
}
