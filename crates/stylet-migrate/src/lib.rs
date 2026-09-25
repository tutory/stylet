//! Migrates Stylus sources to stylet.
//!
//! Stylus is evaluated statically, one entry at a time and in import order
//! (mixins and variables come from earlier imports). Each source file gets
//! its own stylet version: mixins are inlined, functions evaluated, global
//! variables become custom properties (or are inlined), and whatever can't
//! be decided statically is kept as a `// stylet-migrate:` comment with a
//! warning.

mod builtins;
mod eval;
pub mod expr;
mod out;
pub mod parse;
mod value;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use stylet_resolve::FileSystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarMode {
    /// Global variables become custom properties where possible.
    #[default]
    Props,
    /// Every variable is inlined.
    Inline,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub vars: VarMode,
    /// Prefix for generated custom property names (`--<prefix>name`).
    pub var_prefix: String,
    /// Unroll `for` loops instead of commenting them out.
    pub unroll_loops: bool,
    /// Values for identifiers Stylus got from JS (e.g. `isDevelopment=false`).
    pub defines: Vec<(String, String)>,
    /// Stylus files evaluated before each entry for their definitions only,
    /// e.g. a mixin library like `node_modules/axis/axis/index.styl`.
    pub preload: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub path: PathBuf,
    /// 1-based; 0 when not tied to a line.
    pub line: u32,
    pub category: &'static str,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Migration {
    /// New contents per source file.
    pub files: BTreeMap<PathBuf, String>,
    pub warnings: Vec<Warning>,
    /// Problems that prevent writing files (e.g. custom property name collisions).
    pub errors: Vec<String>,
}

/// Migrates `entries` (absolute paths) and the files they import. `root` is
/// where `/`-prefixed imports resolve.
pub fn migrate<F: FileSystem>(
    fs: &F,
    root: &Path,
    entries: &[PathBuf],
    options: &Options,
) -> Migration {
    // Pass 1: find which globals are used in declarations.
    let mut first = eval::Interp::new(fs, root, options, None);
    for entry in entries {
        first.entry(entry);
    }
    let eligible = first.usage.in_declarations.clone();

    let mut migration = Migration::default();
    if options.vars == VarMode::Props {
        let mut generated: HashMap<String, &str> = HashMap::new();
        for var in &eligible {
            let prop = format!("--{}{}", options.var_prefix, var.trim_start_matches('$'));
            if let Some(other) = generated.insert(prop.clone(), var) {
                migration
                    .errors
                    .push(format!("`{other}` and `{var}` would both become `{prop}`; use --var-prefix or rename one"));
            }
            if first.usage.custom_properties.contains(&prop) && !first.mirrors.contains(&prop) {
                migration.errors.push(format!(
                    "`{var}` would become `{prop}`, which is already declared; use --var-prefix"
                ));
            }
        }
    }

    // Pass 2: the real conversion.
    let mut second = eval::Interp::new(fs, root, options, Some(&eligible));
    for entry in entries {
        second.entry(entry);
    }
    migration.warnings = std::mem::take(&mut second.warnings);
    for (path, (text, _)) in std::mem::take(&mut second.outputs) {
        let formatted = match stylet_fmt::format(&text, &stylet_fmt::Options::default()) {
            Ok(formatted) => formatted,
            Err(errors) => {
                for e in errors {
                    migration.warnings.push(Warning {
                        path: path.clone(),
                        line: 0,
                        category: "internal",
                        message: format!("the converted file has a syntax error: {}", e.message()),
                    });
                }
                text
            }
        };
        migration.files.insert(path, formatted);
    }
    migration
        .warnings
        .sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    migration
}

/// Warning counts per category.
pub fn summary(warnings: &[Warning]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for w in warnings {
        *counts.entry(w.category).or_default() += 1;
    }
    counts
}
