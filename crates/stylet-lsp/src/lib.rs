//! Language server for stylet.
//!
//! Diagnostics come from compiling the project's entries (with unsaved editor
//! contents taking precedence over the files on disk), so imports and
//! placeholders are checked in their real context; open files no entry
//! reaches only get syntax errors. Also: formatting, go to definition for
//! imports and placeholders, placeholder references, document symbols and
//! completions (CSS properties and their keywords, the project's custom
//! properties, placeholders after `@extend`, custom media).

mod complete;
mod convert;

use convert::{offset, path_to_uri, range, uri_to_path};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, References, Request as _,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionTextEdit, DiagnosticSeverity,
    DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, InitializeParams, Location,
    OneOf, PublishDiagnosticsParams, ServerCapabilities, SymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Uri,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use stylet_resolve::{FileSystem, LineIndex, Loader, OsFs, ResolveConfig, normalize};
use stylet_syntax::SyntaxKind::*;
use stylet_syntax::ast::{AtRule, Declaration, Import, Item, Placeholder};
use stylet_syntax::{SyntaxNode, SyntaxToken, TextRange};

/// Project settings, usually from `stylet.toml`.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub resolve: ResolveConfig,
    /// Entry files; without entries every open file is compiled on its own.
    pub entries: Vec<PathBuf>,
    pub compile: stylet_compile::Options,
    pub fmt: stylet_fmt::Options,
}

/// Runs the server on stdin/stdout until the client shuts it down.
pub fn run(settings: Settings) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        document_formatting_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(["-", "$", "(", ":", " "].map(String::from).to_vec()),
            ..CompletionOptions::default()
        }),
        ..ServerCapabilities::default()
    };
    let params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let mut server = Server::new(settings);
    // `editor.action.triggerSuggest` is a VS Code command.
    server.suggest_values = params.client_info.is_some_and(|c| {
        ["Code", "VSCodium", "Cursor", "Windsurf"]
            .iter()
            .any(|n| c.name.contains(n))
    });
    server.main_loop(&connection)?;
    // The writer thread ends once the connection's sender is gone.
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Open documents over the real file system.
#[derive(Clone, Default)]
struct Overlay {
    docs: HashMap<PathBuf, String>,
}

impl FileSystem for Overlay {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.docs.get(&normalize(path)) {
            Some(text) => Ok(text.clone().into_bytes()),
            None => OsFs.read(path),
        }
    }

    fn is_file(&self, path: &Path) -> bool {
        self.docs.contains_key(&normalize(path)) || OsFs.is_file(path)
    }
}

pub struct Server {
    settings: Settings,
    fs: Overlay,
    /// Files with published diagnostics (to clear them later).
    published: HashSet<PathBuf>,
    /// Every stylet file reached by the last compile.
    known: HashSet<PathBuf>,
    /// Definitions per file, by text hash.
    definitions: HashMap<PathBuf, (u64, Rc<Definitions>)>,
    /// Whether picking a property opens the value completions (VS Code).
    pub suggest_values: bool,
}

/// What a file defines, for completions.
#[derive(Default)]
struct Definitions {
    /// Custom properties with their first value.
    custom_properties: Vec<(String, String)>,
    placeholders: Vec<String>,
    /// `@custom-media` names with their queries.
    custom_media: Vec<(String, String)>,
}

impl Definitions {
    fn of(root: &SyntaxNode) -> Self {
        let mut out = Self::default();
        for node in root.descendants() {
            match node.kind() {
                DECLARATION => {
                    let d = Declaration::cast(node).expect("declaration");
                    if let (true, Some(name)) = (d.is_custom(), d.property()) {
                        let value = d.value().map(|v| v.syntax().text().to_string());
                        let value = value
                            .unwrap_or_default()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        out.custom_properties.push((name.text().to_string(), value));
                    }
                }
                PLACEHOLDER => {
                    if let Some(name) = Placeholder::cast(node).and_then(|p| p.name()) {
                        out.placeholders.push(name.text().to_string());
                    }
                }
                AT_RULE => {
                    let rule = AtRule::cast(node).expect("at-rule");
                    let prelude = rule.prelude().map(|p| p.syntax().text().to_string());
                    let prelude = prelude.unwrap_or_default();
                    let prelude = prelude.trim();
                    match rule.name().as_str() {
                        "custom-media" => {
                            if let Some((name, query)) = prelude.split_once(char::is_whitespace) {
                                out.custom_media
                                    .push((name.to_string(), query.trim().to_string()));
                            }
                        }
                        // `@property --x { … }` registers a custom property.
                        "property" if prelude.starts_with("--") => {
                            out.custom_properties
                                .push((prelude.to_string(), String::new()));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        out
    }
}

impl Server {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            fs: Overlay::default(),
            published: HashSet::new(),
            known: HashSet::new(),
            definitions: HashMap::new(),
            suggest_values: false,
        }
    }

    fn main_loop(
        &mut self,
        connection: &Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Documents changed since the last compile; it waits for a quiet moment,
        // or for a request that needs to know the project's files.
        let mut dirty = false;
        for message in &connection.receiver {
            match message {
                Message::Request(request) => {
                    if connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    if dirty && self.known.is_empty() {
                        for note in self.diagnostics() {
                            connection.sender.send(Message::Notification(note))?;
                        }
                        dirty = false;
                    }
                    let response = self.request(request);
                    connection.sender.send(Message::Response(response))?;
                }
                Message::Notification(notification) => dirty |= self.notification(notification),
                Message::Response(_) => {}
            }
            if dirty && connection.receiver.is_empty() {
                for note in self.diagnostics() {
                    connection.sender.send(Message::Notification(note))?;
                }
                dirty = false;
            }
        }
        Ok(())
    }

    /// Handles a notification; returns whether documents changed.
    pub fn notification(&mut self, n: Notification) -> bool {
        let n = match cast_notification::<DidOpenTextDocument>(n) {
            Ok(p) => {
                self.set(&p.text_document.uri, p.text_document.text);
                return true;
            }
            Err(n) => n,
        };
        let n = match cast_notification::<DidChangeTextDocument>(n) {
            Ok(p) => {
                if let Some(change) = p.content_changes.into_iter().last() {
                    self.set(&p.text_document.uri, change.text);
                }
                return true;
            }
            Err(n) => n,
        };
        let n = match cast_notification::<DidCloseTextDocument>(n) {
            Ok(p) => {
                if let Some(path) = uri_to_path(&p.text_document.uri) {
                    self.fs.docs.remove(&path);
                }
                return true;
            }
            Err(n) => n,
        };
        cast_notification::<DidSaveTextDocument>(n).is_ok()
    }

    fn set(&mut self, uri: &Uri, text: String) {
        if let Some(path) = uri_to_path(uri) {
            self.fs.docs.insert(path, text);
        }
    }

    fn text(&self, path: &Path) -> Option<String> {
        self.fs.read_to_string(path).ok()
    }

    /// Compiles the project and returns `publishDiagnostics` notifications for
    /// every file whose diagnostics may have changed.
    pub fn diagnostics(&mut self) -> Vec<Notification> {
        let mut by_file: BTreeMap<PathBuf, Vec<lsp_types::Diagnostic>> = BTreeMap::new();
        let mut seen: HashSet<(PathBuf, u32, String)> = HashSet::new();
        let mut loader = Loader::new(self.fs.clone(), self.settings.resolve.clone());
        let entries = if self.settings.entries.is_empty() {
            self.fs.docs.keys().cloned().collect()
        } else {
            self.settings.entries.clone()
        };
        self.known.clear();
        for entry in &entries {
            let options = stylet_compile::Options {
                source_map: false,
                ..self.settings.compile.clone()
            };
            let out = stylet_compile::compile_file(&mut loader, entry, &options);
            for d in &out.diagnostics {
                let file = d
                    .file
                    .map_or_else(|| entry.clone(), |f| loader.file(f).path.clone());
                if !seen.insert((file.clone(), d.range.start().into(), d.message.clone())) {
                    continue;
                }
                let index = d
                    .file
                    .map_or_else(|| LineIndex::new(""), |f| loader.file(f).line_index.clone());
                let severity = match d.severity {
                    stylet_compile::Severity::Error => DiagnosticSeverity::ERROR,
                    stylet_compile::Severity::Warning => DiagnosticSeverity::WARNING,
                };
                by_file
                    .entry(file)
                    .or_default()
                    .push(diagnostic(&index, d.range, severity, &d.message));
            }
            self.known.extend(
                out.dependencies
                    .iter()
                    .filter(|p| p.extension().is_some_and(|e| e == "styl"))
                    .cloned(),
            );
        }
        // Open files outside every entry: syntax errors only.
        for (path, text) in &self.fs.docs {
            if self.known.contains(path) {
                continue;
            }
            let index = LineIndex::new(text);
            let errors = stylet_syntax::parse(text).errors().to_vec();
            let list = by_file.entry(path.clone()).or_default();
            for e in errors {
                list.push(diagnostic(
                    &index,
                    e.range(),
                    DiagnosticSeverity::ERROR,
                    e.message(),
                ));
            }
        }

        let mut notes = Vec::new();
        let files: HashSet<PathBuf> = by_file.keys().cloned().collect();
        for stale in self.published.difference(&files) {
            by_file.entry(stale.clone()).or_default();
        }
        for (path, diagnostics) in by_file {
            let Some(uri) = path_to_uri(&path) else {
                continue;
            };
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            };
            notes.push(Notification::new(
                PublishDiagnostics::METHOD.to_string(),
                params,
            ));
        }
        self.published = files;
        notes
    }

    pub fn request(&mut self, request: Request) -> Response {
        let id = request.id.clone();
        let result = match request.method.as_str() {
            Formatting::METHOD => cast::<Formatting>(request).map(|(_, p)| {
                serde_json::to_value(self.format(&p.text_document.uri)).unwrap_or_default()
            }),
            GotoDefinition::METHOD => cast::<GotoDefinition>(request).map(|(_, p)| {
                let at = p.text_document_position_params;
                serde_json::to_value(self.definition(&at.text_document.uri, at.position))
                    .unwrap_or_default()
            }),
            References::METHOD => cast::<References>(request).map(|(_, p)| {
                let at = p.text_document_position;
                let refs = self.references(
                    &at.text_document.uri,
                    at.position,
                    p.context.include_declaration,
                );
                serde_json::to_value(refs).unwrap_or_default()
            }),
            DocumentSymbolRequest::METHOD => {
                cast::<DocumentSymbolRequest>(request).map(|(_, p)| {
                    serde_json::to_value(self.symbols(&p.text_document.uri)).unwrap_or_default()
                })
            }
            Completion::METHOD => cast::<Completion>(request).map(|(_, p)| {
                let at = p.text_document_position;
                serde_json::to_value(self.completion(&at.text_document.uri, at.position))
                    .unwrap_or_default()
            }),
            _ => return method_not_found(id),
        };
        match result {
            Ok(value) => Response::new_ok(id, value),
            Err(e) => Response::new_err(
                id,
                lsp_server::ErrorCode::InvalidParams as i32,
                e.to_string(),
            ),
        }
    }

    fn format(&self, uri: &Uri) -> Option<Vec<TextEdit>> {
        let path = uri_to_path(uri)?;
        let text = self.text(&path)?;
        let formatted = stylet_fmt::format(&text, &self.settings.fmt).ok()?;
        if formatted == text {
            return Some(Vec::new());
        }
        let index = LineIndex::new(&text);
        let whole = TextRange::up_to((text.len() as u32).into());
        Some(vec![TextEdit {
            range: range(&index, whole),
            new_text: formatted,
        }])
    }

    /// The token under the cursor, with the document's path and parse.
    fn token_at(&self, uri: &Uri, position: lsp_types::Position) -> Option<(PathBuf, SyntaxToken)> {
        let path = uri_to_path(uri)?;
        let text = self.text(&path)?;
        let root = stylet_syntax::parse(&text).syntax();
        let at = offset(&text, position)?;
        let token = root.token_at_offset(at.into()).right_biased()?;
        Some((path, token))
    }

    fn definition(
        &self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<GotoDefinitionResponse> {
        let (path, token) = self.token_at(uri, position)?;
        match token.kind() {
            STRING | URL => {
                let import = token.parent_ancestors().find_map(Import::cast)?;
                let spec = import.path()?;
                let loader = Loader::new(self.fs.clone(), self.settings.resolve.clone());
                let target = loader.resolve(&spec, &path).ok()?;
                let uri = path_to_uri(&target)?;
                let start = lsp_types::Position::new(0, 0);
                Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: lsp_types::Range::new(start, start),
                }))
            }
            PLACEHOLDER_NAME => {
                let definitions = self.placeholder_locations(token.text(), true, false);
                (!definitions.is_empty()).then_some(GotoDefinitionResponse::Array(definitions))
            }
            _ => None,
        }
    }

    fn references(
        &self,
        uri: &Uri,
        position: lsp_types::Position,
        declaration: bool,
    ) -> Option<Vec<Location>> {
        let (_, token) = self.token_at(uri, position)?;
        (token.kind() == PLACEHOLDER_NAME)
            .then(|| self.placeholder_locations(token.text(), declaration, true))
    }

    /// Definitions and/or `@extend`s of placeholder `name` in all known and open files.
    fn placeholder_locations(&self, name: &str, definitions: bool, extends: bool) -> Vec<Location> {
        let mut out = Vec::new();
        for path in self.project_files() {
            let Some(text) = self.text(&path) else {
                continue;
            };
            let Some(uri) = path_to_uri(&path) else {
                continue;
            };
            let index = LineIndex::new(&text);
            let root = stylet_syntax::parse(&text).syntax();
            for token in root
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
            {
                if token.kind() != PLACEHOLDER_NAME || token.text() != name {
                    continue;
                }
                let in_extend = token.parent_ancestors().any(|n| n.kind() == EXTEND);
                if (in_extend && extends) || (!in_extend && definitions) {
                    out.push(Location {
                        uri: uri.clone(),
                        range: range(&index, token.text_range()),
                    });
                }
            }
        }
        out
    }

    /// Known and open files, sorted.
    fn project_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .known
            .iter()
            .chain(self.fs.docs.keys())
            .cloned()
            .collect();
        files.sort();
        files.dedup();
        files
    }

    fn definitions(&mut self, path: &Path) -> Option<Rc<Definitions>> {
        let text = self.text(path)?;
        let mut hasher = std::hash::DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some((h, defs)) = self.definitions.get(path)
            && *h == hash
        {
            return Some(defs.clone());
        }
        let defs = Rc::new(Definitions::of(&stylet_syntax::parse(&text).syntax()));
        self.definitions
            .insert(path.to_path_buf(), (hash, defs.clone()));
        Some(defs)
    }

    fn completion(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<Vec<CompletionItem>> {
        let path = uri_to_path(uri)?;
        let text = self.text(&path)?;
        let at = offset(&text, position)? as usize;
        let root = stylet_syntax::parse(&text).syntax();
        let in_block = root
            .token_at_offset((at as u32).into())
            .left_biased()
            .is_some_and(|t| t.parent_ancestors().any(|n| n.kind() == BLOCK));
        let (context, start) = complete::context(&text, at, in_block);
        let index = LineIndex::new(&text);
        let replace = range(
            &index,
            TextRange::new((start as u32).into(), (at as u32).into()),
        );
        let typed = &text[start..at];

        let mut defs = Vec::new();
        for file in self.project_files() {
            defs.extend(self.definitions(&file));
        }
        let mut seen = HashSet::new();
        let mut custom_properties = Vec::new();
        for (name, value) in defs.iter().flat_map(|d| &d.custom_properties) {
            if seen.insert(name.as_str()) {
                custom_properties.push((name.as_str(), value.as_str()));
            }
        }

        let item = |label: String, kind: CompletionItemKind, new_text: String| CompletionItem {
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace,
                new_text,
            })),
            label,
            kind: Some(kind),
            ..CompletionItem::default()
        };
        let mut items = Vec::new();
        match context {
            complete::Context::Property => {
                let suggest = lsp_types::Command::new(
                    "Suggest".into(),
                    "editor.action.triggerSuggest".into(),
                    None,
                );
                for p in complete::properties() {
                    items.push(CompletionItem {
                        detail: Some(p.syntax.to_string()),
                        documentation: (!p.href.is_empty()).then(|| {
                            lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                                kind: lsp_types::MarkupKind::Markdown,
                                value: format!("[Specification]({})", p.href),
                            })
                        }),
                        // Vendor-prefixed properties after the standard ones.
                        sort_text: Some(format!("{}{}", u8::from(p.name.starts_with('-')), p.name)),
                        command: self.suggest_values.then(|| suggest.clone()),
                        ..item(
                            p.name.into(),
                            CompletionItemKind::PROPERTY,
                            format!("{}: ", p.name),
                        )
                    });
                }
                for (name, value) in &custom_properties {
                    items.push(CompletionItem {
                        detail: (!value.is_empty()).then(|| value.to_string()),
                        ..item(
                            name.to_string(),
                            CompletionItemKind::VARIABLE,
                            format!("{name}: "),
                        )
                    });
                }
            }
            complete::Context::Value { property, in_var } => {
                if !in_var {
                    let keywords = complete::property(&property)
                        .map(|p| p.keywords.as_slice())
                        .unwrap_or_default();
                    let mut seen = HashSet::new();
                    for keyword in keywords.iter().chain(complete::GLOBAL_KEYWORDS) {
                        if seen.insert(*keyword) {
                            items.push(item(
                                keyword.to_string(),
                                CompletionItemKind::VALUE,
                                keyword.to_string(),
                            ));
                        }
                    }
                }
                for (name, value) in &custom_properties {
                    let label = if in_var {
                        name.to_string()
                    } else {
                        format!("var({name})")
                    };
                    items.push(CompletionItem {
                        detail: (!value.is_empty()).then(|| value.to_string()),
                        // Typing `--x` finds `var(--x)`.
                        filter_text: typed.starts_with('-').then(|| name.to_string()),
                        sort_text: Some(format!("0{name}")),
                        ..item(label.clone(), CompletionItemKind::VARIABLE, label)
                    });
                }
            }
            complete::Context::Extend => {
                let mut seen = HashSet::new();
                for name in defs.iter().flat_map(|d| &d.placeholders) {
                    if seen.insert(name) {
                        items.push(item(name.clone(), CompletionItemKind::CLASS, name.clone()));
                    }
                }
            }
            complete::Context::CustomMedia => {
                let mut seen = HashSet::new();
                for (name, query) in defs.iter().flat_map(|d| &d.custom_media) {
                    if seen.insert(name) {
                        items.push(CompletionItem {
                            detail: Some(query.clone()),
                            ..item(name.clone(), CompletionItemKind::CONSTANT, name.clone())
                        });
                    }
                }
            }
            complete::Context::None => return None,
        }
        Some(items)
    }

    fn symbols(&self, uri: &Uri) -> Option<DocumentSymbolResponse> {
        let path = uri_to_path(uri)?;
        let text = self.text(&path)?;
        let index = LineIndex::new(&text);
        let root = stylet_syntax::parse(&text).syntax();
        Some(DocumentSymbolResponse::Nested(symbols(&root, &index)))
    }
}

fn diagnostic(
    index: &LineIndex,
    at: TextRange,
    severity: DiagnosticSeverity,
    message: &str,
) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: range(index, at),
        severity: Some(severity),
        source: Some("stylet".into()),
        message: message.to_string(),
        ..lsp_types::Diagnostic::default()
    }
}

#[allow(deprecated)] // `DocumentSymbol::deprecated` must be set.
fn symbols(parent: &SyntaxNode, index: &LineIndex) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    for child in parent.children() {
        let node = match child.kind() {
            BLOCK => {
                out.extend(symbols(&child, index));
                continue;
            }
            _ => child,
        };
        let Some(item) = Item::cast(node.clone()) else {
            continue;
        };
        let (name, kind, selection, block) = match &item {
            Item::Rule(rule) => {
                let Some(selector) = rule.selector() else {
                    continue;
                };
                let name = selector.syntax().text().to_string();
                (
                    name,
                    SymbolKind::CLASS,
                    selector.syntax().text_range(),
                    rule.block(),
                )
            }
            Item::Placeholder(p) => {
                let Some(name) = p.name() else { continue };
                (
                    name.text().to_string(),
                    SymbolKind::INTERFACE,
                    name.text_range(),
                    p.block(),
                )
            }
            Item::AtRule(rule) if rule.block().is_some() => {
                let prelude = rule
                    .prelude()
                    .map(|p| p.syntax().text().to_string())
                    .unwrap_or_default();
                let name = format!("@{} {prelude}", rule.name()).trim().to_string();
                (name, SymbolKind::NAMESPACE, node.text_range(), rule.block())
            }
            _ => continue,
        };
        let children = block
            .map(|b| symbols(b.syntax(), index))
            .unwrap_or_default();
        out.push(DocumentSymbol {
            name: name.split_whitespace().collect::<Vec<_>>().join(" "),
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range: range(index, node.text_range()),
            selection_range: range(index, selection),
            children: Some(children),
        });
    }
    out
}

fn cast<R: lsp_types::request::Request>(
    request: Request,
) -> Result<(RequestId, R::Params), ExtractError<Request>> {
    request.extract(R::METHOD)
}

fn cast_notification<N: lsp_types::notification::Notification>(
    n: Notification,
) -> Result<N::Params, Notification> {
    match n.extract(N::METHOD) {
        Ok(params) => Ok(params),
        Err(ExtractError::MethodMismatch(n)) => Err(n),
        Err(ExtractError::JsonError { .. }) => Err(Notification::new(String::new(), ())),
    }
}

fn method_not_found(id: RequestId) -> Response {
    Response::new_err(
        id,
        lsp_server::ErrorCode::MethodNotFound as i32,
        "unsupported request".into(),
    )
}
