//! End-to-end migration of a small in-memory Stylus project.

use std::path::{Path, PathBuf};
use stylet_migrate::{Options, VarMode, migrate};
use stylet_resolve::MemoryFs;

const FILES: &[(&str, &str)] = &[
    (
        "/p/index.styl",
        "@import 'vars'\n@import 'mixins'\n\n@layer base\n  @import 'components/button'\n",
    ),
    (
        "/p/vars.styl",
        "// variables\nbreakpoint = 600px\n$gap = 4px\n$shadow = 0 1px 2px rgba(0, 0, 0, .3)\n$alias = $gap\n$map = { a: 1 }\nunused = 1px\n",
    ),
    (
        "/p/mixins.styl",
        "\
flexify(h = center, v = center)
  display: flex
  justify-content: h
  align-items: v

line-clamp(n)
  text-overflow: ellipsis
  if n == 1
    white-space: nowrap
  else
    -webkit-line-clamp: n

alphaColor(color, alpha)
  s('color-mix(in srgb, %s %s, transparent)', color, alpha)

$focus
  outline: 1px solid
",
    ),
    (
        "/p/components/button.styl",
        "\
.button
.link
  flexify(space-between)
  line-clamp: 1
  gap: $gap * 2
  margin: $alias
  box-shadow: $shadow
  color: alphaColor(var(--black), 80%)
  width: calc(100% - 10px)
  font: 12px/1.5 sans-serif
  @extends $focus
  &:hover
    color: red // hover
  @media (max-width: breakpoint)
    padding: 0
  for n in 1..2
    .l-{n}
      x: n
  if isDevelopment
    outline: red
  @extend .other
",
    ),
];

fn run(options: &Options) -> String {
    let mut fs = MemoryFs::default();
    for (path, text) in FILES {
        fs.insert(path, *text);
    }
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        options,
    );
    let mut out = String::new();
    for (path, text) in &migration.files {
        out += &format!("=== {}\n{text}", path.display());
    }
    out += "=== warnings\n";
    for w in &migration.warnings {
        out += &format!(
            "{}:{} [{}] {}\n",
            w.path.display(),
            w.line,
            w.category,
            w.message
        );
    }
    for e in &migration.errors {
        out += &format!("error: {e}\n");
    }
    out
}

#[test]
fn props() {
    expect_test::expect_file!["migrate_props.out"].assert_eq(&run(&Options::default()));
}

#[test]
fn inline() {
    let options = Options {
        vars: VarMode::Inline,
        unroll_loops: true,
        defines: vec![("isDevelopment".into(), "false".into())],
        ..Options::default()
    };
    expect_test::expect_file!["migrate_inline.out"].assert_eq(&run(&options));
}

/// A `.css` import never pops Stylus' lookup stack, so the importing file's
/// directory stays searchable for later imports; the migration rewrites such
/// imports to root-relative paths.
#[test]
fn leaked_import_directories() {
    let mut fs = MemoryFs::default();
    fs.insert(
        "/p/env/t/index.styl",
        "@layer core\n  @import '/client/apps'\n@import './apps'\n",
    );
    fs.insert(
        "/p/client/apps/index.styl",
        "@import 'base'\n@import 'vendor.css'\n",
    );
    fs.insert("/p/client/apps/vendor.css", "");
    fs.insert("/p/client/apps/base/base.styl", ".base\n  a: b\n");
    fs.insert("/p/client/apps/editor/x.styl", ".x\n  a: b\n");
    fs.insert("/p/env/t/apps/index.styl", "@import './editor/x'\n");
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/env/t/index.styl")],
        &Options::default(),
    );
    assert_eq!(migration.warnings, []);
    assert_eq!(
        migration.files[Path::new("/p/env/t/apps/index.styl")],
        "@import '/client/apps/editor/x'\n"
    );
    assert_eq!(
        migration.files[Path::new("/p/client/apps/index.styl")],
        "@import '/client/apps/base/base'\n@import 'vendor.css'\n"
    );
}

/// Stylus scopes mixins defined in files imported inside a block to that block.
#[test]
fn block_scoped_definitions() {
    let mut fs = MemoryFs::default();
    fs.insert(
        "/p/index.styl",
        "@layer a\n  @import 'quirks'\n@layer b\n  @import 'form'\n",
    );
    fs.insert("/p/quirks.styl", "mb(n)\n  mbe: n\n");
    fs.insert("/p/form.styl", ".f\n  mb: 1px\n");
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        &Options::default(),
    );
    assert_eq!(
        migration.files[Path::new("/p/form.styl")],
        ".f {\n  mb: 1px\n}\n"
    );
}

/// Library mixins named like standard properties (axis' `transition`) are
/// not called when used as a property.
#[test]
fn preloaded_property_mixins() {
    let mut fs = MemoryFs::default();
    fs.insert(
        "/p/lib.styl",
        "transition()\n  -webkit-transition: arguments\n  transition: arguments\n",
    );
    fs.insert("/p/index.styl", ".b\n  transition: width, opacity .3s\n");
    let options = Options {
        preload: vec![PathBuf::from("/p/lib.styl")],
        ..Options::default()
    };
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        &options,
    );
    assert_eq!(
        migration.files[Path::new("/p/index.styl")],
        ".b {\n  transition: width, opacity .3s\n}\n"
    );
}

/// `/selector` escapes nesting: the rule moves to the top level of the file.
#[test]
fn root_selectors() {
    let mut fs = MemoryFs::default();
    fs.insert("/p/index.styl", ".page\n  .a\n  /#editor\n    x: y\n");
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        &Options::default(),
    );
    assert_eq!(
        migration.files[Path::new("/p/index.styl")],
        "#editor {\n  x: y\n}\n\n.page {\n  .a {\n    x: y\n  }\n}\n"
    );
}

/// `--custom-media`: variables in media conditions become `@custom-media`.
#[test]
fn custom_media() {
    let mut fs = MemoryFs::default();
    fs.insert(
        "/p/index.styl",
        "@import 'vars'\n.a\n  @media (max-width: phone)\n    b: c\n",
    );
    fs.insert("/p/vars.styl", "phone = 600px\n");
    let options = Options {
        custom_media: true,
        ..Options::default()
    };
    let migration = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        &options,
    );
    assert_eq!(
        migration.files[Path::new("/p/vars.styl")],
        "@custom-media --phone-max-width (max-width: 600px)\n"
    );
    assert!(migration.files[Path::new("/p/index.styl")].contains("@media (--phone-max-width) {"));
    let inline = migrate(
        &fs,
        Path::new("/p"),
        &[PathBuf::from("/p/index.styl")],
        &Options::default(),
    );
    assert!(inline.files[Path::new("/p/index.styl")].contains("@media (max-width: 600px) {"));
}
