//! Static Stylus interpreter producing stylet output per source file.

use crate::builtins;
use crate::expr::{self, Arg, Expr};
use crate::out::Out;
use crate::parse::{self, AssignOp, Branch, Stmt, StmtKind};
use crate::value::{Number, Value};
use crate::{Options, VarMode, Warning};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use stylet_resolve::{FileSystem, normalize};

/// Standard properties that mixin libraries redefine to add vendor prefixes.
const CSS_PROPERTIES: &[&str] = &[
    "transition",
    "columns",
    "border-radius",
    "box-shadow",
    "box-sizing",
    "opacity",
    "transform",
    "animation",
    "user-select",
    "appearance",
    "filter",
    "background-size",
    "background-clip",
    "backface-visibility",
    "perspective",
    "hyphens",
    "text-shadow",
    "column-count",
    "column-gap",
];

/// Stylus settings that are variables but not styles.
const SETTINGS: &[&str] = &["vendors"];

pub struct Def {
    name: String,
    /// From a `--preload` library.
    preloaded: bool,
    params: Vec<(String, Option<Expr>, bool)>,
    body: Rc<Vec<Stmt>>,
    file: PathBuf,
}

/// A block scope: Stylus scopes variables and mixins to the block (including
/// files imported inside it).
#[derive(Default)]
struct Scope {
    vars: HashMap<String, Value>,
    defs: HashMap<String, Rc<Def>>,
}

pub struct Source {
    pub text: String,
    pub stmts: Rc<Vec<Stmt>>,
}

/// Where a value ends up, for deciding which globals become custom properties.
#[derive(Default)]
pub struct Usage {
    /// Globals used in declaration values.
    pub in_declarations: BTreeSet<String>,
    /// Custom properties declared anywhere in the sources (for collision checks).
    pub custom_properties: BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct Ctx {
    /// Statements at the top level of a file (assignments are global).
    top_level: bool,
    /// Inside a style rule (declarations allowed).
    in_rule: bool,
    /// Inside a mixin body (output belongs to the caller).
    in_mixin: bool,
}

/// Result of a function body.
enum Flow {
    Normal,
    Return(Value),
}

pub struct Interp<'a, F> {
    fs: &'a F,
    root: PathBuf,
    options: &'a Options,
    /// Globals eligible to become custom properties (`None` in the first pass: all).
    eligible: Option<&'a BTreeSet<String>>,
    pub usage: Usage,
    globals: HashMap<String, Value>,
    scopes: Vec<Scope>,
    defs: HashMap<String, Rc<Def>>,
    sources: HashMap<PathBuf, Rc<Source>>,
    /// Converted output per file; the first conversion wins.
    pub outputs: BTreeMap<PathBuf, (String, Vec<Out>)>,
    /// Files whose conversion differed between imports.
    conflicts: HashSet<PathBuf>,
    file_stack: Vec<PathBuf>,
    required: HashSet<PathBuf>,
    pub warnings: Vec<Warning>,
    depth: usize,
    /// Running a function body (not a mixin): call statements are expressions.
    in_function: bool,
    /// Last value of an expression statement (function results).
    last_value: Option<Value>,
    /// Evaluating `--preload` files: definitions only, no output.
    preloading: bool,
    /// Nested placeholders moved to the top level of the current file.
    hoisted: Vec<Out>,
    /// Properties set so far in each enclosing rule, for `@prop` lookups.
    properties: Vec<HashMap<String, Value>>,
    /// Mixins being executed (a property named like one isn't a call).
    calling: Vec<String>,
    /// Unknown functions: (warning index, name), to improve messages later.
    unknown_functions: Vec<(usize, String)>,
    /// `--x: $x` declarations dropped because `$x` becomes `--x`.
    pub mirrors: BTreeSet<String>,
    /// (file, line, message) of warnings already reported.
    reported: HashSet<(PathBuf, u32, String)>,
    /// Directories of files imported so far in this entry (Stylus keeps them
    /// searchable for later imports).
    visited_dirs: Vec<PathBuf>,
    /// The file whose output is being built (mixin bodies run in the caller's output).
    output_file: PathBuf,
}

impl<'a, F: FileSystem> Interp<'a, F> {
    pub fn new(
        fs: &'a F,
        root: &Path,
        options: &'a Options,
        eligible: Option<&'a BTreeSet<String>>,
    ) -> Self {
        let mut globals = HashMap::new();
        for (name, value) in &options.defines {
            globals.insert(name.clone(), literal_value(value));
        }
        Self {
            fs,
            root: root.to_path_buf(),
            options,
            eligible,
            usage: Usage::default(),
            globals,
            scopes: Vec::new(),
            defs: HashMap::new(),
            sources: HashMap::new(),
            outputs: BTreeMap::new(),
            conflicts: HashSet::new(),
            file_stack: Vec::new(),
            required: HashSet::new(),
            warnings: Vec::new(),
            depth: 0,
            in_function: false,
            last_value: None,
            preloading: false,
            hoisted: Vec::new(),
            properties: Vec::new(),
            calling: Vec::new(),
            unknown_functions: Vec::new(),
            mirrors: BTreeSet::new(),
            reported: HashSet::new(),
            visited_dirs: Vec::new(),
            output_file: PathBuf::new(),
        }
    }

    /// Migrates one entry and everything it imports. State (globals, mixins)
    /// starts fresh for each entry, like a separate Stylus compilation.
    pub fn entry(&mut self, path: &Path) {
        self.globals
            .retain(|name, _| self.options.defines.iter().any(|(n, _)| n == name));
        self.defs.clear();
        self.required.clear();
        self.visited_dirs.clear();
        self.preloading = true;
        for preload in &self.options.preload {
            self.file(preload);
        }
        self.preloading = false;
        self.file(path);
        for (index, name) in std::mem::take(&mut self.unknown_functions) {
            if self.find_def(&name).is_some()
                && let Some(w) = self.warnings.get_mut(index)
            {
                w.message = format!(
                    "`{name}()` is used before it is defined, so Stylus outputs it literally (probably a bug); kept as is"
                );
                w.category = "used-before-defined";
            }
        }
    }

    fn warn(&mut self, line: u32, category: &'static str, message: impl Into<String>) {
        let path = self.file_stack.last().cloned().unwrap_or_default();
        let message = message.into();
        if self.reported.insert((path.clone(), line, message.clone())) {
            self.warnings.push(Warning {
                path,
                line,
                category,
                message,
            });
        }
    }

    fn source(&mut self, path: &Path) -> Option<Rc<Source>> {
        if let Some(source) = self.sources.get(path) {
            return Some(source.clone());
        }
        let text = self.fs.read_to_string(path).ok()?;
        let stmts = Rc::new(parse::parse(&text));
        let source = Rc::new(Source { text, stmts });
        self.sources.insert(path.to_path_buf(), source.clone());
        Some(source)
    }

    fn file(&mut self, path: &Path) {
        let Some(source) = self.source(path) else {
            return self.warn(0, "import", format!("can't read {}", path.display()));
        };
        if self.file_stack.iter().any(|p| p == path) {
            return self.warn(0, "import", "import cycle");
        }
        self.file_stack.push(path.to_path_buf());
        if let Some(dir) = path.parent() {
            self.visited_dirs.retain(|d| d != dir);
            self.visited_dirs.push(dir.to_path_buf());
        }
        let saved_hoisted = std::mem::take(&mut self.hoisted);
        let saved_output = std::mem::replace(&mut self.output_file, path.to_path_buf());
        let mut out = Vec::new();
        let ctx = Ctx {
            top_level: true,
            in_rule: false,
            in_mixin: false,
        };
        self.stmts(&source.stmts, &mut out, ctx);
        self.hoisted = saved_hoisted;
        self.output_file = saved_output;
        self.file_stack.pop();
        if self.preloading {
            return;
        }

        // Stylus includes a file at every import, so its last copy wins in the
        // cascade; stylet includes it once, so keep the last version.
        let rendered = crate::out::render(&out);
        if let Some((previous, _)) = self.outputs.get(path)
            && *previous != rendered
            && self.conflicts.insert(path.to_path_buf())
        {
            self.file_stack.push(path.to_path_buf());
            self.warn(
                0,
                "import-dependent",
                "the file converts differently depending on where it is imported (import order or per env); kept the last version",
            );
            self.file_stack.pop();
        }
        self.outputs.insert(path.to_path_buf(), (rendered, out));
    }

    fn current_file(&self) -> PathBuf {
        self.file_stack.last().cloned().unwrap_or_default()
    }

    /// Original source lines of a statement, as `// stylet-migrate:` comments.
    fn commented(&mut self, stmt: &Stmt, ctx: Ctx, out: &mut Vec<Out>) {
        if ctx.in_mixin {
            out.push(Out::Comment(
                "// stylet-migrate: part of a mixin was not migrated".into(),
            ));
            return;
        }
        let path = self.current_file();
        let Some(source) = self.sources.get(&path) else {
            return;
        };
        let lines: Vec<&str> = source.text.lines().collect();
        let start = stmt.line.saturating_sub(1) as usize;
        let end = (stmt.end as usize).min(lines.len());
        let indent = lines[start..end]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        for line in &lines[start..end] {
            let text = line.get(indent..).unwrap_or(line.trim_start());
            out.push(Out::Comment(
                format!("// stylet-migrate: {text}").trim_end().to_string(),
            ));
        }
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.vars.get(name) {
                return Some(v.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    fn find_def(&self, name: &str) -> Option<Rc<Def>> {
        for scope in self.scopes.iter().rev() {
            if let Some(def) = scope.defs.get(name) {
                return Some(def.clone());
            }
        }
        self.defs.get(name).cloned()
    }

    fn define(&mut self, def: Def) {
        let def = Rc::new(def);
        match self.scopes.last_mut() {
            Some(scope) => scope.defs.insert(def.name.clone(), def),
            None => self.defs.insert(def.name.clone(), def),
        };
    }

    fn assign_local(&mut self, name: &str, value: Value) {
        match self.scopes.last_mut() {
            Some(scope) => {
                scope.vars.insert(name.to_string(), value);
            }
            None => {
                self.globals.insert(name.to_string(), value);
            }
        }
    }

    fn prop_name(&self, var: &str) -> String {
        format!(
            "--{}{}",
            self.options.var_prefix,
            var.trim_start_matches('$')
        )
    }

    fn stmts(&mut self, stmts: &[Stmt], out: &mut Vec<Out>, ctx: Ctx) -> Flow {
        for stmt in stmts {
            let before = out.len();
            let flow = self.stmt(stmt, out, ctx);
            if ctx.top_level && !ctx.in_mixin && !self.hoisted.is_empty() {
                let hoisted = std::mem::take(&mut self.hoisted);
                out.splice(before..before, hoisted);
            }
            if let Flow::Return(v) = flow {
                return Flow::Return(v);
            }
        }
        Flow::Normal
    }

    fn stmt(&mut self, stmt: &Stmt, out: &mut Vec<Out>, ctx: Ctx) -> Flow {
        let line = stmt.line;
        match &stmt.kind {
            StmtKind::Comment(text) => out.push(Out::Comment(text.clone())),
            StmtKind::Import { path, require } => self.import(stmt, path, *require, None, out, ctx),
            StmtKind::Extend(targets) => self.extend(stmt, targets, out, ctx),
            StmtKind::Rule { selectors, body }
                if !ctx.top_level && selectors.len() == 1 && is_placeholder(&selectors[0]) =>
            {
                let mut inner = Vec::new();
                self.scopes.push(Scope::default());
                let inner_ctx = Ctx {
                    top_level: false,
                    in_rule: true,
                    ..ctx
                };
                self.stmts(body, &mut inner, inner_ctx);
                self.scopes.pop();
                self.hoisted.push(Out::Rule {
                    selectors: selectors.clone(),
                    body: inner,
                });
            }
            StmtKind::Rule { selectors, body } => {
                let selectors: Vec<String> = selectors
                    .iter()
                    .flat_map(|s| {
                        s.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .map(|s| self.interpolate(s, line))
                    .collect();
                // `/selector`: a root selector, outside of any nesting.
                let (root, selectors): (Vec<String>, Vec<String>) = if ctx.in_rule {
                    selectors
                        .into_iter()
                        .partition(|s| s.starts_with('/') && !s.starts_with("//"))
                } else {
                    (Vec::new(), selectors)
                };
                let root: Vec<String> = root
                    .iter()
                    .map(|s| s[1..].trim_start().to_string())
                    .collect();
                let mut inner = Vec::new();
                self.scopes.push(Scope::default());
                self.properties.push(HashMap::new());
                let flow = self.stmts(
                    body,
                    &mut inner,
                    Ctx {
                        top_level: false,
                        in_rule: true,
                        ..ctx
                    },
                );
                self.scopes.pop();
                self.properties.pop();
                if !root.is_empty() {
                    self.hoisted.push(Out::Rule {
                        selectors: root,
                        body: inner.clone(),
                    });
                }
                if !selectors.is_empty() {
                    out.push(Out::Rule {
                        selectors,
                        body: inner,
                    });
                }
                if let Flow::Return(v) = flow {
                    return Flow::Return(v);
                }
            }
            StmtKind::Property {
                name,
                value,
                comment,
            } => self.property(stmt, name, value, comment.clone(), out, ctx),
            StmtKind::Assign { name, op, value } => self.assign(stmt, name, *op, value, out, ctx),
            StmtKind::AssignBlock { name, body } => {
                let value = Value::Block(Rc::new(body.clone()));
                if ctx.top_level && !ctx.in_mixin {
                    self.globals.insert(name.clone(), value);
                } else {
                    self.assign_local(name, value);
                }
            }
            StmtKind::Def { name, params, body } => match expr::parse_params(params) {
                Ok(params) => {
                    let def = Def {
                        name: name.clone(),
                        params,
                        body: Rc::new(body.clone()),
                        file: self.current_file(),
                        preloaded: self.preloading,
                    };
                    self.define(def);
                }
                Err(e) => {
                    self.warn(
                        line,
                        "definition",
                        format!("can't parse the parameters of `{name}`: {e}"),
                    );
                    self.commented(stmt, ctx, out);
                }
            },
            StmtKind::Call { name, args } => {
                let text = format!("{name}({args})");
                match expr::parse(&text) {
                    Ok(call @ Expr::Call { .. }) if self.in_function => {
                        self.last_value = Some(self.eval(&call, line, false));
                    }
                    Ok(Expr::Call { name, args }) => {
                        self.call_statement(stmt, &name, &args, out, ctx)
                    }
                    Ok(_) | Err(_) => {
                        self.warn(line, "expression", format!("can't parse `{text}`"));
                        self.commented(stmt, ctx, out);
                    }
                }
            }
            StmtKind::If { branches } => return self.conditional(stmt, branches, out, ctx),
            StmtKind::For {
                vars,
                iterable,
                body,
            } => {
                if !self.options.unroll_loops {
                    self.warn(
                        line,
                        "loop",
                        "loops aren't migrated; consider CSS custom properties (or --unroll-loops)",
                    );
                    self.commented(stmt, ctx, out);
                    return Flow::Normal;
                }
                let items = match self.eval_str(iterable, line) {
                    Some(Value::Hash(pairs)) => pairs
                        .into_iter()
                        .map(|(k, v)| (Value::Str { s: k, quote: None }, v))
                        .collect::<Vec<_>>(),
                    Some(v) => v
                        .items()
                        .into_iter()
                        .enumerate()
                        .map(|(i, item)| (item, Value::Number(Number::new(i as f64, ""))))
                        .collect(),
                    None => {
                        self.commented(stmt, ctx, out);
                        return Flow::Normal;
                    }
                };
                for (first, second) in items {
                    if let Some(name) = vars.first() {
                        self.assign_local(name, first);
                    }
                    if let Some(name) = vars.get(1) {
                        self.assign_local(name, second);
                    }
                    if let Flow::Return(v) = self.stmts(body, out, ctx) {
                        return Flow::Return(v);
                    }
                }
            }
            StmtKind::Return(expr) => {
                let (expr, postfix) = split_postfix(expr);
                if let Some((negate, condition)) = postfix
                    && self
                        .condition(condition, line)
                        .is_none_or(|taken| taken == negate)
                {
                    return Flow::Normal;
                }
                let value = self.eval_str(expr, line).unwrap_or(Value::Null);
                return Flow::Return(value);
            }
            StmtKind::AtRule {
                name,
                prelude,
                body,
            } => self.at_rule(stmt, name, prelude, body.as_deref(), out, ctx),
            StmtKind::Css(css) => out.push(Out::Raw(css.trim().to_string())),
            StmtKind::Expr(text) => self.expression_statement(stmt, text, out, ctx),
            StmtKind::Unknown { reason, .. } => {
                self.warn(line, "syntax", reason.clone());
                self.commented(stmt, ctx, out);
            }
        }
        Flow::Normal
    }

    fn import(
        &mut self,
        stmt: &Stmt,
        spec: &str,
        require: bool,
        layer: Option<&str>,
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) {
        let _ = ctx;
        let path = match expr::parse(spec).map(|e| self.eval(&e, stmt.line, false)) {
            Ok(v) => v.text(false),
            Err(_) => spec.trim_matches(['\'', '"']).to_string(),
        };
        let layer = layer.map(str::to_string);
        if path.starts_with("http") || path.starts_with("//") {
            return out.push(Out::Import { path, layer });
        }
        if path.ends_with(".css") {
            let found = self.lookup_file(&path);
            let path = match found {
                Some(found) if self.stylet_resolve(&path).as_ref() != Some(&found) => {
                    self.root_relative(&found, false)
                }
                Some(_) => path,
                None => {
                    self.warn(stmt.line, "import", format!("can't find `{path}`"));
                    path
                }
            };
            return out.push(Out::Import { path, layer });
        }
        let Some(resolved) = self.resolve(&path) else {
            self.warn(stmt.line, "import", format!("can't find `{path}`"));
            return out.push(Out::Import { path, layer });
        };
        // stylet resolves only relative to the file or the root: rewrite
        // imports that Stylus found through its import-chain lookup.
        let path = if self.stylet_resolve(&path).as_ref() == Some(&resolved) {
            path
        } else {
            self.root_relative(&resolved, true)
        };
        out.push(Out::Import { path, layer });
        if require && !self.required.insert(resolved.clone()) {
            return;
        }
        self.required.insert(resolved.clone());
        self.file(&resolved);
    }

    /// `/path` from the root, without `.styl` / `/index.styl` if `strip`.
    fn root_relative(&self, path: &Path, strip: bool) -> String {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        let mut out = format!("/{}", relative.display());
        if strip {
            if let Some(dir) = out.strip_suffix("/index.styl") {
                out = dir.to_string();
            } else if let Some(stem) = out.strip_suffix(".styl") {
                out = stem.to_string();
            }
        }
        out
    }

    /// Directories Stylus searches, most specific first.
    fn lookup_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .file_stack
            .iter()
            .rev()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        dirs.push(self.root.clone());
        for dir in self.visited_dirs.iter().rev() {
            if !dirs.contains(dir) {
                dirs.push(dir.clone());
            }
        }
        dirs
    }

    /// A file (asset or CSS) found through the Stylus lookup directories.
    fn lookup_file(&self, spec: &str) -> Option<PathBuf> {
        let spec = spec.trim_start_matches('/');
        self.lookup_dirs()
            .into_iter()
            .map(|d| normalize(&d.join(spec)))
            .find(|c| self.fs.is_file(c))
    }

    /// Stylus lookup: relative to the importing file, then to each file up the
    /// import chain, then to the root; `x`, `x.styl`, `x/index.styl`.
    fn resolve(&self, spec: &str) -> Option<PathBuf> {
        let spec = spec.trim_start_matches('/');
        let dirs = self.lookup_dirs();
        let with_ext = if spec.ends_with(".styl") {
            spec.to_string()
        } else {
            format!("{spec}.styl")
        };
        let found = |candidate: PathBuf| {
            let candidate = normalize(&candidate);
            self.fs.is_file(&candidate).then_some(candidate)
        };
        let base_name = Path::new(spec)
            .file_name()
            .map(|n| format!("{}.styl", n.to_string_lossy()));
        dirs.iter()
            .find_map(|d| found(d.join(&with_ext)))
            .or_else(|| {
                dirs.iter()
                    .find_map(|d| found(d.join(spec).join("index.styl")))
            })
            .or_else(|| {
                let base_name = base_name.as_ref()?;
                dirs.iter()
                    .find_map(|d| found(d.join(spec).join(base_name)))
            })
    }

    /// Where stylet's own resolution would find `spec` from the current file.
    fn stylet_resolve(&self, spec: &str) -> Option<PathBuf> {
        let base = match spec.strip_prefix('/') {
            Some(rest) => self.root.join(rest),
            None => self
                .current_file()
                .parent()
                .unwrap_or(Path::new(""))
                .join(spec),
        };
        let mut candidates = vec![
            PathBuf::from(format!("{}.styl", base.display())),
            base.join("index.styl"),
        ];
        if spec.ends_with(".styl") {
            candidates.insert(0, base);
        }
        candidates
            .into_iter()
            .map(|c| normalize(&c))
            .find(|c| self.fs.is_file(c))
    }

    fn extend(&mut self, stmt: &Stmt, targets: &str, out: &mut Vec<Out>, ctx: Ctx) {
        let targets: Vec<String> = targets
            .split(',')
            .map(|t| self.interpolate(t.trim(), stmt.line))
            .collect();
        if targets.iter().all(|t| t.starts_with('$')) {
            out.push(Out::Extend(targets.join(", ")));
        } else {
            self.warn(
                stmt.line,
                "extend-selector",
                format!("`@extend {}`: only placeholders can be extended; turn the target into a placeholder", targets.join(", ")),
            );
            self.commented(stmt, ctx, out);
        }
    }

    fn property(
        &mut self,
        stmt: &Stmt,
        name: &str,
        value: &str,
        comment: Option<String>,
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) {
        let line = stmt.line;
        let name = self.interpolate(name, line);
        if name.starts_with("--") {
            self.usage.custom_properties.insert(name.clone());
        }
        let (value, postfix) = split_postfix(value);
        if let Some((negate, condition)) = postfix {
            match self.condition(condition, line) {
                Some(taken) if taken != negate => {}
                Some(_) => return,
                None => return self.commented(stmt, ctx, out),
            }
        }
        // Transparent mixin: `name: args` calls mixin `name`, except inside
        // itself and for library mixins that only add vendor prefixes to a
        // standard property (e.g. axis' `transition`).
        if !name.starts_with("--")
            && self
                .find_def(&name)
                .is_some_and(|d| !(d.preloaded && CSS_PROPERTIES.contains(&name.as_str())))
            && !self.calling.contains(&name)
            && let Ok(e) = expr::parse(value)
        {
            let args: Vec<Arg> = match e {
                Expr::List { items, .. } => items
                    .into_iter()
                    .map(|value| Arg { name: None, value })
                    .collect(),
                other => vec![Arg {
                    name: None,
                    value: other,
                }],
            };
            return self.call_statement(stmt, &name, &args, out, ctx);
        }
        let css = if let Some(inner) = value.strip_prefix("@css").map(str::trim) {
            inner
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim()
                .to_string()
        } else if value.is_empty() {
            String::new()
        } else {
            match expr::parse(value) {
                Ok(e) => {
                    // `--x: $x` mirrors a variable that becomes `--x` itself.
                    if let Expr::Ident(var) = &e
                        && self.options.vars == VarMode::Props
                        && self.prop_name(var) == name
                        && matches!(self.lookup(var), Some(Value::Tracked { .. }))
                    {
                        self.mirrors.insert(name);
                        return;
                    }
                    let v = self.eval(&e, line, true);
                    self.record_declaration_usage(&v);
                    if has_relative_url(&v.css(false)) {
                        self.warn(
                            line,
                            "url",
                            "relative `url()`: Stylus left it relative to the output CSS, stylet rebases it from this file; check the path",
                        );
                    }
                    if let Some(props) = self.properties.last_mut() {
                        props.insert(name.clone(), v.clone());
                    }
                    v.css(self.options.vars == VarMode::Props)
                }
                Err(err) => {
                    self.warn(
                        line,
                        "expression",
                        format!("can't evaluate `{value}` ({err}); kept as is"),
                    );
                    value.to_string()
                }
            }
        };
        let _ = ctx;
        out.push(Out::Declaration {
            name,
            value: css,
            comment,
        });
    }

    fn record_declaration_usage(&mut self, value: &Value) {
        let mut vars = Vec::new();
        tracked_vars(value, &mut vars);
        self.usage.in_declarations.extend(vars);
    }

    fn assign(
        &mut self,
        stmt: &Stmt,
        name: &str,
        op: AssignOp,
        value: &str,
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) {
        let line = stmt.line;
        let (value, postfix) = split_postfix(value);
        if let Some((negate, condition)) = postfix
            && self
                .condition(condition, line)
                .is_none_or(|taken| taken == negate)
        {
            return;
        }
        if op == AssignOp::Default && self.lookup(name).is_some() {
            return;
        }
        let Some(mut v) = self.eval_str(value, line) else {
            return self.commented(stmt, ctx, out);
        };
        if let AssignOp::Compound(op) = op {
            let current = self.lookup(name).unwrap_or(Value::Null);
            v = builtins::binary(&op.to_string(), &current, &v).unwrap_or(v);
        }
        let global = ctx.top_level && !ctx.in_mixin && self.scopes.is_empty();
        if !global {
            return self.assign_local(name, v);
        }
        let serializable = is_serializable(&v) && !SETTINGS.contains(&name);
        let eligible = !self.preloading && self.eligible.is_none_or(|e| e.contains(name));
        if self.options.vars == VarMode::Props && serializable && eligible {
            let prop = self.prop_name(name);
            out.push(Out::RootVar {
                name: prop.clone(),
                value: v.css(true),
            });
            v = Value::Tracked {
                css: format!("var({prop})"),
                value: Box::new(v.literal().clone()),
                vars: vec![name.to_string()],
            };
        }
        self.globals.insert(name.to_string(), v);
    }

    fn conditional(
        &mut self,
        stmt: &Stmt,
        branches: &[Branch],
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) -> Flow {
        for branch in branches {
            let taken = match &branch.condition {
                None => true,
                Some(condition) => match self.condition(condition, stmt.line) {
                    Some(value) => value != branch.negate,
                    None => {
                        self.commented(stmt, ctx, out);
                        return Flow::Normal;
                    }
                },
            };
            if taken {
                return self.stmts(&branch.body, out, ctx);
            }
        }
        Flow::Normal
    }

    /// Evaluates a condition; `None` if it can't be decided statically.
    fn condition(&mut self, condition: &str, line: u32) -> Option<bool> {
        let e = match expr::parse(condition) {
            Ok(e) => e,
            Err(err) => {
                self.warn(
                    line,
                    "condition",
                    format!("can't parse condition `{condition}`: {err}"),
                );
                return None;
            }
        };
        let bare = match &e {
            Expr::Ident(name) => Some(name),
            Expr::Paren(inner) => match inner.as_ref() {
                Expr::Ident(name) => Some(name),
                _ => None,
            },
            _ => None,
        };
        if let Some(name) = bare
            && self.lookup(name).is_none()
        {
            self.warn(
                line,
                "condition",
                format!("condition on `{name}`, which isn't defined in Stylus (injected from JS?); use --define {name}=…"),
            );
            return None;
        }
        Some(self.eval(&e, line, false).truthy())
    }

    fn at_rule(
        &mut self,
        stmt: &Stmt,
        name: &str,
        prelude: &str,
        body: Option<&[Stmt]>,
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) {
        let line = stmt.line;
        let prelude = self.substitute_prelude(prelude, line);
        let Some(body) = body else {
            return out.push(Out::AtRule {
                name: name.to_string(),
                prelude,
                body: None,
            });
        };
        // `@layer x` containing only imports → `@import '…' layer(x)`.
        if name == "layer"
            && !prelude.is_empty()
            && body
                .iter()
                .all(|s| matches!(s.kind, StmtKind::Import { .. } | StmtKind::Comment(_)))
        {
            self.scopes.push(Scope::default());
            for s in body {
                match &s.kind {
                    StmtKind::Import { path, require } => {
                        self.import(s, path, *require, Some(&prelude), out, ctx)
                    }
                    StmtKind::Comment(c) => out.push(Out::Comment(c.clone())),
                    _ => {}
                }
            }
            self.scopes.pop();
            return;
        }
        let mut inner = Vec::new();
        self.scopes.push(Scope::default());
        let keyframes = name.ends_with("keyframes");
        let inner_ctx = Ctx {
            top_level: false,
            in_rule: !keyframes
                && (ctx.in_rule
                    || matches!(name, "font-face" | "page" | "property" | "counter-style")),
            ..ctx
        };
        self.stmts(body, &mut inner, inner_ctx);
        self.scopes.pop();
        let rule = Out::AtRule {
            name: name.to_string(),
            prelude,
            body: Some(inner),
        };
        // Stylus moves nested `@keyframes` to the top level; stylet requires it.
        if keyframes && ctx.in_rule {
            self.hoisted.push(rule);
        } else {
            out.push(rule);
        }
    }

    /// Replaces variables and `{interpolation}` in at-rule preludes with literal values.
    fn substitute_prelude(&mut self, prelude: &str, line: u32) -> String {
        let interpolated = self.interpolate(prelude, line);
        let mut out = String::new();
        let mut word = String::new();
        let flush = |this: &mut Self, word: &mut String, out: &mut String| {
            if !word.is_empty() {
                match this.lookup(word) {
                    Some(v) if !word.starts_with('-') => *out += &v.css(false),
                    _ => *out += word,
                }
                word.clear();
            }
        };
        for c in interpolated.chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '$') {
                word.push(c);
            } else {
                flush(self, &mut word, &mut out);
                out.push(c);
            }
        }
        flush(self, &mut word, &mut out);
        out
    }

    fn expression_statement(&mut self, stmt: &Stmt, text: &str, out: &mut Vec<Out>, ctx: Ctx) {
        let (text, postfix) = split_postfix(text);
        if let Some((negate, condition)) = postfix {
            match self.condition(condition, stmt.line) {
                Some(taken) if taken != negate => {}
                Some(_) => return,
                None => return self.commented(stmt, ctx, out),
            }
        }
        // `{$hash}` / `{block}`: expand into the current block.
        if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            match self.eval_str(inner, stmt.line) {
                Some(Value::Hash(pairs)) => {
                    for (name, value) in pairs {
                        let css = value.css(self.options.vars == VarMode::Props);
                        out.push(Out::Declaration {
                            name,
                            value: css,
                            comment: None,
                        });
                    }
                    return;
                }
                Some(Value::Block(body)) => {
                    self.stmts(&body, out, ctx);
                    return;
                }
                _ => {}
            }
        }
        match self.eval_str(text, stmt.line) {
            Some(value) => self.last_value = Some(value),
            None => self.commented(stmt, ctx, out),
        }
    }

    /// `name(args)` as a statement: a mixin call.
    fn call_statement(
        &mut self,
        stmt: &Stmt,
        name: &str,
        args: &[Arg],
        out: &mut Vec<Out>,
        ctx: Ctx,
    ) {
        let line = stmt.line;
        if let Some(def) = self.find_def(name) {
            let args = self.eval_args(args, line);
            self.invoke(&def, args, out, ctx, true);
            return;
        }
        match name {
            "use" => self.warn(
                line,
                "js-plugin",
                "`use()` loads a JS plugin; its functions aren't available in stylet",
            ),
            "error" | "warn" | "p" => {}
            _ => {
                self.warn(line, "mixin", format!("unknown mixin `{name}()`"));
                self.commented(stmt, ctx, out);
            }
        }
    }

    fn eval_args(&mut self, args: &[Arg], line: u32) -> Vec<(Option<String>, Value)> {
        args.iter()
            .map(|a| (a.name.clone(), self.eval(&a.value, line, false)))
            .collect()
    }

    /// Runs a mixin (`as_mixin`) or function body.
    fn invoke(
        &mut self,
        def: &Def,
        args: Vec<(Option<String>, Value)>,
        out: &mut Vec<Out>,
        ctx: Ctx,
        as_mixin: bool,
    ) -> Value {
        if self.depth > 64 {
            self.warn(0, "mixin", format!("`{}` recurses too deeply", def.name));
            return Value::Null;
        }
        let mut scope = HashMap::new();
        let positional: Vec<Value> = args
            .iter()
            .filter(|(n, _)| n.is_none())
            .map(|(_, v)| v.clone())
            .collect();
        let all: Vec<Value> = args.iter().map(|(_, v)| v.clone()).collect();
        let mut next = 0;
        self.depth += 1;
        self.scopes.push(Scope::default());
        for (name, default, rest) in &def.params {
            let value = if *rest {
                let rest: Vec<Value> = positional[next.min(positional.len())..].to_vec();
                next = positional.len();
                Value::List {
                    items: rest,
                    comma: true,
                }
            } else if let Some((_, v)) = args.iter().find(|(n, _)| n.as_deref() == Some(name)) {
                v.clone()
            } else if next < positional.len() {
                next += 1;
                positional[next - 1].clone()
            } else if let Some(default) = default {
                self.eval(default, 0, false)
            } else {
                Value::Null
            };
            self.scopes
                .last_mut()
                .expect("scope")
                .vars
                .insert(name.clone(), value.clone());
            scope.insert(name.clone(), value);
        }
        let scope_ref = &mut self.scopes.last_mut().expect("scope").vars;
        scope_ref.insert(
            "arguments".into(),
            Value::List {
                items: all,
                comma: false,
            },
        );
        scope_ref.insert("mixin".into(), Value::Bool(as_mixin));
        self.file_stack.push(def.file.clone());
        self.calling.push(def.name.clone());
        let saved_last = self.last_value.take();
        let saved_function = std::mem::replace(&mut self.in_function, !as_mixin);
        let mut sink = Vec::new();
        let target = if as_mixin { out } else { &mut sink };
        let flow = self.stmts(
            &def.body,
            target,
            Ctx {
                top_level: false,
                in_mixin: true,
                ..ctx
            },
        );
        self.in_function = saved_function;
        self.calling.pop();
        self.file_stack.pop();
        self.scopes.pop();
        self.depth -= 1;
        let last = std::mem::replace(&mut self.last_value, saved_last);
        match flow {
            Flow::Return(v) => v,
            Flow::Normal => last.unwrap_or(Value::Null),
        }
    }

    fn eval_str(&mut self, src: &str, line: u32) -> Option<Value> {
        match expr::parse(src) {
            Ok(e) => Some(self.eval(&e, line, false)),
            Err(err) => {
                self.warn(line, "expression", format!("can't parse `{src}`: {err}"));
                None
            }
        }
    }

    /// Evaluates an expression. `property`: a top-level property value, where
    /// `/` is literal unless parenthesized.
    pub fn eval(&mut self, e: &Expr, line: u32, property: bool) -> Value {
        match e {
            Expr::Number(n) => Value::Number(n.clone()),
            Expr::Color(c) => Value::Color(c.clone()),
            Expr::Str(s, q) => Value::Str {
                s: s.clone(),
                quote: Some(*q),
            },
            Expr::Ident(name) => match self.lookup(name) {
                Some(v) => v,
                None if name == "true" => Value::Bool(true),
                None if name == "false" => Value::Bool(false),
                None if name == "null" => Value::Null,
                None => Value::Ident(name.clone()),
            },
            Expr::Raw(r) => Value::Raw(r.clone()),
            Expr::PropertyLookup(p) => {
                match self.properties.iter().rev().find_map(|props| props.get(p)) {
                    Some(v) => v.clone(),
                    None => {
                        self.warn(
                            line,
                            "property-lookup",
                            format!("`@{p}` isn't set in an enclosing rule"),
                        );
                        Value::Raw(format!("@{p}"))
                    }
                }
            }
            Expr::Paren(inner) => self.eval(inner, line, false),
            Expr::List { items, comma } => Value::List {
                items: items.iter().map(|i| self.eval(i, line, property)).collect(),
                comma: *comma,
            },
            Expr::Hash(pairs) => Value::Hash(
                pairs
                    .iter()
                    .map(|(k, v)| (k.clone(), self.eval(v, line, false)))
                    .collect(),
            ),
            Expr::Unary { op, expr } => {
                let v = self.eval(expr, line, property);
                builtins::unary(op, &v)
            }
            Expr::Binary { op, lhs, rhs } => {
                if *op == "/" && property {
                    let l = self.eval(lhs, line, property);
                    let r = self.eval(rhs, line, property);
                    return join_raw(&l, "/", &r);
                }
                if *op == "&&" {
                    let l = self.eval(lhs, line, false);
                    return if l.truthy() {
                        self.eval(rhs, line, false)
                    } else {
                        l
                    };
                }
                if *op == "||" {
                    let l = self.eval(lhs, line, false);
                    return if l.truthy() {
                        l
                    } else {
                        self.eval(rhs, line, false)
                    };
                }
                let l = self.eval(lhs, line, false);
                let r = self.eval(rhs, line, false);
                match builtins::binary(op, &l, &r) {
                    Some(v) => v,
                    None => {
                        if !matches!(op, &"==" | &"!=" | &"<" | &">" | &"<=" | &">=" | &"in") {
                            self.warn(
                                line,
                                "expression",
                                format!(
                                    "can't evaluate `{} {op} {}`; kept as is",
                                    l.css(true),
                                    r.css(true)
                                ),
                            );
                        }
                        join_raw(&l, &format!(" {op} "), &r)
                    }
                }
            }
            Expr::Ternary {
                cond,
                then,
                otherwise,
            } => {
                if self.eval(cond, line, false).truthy() {
                    self.eval(then, line, property)
                } else {
                    self.eval(otherwise, line, property)
                }
            }
            Expr::Index { expr, index } => {
                let v = self.eval(expr, line, false);
                let i = self.eval(index, line, false);
                builtins::index(&v, &i)
            }
            Expr::Member { expr, key } => {
                let v = self.eval(expr, line, false);
                builtins::index(&v, &Value::Ident(key.clone()))
            }
            Expr::Call { name, args } => {
                if let Some(def) = self.find_def(name) {
                    let args = self.eval_args(args, line);
                    let mut sink = Vec::new();
                    let ctx = Ctx {
                        top_level: false,
                        in_rule: false,
                        in_mixin: true,
                    };
                    return self.invoke(&def, args, &mut sink, ctx, false);
                }
                let args = self.eval_args(args, line);
                self.call_function(name, args, line)
            }
        }
    }

    fn call_function(
        &mut self,
        name: &str,
        args: Vec<(Option<String>, Value)>,
        line: u32,
    ) -> Value {
        if name == "embedurl" {
            let spec = args.first().map(|(_, v)| v.text(false)).unwrap_or_default();
            let Some(found) = self.lookup_file(&spec) else {
                self.warn(line, "asset", format!("`embedurl()`: can't find `{spec}`"));
                return Value::Raw(format!("url('{spec}?inline')"));
            };
            let from = self
                .output_file
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf();
            let relative = stylet_resolve::relative(&from, &found);
            let relative = relative.to_string_lossy().replace('\\', "/");
            return Value::Raw(format!("url('{relative}?inline')"));
        }
        match builtins::call(name, &args) {
            builtins::Result::Value(v) => v,
            builtins::Result::Warn(v, message) => {
                self.warn(line, "function", message);
                v
            }
            builtins::Result::Unknown(v) => {
                self.unknown_functions
                    .push((self.warnings.len(), name.to_string()));
                self.warn(
                    line,
                    "js-function",
                    format!("unknown function `{name}()` (JS plugin?); kept as a CSS function"),
                );
                v
            }
        }
    }

    /// Replaces `{expr}` in selectors, property names and preludes.
    fn interpolate(&mut self, text: &str, line: u32) -> String {
        if !text.contains('{') {
            return text.to_string();
        }
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            out += &rest[..start];
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                out += &rest[start..];
                return out;
            };
            let value = self.eval_str(&after[..end], line).unwrap_or(Value::Null);
            out += &value.text(false);
            rest = &after[end + 1..];
        }
        out + rest
    }
}

pub fn tracked_vars(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Tracked { vars, .. } => out.extend(vars.iter().cloned()),
        Value::List { items, .. } => items.iter().for_each(|v| tracked_vars(v, out)),
        _ => {}
    }
}

fn is_serializable(value: &Value) -> bool {
    match value.literal() {
        Value::Number(_)
        | Value::Color(_)
        | Value::Str { .. }
        | Value::Ident(_)
        | Value::Raw(_) => true,
        Value::List { items, .. } => !items.is_empty() && items.iter().all(is_serializable),
        _ => false,
    }
}

fn join_raw(l: &Value, op: &str, r: &Value) -> Value {
    let raw = |symbolic| format!("{}{op}{}", l.css(symbolic), r.css(symbolic));
    if l.is_tracked() || r.is_tracked() {
        let mut vars = Vec::new();
        tracked_vars(l, &mut vars);
        tracked_vars(r, &mut vars);
        Value::Tracked {
            css: raw(true),
            value: Box::new(Value::Raw(raw(false))),
            vars,
        }
    } else {
        Value::Raw(raw(false))
    }
}

/// A `--define name=value`.
fn literal_value(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => expr::parse(text)
            .ok()
            .and_then(|e| match e {
                Expr::Number(n) => Some(Value::Number(n)),
                Expr::Str(s, q) => Some(Value::Str { s, quote: Some(q) }),
                _ => None,
            })
            .unwrap_or_else(|| Value::Ident(text.to_string())),
    }
}

fn is_placeholder(selector: &str) -> bool {
    selector.starts_with('$')
        && selector[1..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Splits a postfix condition: `expr if cond` / `expr unless cond`.
fn split_postfix(text: &str) -> (&str, Option<(bool, &str)>) {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ' ' if depth == 0 => {
                    let rest = &text[i + 1..];
                    for (word, negate) in [("if ", false), ("unless ", true)] {
                        if let Some(condition) = rest.strip_prefix(word) {
                            return (text[..i].trim_end(), Some((negate, condition.trim())));
                        }
                    }
                }
                _ => {}
            },
        }
    }
    (text, None)
}

/// A plain `url(…)` with a relative path (not `?inline`, absolute, data or external).
fn has_relative_url(css: &str) -> bool {
    let mut rest = css;
    while let Some(i) = rest.find("url(") {
        let arg = rest[i + 4..].trim_start().trim_start_matches(['"', '\'']);
        let relative = !(arg.starts_with('/')
            || arg.starts_with('#')
            || arg.starts_with("data:")
            || arg.starts_with("http:")
            || arg.starts_with("https:")
            || arg.starts_with("var(")
            || arg
                .split([')', '"', '\''])
                .next()
                .is_some_and(|u| u.contains("?inline")));
        if relative {
            return true;
        }
        rest = &rest[i + 4..];
    }
    false
}
