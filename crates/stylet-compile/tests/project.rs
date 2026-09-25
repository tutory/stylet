//! Multi-file compilation against `tests/project`.

use std::path::Path;
use stylet_compile::{Options, compile_file};
use stylet_resolve::{Loader, OsFs, ResolveConfig};

#[test]
fn project() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/project");
    let mut loader = Loader::new(
        OsFs,
        ResolveConfig {
            root: root.clone(),
            aliases: vec![("@/".into(), "client".into())],
        },
    );
    let options = Options {
        source_map: true,
        output: Some(root.join("public/index.css")),
        ..Options::default()
    };
    let mut text = String::new();
    for entry in ["env/a/index.styl", "env/a/errors.styl"] {
        text += &format!("=== {entry}\n");
        compile(&mut loader, &root, &root.join(entry), &options, &mut text);
    }
    expect_test::expect_file!["project.out"].assert_eq(&text);
}

fn compile(
    loader: &mut Loader<OsFs>,
    root: &Path,
    entry: &Path,
    options: &Options,
    text: &mut String,
) {
    let out = compile_file(loader, entry, options);
    *text += &out.css;
    *text += "--- diagnostics\n";
    for d in &out.diagnostics {
        let file = d.file.map(|f| {
            loader
                .file(f)
                .path
                .strip_prefix(root)
                .unwrap()
                .display()
                .to_string()
        });
        let message = d.message.replace(&format!("{}/", root.display()), "");
        *text += &format!("{}: {message}\n", file.unwrap_or_default());
    }
    *text += "--- dependencies\n";
    for dep in &out.dependencies {
        *text += &format!("{}\n", dep.strip_prefix(root).unwrap().display());
    }
    *text += "--- source map\n";
    *text += out.source_map.as_deref().unwrap_or_default();
    text.push('\n');
}
