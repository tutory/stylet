//! Import resolution, a virtual file system and a cache of parsed files.
//!
//! Nothing here touches the real file system except [`OsFs`], so the crate
//! builds for `wasm32`.

mod fs;
mod line_index;
mod path;

pub use fs::{FileSystem, MemoryFs, OsFs};
pub use line_index::{LineCol, LineIndex};
pub use path::{normalize, relative};

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use stylet_syntax::Parse;

#[derive(Debug, Clone, Default)]
pub struct ResolveConfig {
    /// Directory that `/`-prefixed imports resolve against.
    pub root: PathBuf,
    /// Prefix → directory, e.g. `@/` → `client/`. The longest matching prefix wins.
    pub aliases: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    /// `None` for plain `.css` files, which are inlined verbatim.
    pub parse: Option<Parse>,
    pub line_index: LineIndex,
}

impl SourceFile {
    pub fn is_css(&self) -> bool {
        self.parse.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotFound { spec: String, tried: Vec<PathBuf> },
    Io { path: PathBuf, message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { spec, tried } => {
                write!(f, "can't find `{spec}`")?;
                if !tried.is_empty() {
                    let tried: Vec<_> = tried.iter().map(|p| p.display().to_string()).collect();
                    write!(f, " (tried {})", tried.join(", "))?;
                }
                Ok(())
            }
            Self::Io { path, message } => write!(f, "can't read {}: {message}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

pub struct Loader<F> {
    fs: F,
    config: ResolveConfig,
    files: Vec<SourceFile>,
    by_path: HashMap<PathBuf, FileId>,
}

impl<F> Loader<F> {
    pub fn new(fs: F, config: ResolveConfig) -> Self {
        Self {
            fs,
            config,
            files: Vec::new(),
            by_path: HashMap::new(),
        }
    }

    pub fn fs(&self) -> &F {
        &self.fs
    }

    pub fn config(&self) -> &ResolveConfig {
        &self.config
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    /// Forgets a cached file so the next [`load`](Self::load) reads it again.
    /// Existing [`FileId`]s stay valid and keep the old contents.
    pub fn invalidate(&mut self, path: &Path) {
        self.by_path.remove(&normalize(path));
    }
}

impl<F: FileSystem> Loader<F> {
    /// Loads and parses `path`, or returns the cached file.
    pub fn load(&mut self, path: &Path) -> Result<FileId, Error> {
        let path = normalize(path);
        if let Some(&id) = self.by_path.get(&path) {
            return Ok(id);
        }
        let text = self.fs.read_to_string(&path).map_err(|e| Error::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let is_css = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("css"));
        let parse = (!is_css).then(|| stylet_syntax::parse(&text));
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile {
            line_index: LineIndex::new(&text),
            path: path.clone(),
            text,
            parse,
        });
        self.by_path.insert(path, id);
        Ok(id)
    }

    /// Resolves an import `spec` written in `from`.
    ///
    /// - `/x` resolves against the configured root, alias prefixes against their directory,
    ///   anything else relative to the importing file.
    /// - Tries the path itself (if it has a `.styl` or `.css` extension), then `x.styl`, then `x/index.styl`.
    pub fn resolve(&self, spec: &str, from: &Path) -> Result<PathBuf, Error> {
        let base = self.base_path(spec, from);
        let mut tried = Vec::new();
        let has_ext = base
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("styl") || e.eq_ignore_ascii_case("css"));
        let mut candidates = Vec::new();
        if has_ext {
            candidates.push(base.clone());
        }
        let mut with_ext = base.clone().into_os_string();
        with_ext.push(".styl");
        candidates.push(with_ext.into());
        candidates.push(base.join("index.styl"));
        for candidate in candidates {
            let candidate = normalize(&candidate);
            if self.fs.is_file(&candidate) {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
        Err(Error::NotFound {
            spec: spec.to_string(),
            tried,
        })
    }

    fn base_path(&self, spec: &str, from: &Path) -> PathBuf {
        let alias = self
            .config
            .aliases
            .iter()
            .filter(|(prefix, _)| spec.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len());
        if let Some((prefix, dir)) = alias {
            return self.config.root.join(dir).join(&spec[prefix.len()..]);
        }
        if let Some(rest) = spec.strip_prefix('/') {
            return self.config.root.join(rest);
        }
        from.parent().unwrap_or(Path::new("")).join(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader() -> Loader<MemoryFs> {
        let mut fs = MemoryFs::default();
        for path in [
            "/p/client/variables.styl",
            "/p/client/components/index.styl",
            "/p/client/components/button.styl",
            "/p/client/legacy.css",
            "/p/env/a/index.styl",
        ] {
            fs.insert(path, "");
        }
        Loader::new(
            fs,
            ResolveConfig {
                root: "/p".into(),
                aliases: vec![
                    ("@/".into(), "client".into()),
                    ("@/c/".into(), "client/components".into()),
                ],
            },
        )
    }

    #[test]
    fn resolves() {
        let l = loader();
        let from = Path::new("/p/env/a/index.styl");
        let ok = |spec| l.resolve(spec, from).unwrap();
        assert_eq!(
            ok("/client/variables"),
            Path::new("/p/client/variables.styl")
        );
        assert_eq!(
            ok("/client/components"),
            Path::new("/p/client/components/index.styl")
        );
        assert_eq!(
            ok("../../client/variables.styl"),
            Path::new("/p/client/variables.styl")
        );
        assert_eq!(ok("@/legacy.css"), Path::new("/p/client/legacy.css"));
        assert_eq!(
            ok("@/c/button"),
            Path::new("/p/client/components/button.styl")
        );
        assert_eq!(ok("./index"), Path::new("/p/env/a/index.styl"));
    }

    #[test]
    fn not_found() {
        let err = loader()
            .resolve("./nope", Path::new("/p/a.styl"))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "can't find `./nope` (tried /p/nope.styl, /p/nope/index.styl)"
        );
    }

    #[test]
    fn caches_and_invalidates() {
        let mut l = loader();
        let a = l.load(Path::new("/p/client/./variables.styl")).unwrap();
        assert_eq!(l.load(Path::new("/p/client/variables.styl")).unwrap(), a);
        l.invalidate(Path::new("/p/client/variables.styl"));
        assert_ne!(l.load(Path::new("/p/client/variables.styl")).unwrap(), a);
        let css = l.load(Path::new("/p/client/legacy.css")).unwrap();
        assert!(l.file(css).is_css());
    }
}
