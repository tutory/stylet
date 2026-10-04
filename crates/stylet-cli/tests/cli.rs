//! Runs the `stylet` binary.

use std::fs;
use std::process::Command;

#[test]
fn build_deps() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir(root.join("parts")).unwrap();
    fs::write(
        root.join("index.styl"),
        "@import 'parts/a'\n@import 'b.css'\n",
    )
    .unwrap();
    fs::write(root.join("parts/a.styl"), ".a {\n  color: red\n}\n").unwrap();
    fs::write(root.join("b.css"), ".b { color: blue; }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_stylet"))
        .args(["build", "index.styl", "--deps"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "index.styl\nparts/a.styl\nb.css\n"
    );
}
