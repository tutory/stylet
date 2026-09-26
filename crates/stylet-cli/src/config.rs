//! `stylet.toml`.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use stylet_resolve::{ResolveConfig, normalize};

pub const FILE_NAME: &str = "stylet.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directory that `/`-prefixed imports resolve against, relative to the config file.
    #[serde(default)]
    pub root: Option<PathBuf>,
    /// Import prefix → directory relative to `root`.
    #[serde(default)]
    pub aliases: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub fmt: Fmt,
    #[serde(default, rename = "entry")]
    pub entries: Vec<Entry>,
    /// Directory of the config file; set after loading.
    #[serde(skip)]
    pub dir: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    #[serde(default)]
    pub minify: bool,
    #[serde(default)]
    pub source_map: bool,
    #[serde(default)]
    pub resolve_custom_media: bool,
    #[serde(default)]
    pub flatten: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fmt {
    /// Spaces per level, or `"tab"`.
    #[serde(default)]
    pub indent: Option<IndentSetting>,
    // Unset keys use the formatter's defaults.
    pub sort_properties: Option<bool>,
    pub nested_blocks_last: Option<bool>,
    pub align_strings: Option<bool>,
    pub blank_lines_around_blocks: Option<bool>,
    pub blank_lines_around_imports: Option<bool>,
    pub selector_per_line: Option<bool>,
    pub normalize_spacing: Option<bool>,
    pub single_quotes: Option<bool>,
    pub leading_zero: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum IndentSetting {
    Spaces(u8),
    Keyword(String),
}

impl Fmt {
    pub fn options(&self) -> Result<stylet_fmt::Options, String> {
        let indent = match &self.indent {
            None => stylet_fmt::Indent::default(),
            Some(IndentSetting::Spaces(n)) => stylet_fmt::Indent::Spaces(*n),
            Some(IndentSetting::Keyword(k)) if k == "tab" => stylet_fmt::Indent::Tabs,
            Some(IndentSetting::Keyword(k)) => {
                return Err(format!("fmt.indent must be a number or \"tab\", not {k:?}"));
            }
        };
        let d = stylet_fmt::Options::default();
        Ok(stylet_fmt::Options {
            indent,
            sort_properties: self.sort_properties.unwrap_or(d.sort_properties),
            nested_blocks_last: self.nested_blocks_last.unwrap_or(d.nested_blocks_last),
            align_strings: self.align_strings.unwrap_or(d.align_strings),
            blank_lines_around_blocks: self
                .blank_lines_around_blocks
                .unwrap_or(d.blank_lines_around_blocks),
            blank_lines_around_imports: self
                .blank_lines_around_imports
                .unwrap_or(d.blank_lines_around_imports),
            selector_per_line: self.selector_per_line.unwrap_or(d.selector_per_line),
            normalize_spacing: self.normalize_spacing.unwrap_or(d.normalize_spacing),
            single_quotes: self.single_quotes.unwrap_or(d.single_quotes),
            leading_zero: self.leading_zero.unwrap_or(d.leading_zero),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub input: PathBuf,
    pub output: PathBuf,
}

impl Config {
    /// Loads `path`, or searches `stylet.toml` from `start` upwards. Without a
    /// config file, returns defaults rooted at `start`.
    pub fn load(path: Option<&Path>, start: &Path) -> Result<Self, String> {
        let path = match path {
            Some(path) => Some(path.to_path_buf()),
            None => start
                .ancestors()
                .map(|d| d.join(FILE_NAME))
                .find(|p| p.is_file()),
        };
        let Some(path) = path else {
            return Ok(Self {
                dir: start.to_path_buf(),
                ..Self::default()
            });
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("can't read {}: {e}", path.display()))?;
        let mut config: Self =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        config.dir = normalize(&start.join(path.parent().unwrap_or(Path::new(""))));
        Ok(config)
    }

    pub fn root(&self) -> PathBuf {
        normalize(&self.dir.join(self.root.as_deref().unwrap_or(Path::new(""))))
    }

    pub fn resolve(&self) -> ResolveConfig {
        ResolveConfig {
            root: self.root(),
            aliases: self
                .aliases
                .iter()
                .map(|(prefix, dir)| (prefix.clone(), dir.clone()))
                .collect(),
        }
    }

    /// A path from the config file, made absolute.
    pub fn path(&self, path: &Path) -> PathBuf {
        normalize(&self.dir.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let config: Config = toml::from_str(
            r#"
            root = "."
            [aliases]
            "@/" = "client"
            [build]
            minify = true
            [fmt]
            indent = "tab"
            sort_properties = false
            [[entry]]
            input = "a.styl"
            output = "a.css"
            "#,
        )
        .unwrap();
        assert!(config.build.minify);
        let fmt = config.fmt.options().unwrap();
        assert_eq!(fmt.indent, stylet_fmt::Indent::Tabs);
        assert!(!fmt.sort_properties);
        assert!(fmt.single_quotes);
        assert_eq!(config.entries.len(), 1);
        assert_eq!(config.aliases["@/"], Path::new("client"));
        assert!(toml::from_str::<Config>("nope = 1").is_err());
    }
}
