//! Compiles stylet to CSS.
//!
//! Nesting is emitted as native CSS nesting; values, selectors and unknown
//! at-rules pass through unchanged apart from whitespace and comments.

mod custom_media;
mod emit;
mod source_map;
mod text;
mod url;

use std::path::{Path, PathBuf};
use stylet_resolve::{FileId, FileSystem, Loader, MemoryFs, ResolveConfig, relative};
use stylet_syntax::TextRange;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Strip whitespace and comments (except `/*! … */`).
    pub minify: bool,
    /// Substitute `(--name)` media conditions and drop `@custom-media` definitions.
    /// Definitions must come before their first use.
    pub resolve_custom_media: bool,
    /// Generate a source map.
    pub source_map: bool,
    /// Path the CSS will be written to. Relative `url()`s are rebased against its
    /// directory, and source map paths are relative to it. Defaults to the entry file.
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// `None` for problems not tied to a file, e.g. a missing entry.
    pub file: Option<FileId>,
    pub range: TextRange,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, file: Option<FileId>, range: TextRange) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            file,
            range,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub css: String,
    /// Source map JSON, if requested.
    pub source_map: Option<String>,
    /// Syntax and compile diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Every file that was read, starting with the entry.
    pub dependencies: Vec<PathBuf>,
}

impl Output {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Compiles `entry` and everything it imports. Files are cached in `loader`,
/// so reusing it across entries parses shared imports once.
pub fn compile_file<F: FileSystem>(
    loader: &mut Loader<F>,
    entry: &Path,
    options: &Options,
) -> Output {
    let entry = match loader.load(entry) {
        Ok(file) => file,
        Err(e) => {
            return Output {
                css: String::new(),
                source_map: None,
                diagnostics: vec![Diagnostic::error(e.to_string(), None, TextRange::default())],
                dependencies: vec![entry.to_path_buf()],
            };
        }
    };
    let output = options
        .output
        .clone()
        .unwrap_or_else(|| loader.file(entry).path.clone());
    let out_dir = output.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut emitter = emit::Emitter::new(loader, options, out_dir.clone());
    emitter.entry(entry);
    emitter.finish();
    let emit::Emitter {
        out: css,
        mappings,
        mut diagnostics,
        dependencies,
        sources,
        ..
    } = emitter;

    // Syntax errors of every file that took part.
    for file in sources {
        if let Some(parse) = &loader.file(file).parse {
            diagnostics.extend(
                parse
                    .errors()
                    .iter()
                    .map(|e| Diagnostic::error(e.message(), Some(file), e.range())),
            );
        }
    }
    diagnostics.sort_by_key(|d| (d.file, d.range.start()));

    let source_map = options.source_map.then(|| {
        let file_name = output.file_name().map(|n| n.to_string_lossy().into_owned());
        source_map::build(
            &css,
            &mappings,
            |id| {
                let file = loader.file(id);
                source_map::Source {
                    path: relative(&out_dir, &file.path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    content: &file.text,
                }
            },
            file_name.as_deref(),
        )
    });

    Output {
        css,
        source_map,
        diagnostics,
        dependencies,
    }
}

/// Compiles a single source string, e.g. in the playground. `@import` fails
/// because there are no other files.
pub fn compile_str(src: &str, options: &Options) -> Output {
    let mut fs = MemoryFs::default();
    fs.insert("/input.styl", src);
    let mut loader = Loader::new(fs, ResolveConfig::default());
    compile_file(&mut loader, Path::new("/input.styl"), options)
}
