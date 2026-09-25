//! Fixture tests: `tests/data/{ok,err}/*.styl` against `*.rast` snapshots.
//! Run with `UPDATE_EXPECT=1` to update the snapshots.

use std::fs;
use std::path::Path;

fn run(dir: &str, expect_errors: bool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "styl"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
    for path in paths {
        let src = fs::read_to_string(&path).unwrap();
        let parse = stylet_syntax::parse(&src);
        assert_eq!(
            parse.syntax().to_string(),
            src,
            "lossless: {}",
            path.display()
        );
        assert_eq!(
            !parse.errors().is_empty(),
            expect_errors,
            "{}: {:?}",
            path.display(),
            parse.errors()
        );
        expect_test::expect_file![path.with_extension("rast")].assert_eq(&parse.debug_tree());
    }
}

#[test]
fn ok() {
    run("ok", false);
}

#[test]
fn err() {
    run("err", true);
}

/// Every prefix of every fixture parses losslessly (catches panics on truncated input).
#[test]
fn prefixes() {
    for dir in ["ok", "err"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(dir);
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "styl") {
                continue;
            }
            let src = fs::read_to_string(&path).unwrap();
            for (i, _) in src.char_indices() {
                let prefix = &src[..i];
                assert_eq!(stylet_syntax::parse(prefix).syntax().to_string(), prefix);
            }
        }
    }
}

/// Parses every `.styl` file below `$STYLET_CORPUS` (e.g. `../webapp`), checking losslessness.
/// Stylus files are not valid stylet, so errors are expected; only panics fail.
#[test]
#[ignore = "needs STYLET_CORPUS"]
fn corpus() {
    let root = std::env::var("STYLET_CORPUS").expect("set STYLET_CORPUS");
    let mut stack = vec![std::path::PathBuf::from(root)];
    let mut count = 0;
    let mut parse_time = std::time::Duration::ZERO;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    stack.push(path);
                }
            } else if name.ends_with(".styl") {
                let src = fs::read_to_string(&path).unwrap();
                let start = std::time::Instant::now();
                let parse = stylet_syntax::parse(&src);
                parse_time += start.elapsed();
                assert_eq!(parse.syntax().to_string(), src, "{}", path.display());
                count += 1;
            }
        }
    }
    eprintln!("parsed {count} files in {parse_time:?}");
}
