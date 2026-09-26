//! Fixture tests: each `tests/data/*.styl` is compiled pretty and minified and compared
//! against `*.out`. A first line `// options: resolve_custom_media` enables options.
//! Run with `UPDATE_EXPECT=1` to update the snapshots.

use std::fs;
use std::path::Path;
use stylet_compile::{Options, compile_str};

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
        let options = Options {
            resolve_custom_media: flags.contains("resolve_custom_media"),
            flatten: flags.contains("flatten"),
            ..Options::default()
        };
        let pretty = compile_str(&src, &options);
        let minified = compile_str(
            &src,
            &Options {
                minify: true,
                ..options
            },
        );
        let mut out = format!(
            "{}\n--- minified\n{}\n",
            pretty.css.trim_end(),
            minified.css
        );
        if !pretty.diagnostics.is_empty() {
            out += "--- diagnostics\n";
            for d in &pretty.diagnostics {
                out += &format!(
                    "{:?} {:?} {:?}: {}\n",
                    d.severity, d.range, &src[d.range], d.message
                );
            }
        }
        expect_test::expect_file![path.with_extension("out")].assert_eq(&out);
    }
}
