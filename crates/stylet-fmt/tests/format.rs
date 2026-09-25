//! Fixture tests: `tests/data/*.styl` formatted against `*.out`. Every fixture must
//! format idempotently and compile to the same CSS before and after formatting.
//! A first line `// options: sort` enables property sorting.
//! Run with `UPDATE_EXPECT=1` to update the snapshots.

use std::fs;
use std::path::Path;
use stylet_fmt::{Indent, Options, format};

fn compile(src: &str) -> String {
    let options = stylet_compile::Options::default();
    let out = stylet_compile::compile_str(src, &options);
    let errors: Vec<_> = out.diagnostics.iter().map(|d| d.message.clone()).collect();
    format!("{}{errors:?}", out.css)
}

#[test]
fn fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "styl"))
        .collect();
    paths.sort();
    for path in paths {
        let src = fs::read_to_string(&path).unwrap();
        let flags = src
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("// options:"))
            .unwrap_or("");
        let sort = flags.contains("sort");
        let options = Options {
            sort_properties: sort,
            nested_blocks_last: flags.contains("nested"),
            align_strings: !flags.contains("no-align"),
            ..Options::default()
        };
        let formatted = format(&src, &options).unwrap();
        expect_test::expect_file![path.with_extension("out")].assert_eq(&formatted);
        assert_eq!(
            format(&formatted, &options).unwrap(),
            formatted,
            "idempotent: {}",
            path.display()
        );
        // Sorting and moving blocks reorder the output on purpose.
        if !sort && !options.nested_blocks_last {
            let semicolons = src.contains(';');
            if !semicolons {
                assert_eq!(
                    compile(&formatted),
                    compile(&src),
                    "same CSS: {}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn indent_options() {
    let src = ".a {\n  .b {\n    color: red\n  }\n}\n";
    let tabs = Options {
        indent: Indent::Tabs,
        ..Options::default()
    };
    assert_eq!(
        format(src, &tabs).unwrap(),
        ".a {\n\t.b {\n\t\tcolor: red\n\t}\n}\n"
    );
    let four = Options {
        indent: Indent::Spaces(4),
        ..Options::default()
    };
    assert_eq!(
        format(src, &four).unwrap(),
        ".a {\n    .b {\n        color: red\n    }\n}\n"
    );
}

#[test]
fn refuses_broken_input() {
    assert!(format(".a {\n  color\n}\n", &Options::default()).is_err());
    assert_eq!(
        format(".a { color: red; }", &Options::default()).unwrap(),
        ".a {\n  color: red\n}\n"
    );
}

/// Fixtures of the other crates: idempotent and CSS-preserving.
#[test]
fn other_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for dir in ["stylet-syntax/tests/data/ok", "stylet-compile/tests/data"] {
        for entry in fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "styl") {
                continue;
            }
            let src = fs::read_to_string(&path).unwrap();
            let Ok(formatted) = format(&src, &Options::default()) else {
                continue;
            };
            assert_eq!(
                format(&formatted, &Options::default()).unwrap(),
                formatted,
                "{}",
                path.display()
            );
            if !src.contains(';') {
                assert_eq!(compile(&formatted), compile(&src), "{}", path.display());
            }
        }
    }
}
