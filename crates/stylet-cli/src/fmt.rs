//! `stylet fmt`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use stylet_fmt::{Options, format};
use stylet_resolve::LineIndex;

/// Returns whether everything was formatted (or, with `check`, already formatted).
pub fn run(paths: &[PathBuf], check: bool, options: &Options, cwd: &Path) -> Result<bool, String> {
    if paths.len() == 1 && paths[0].as_os_str() == "-" {
        let mut src = String::new();
        std::io::stdin()
            .read_to_string(&mut src)
            .map_err(|e| e.to_string())?;
        return match format(&src, options) {
            Ok(out) => {
                std::io::stdout()
                    .write_all(out.as_bytes())
                    .map_err(|e| e.to_string())?;
                Ok(true)
            }
            Err(errors) => {
                report("<stdin>", &src, &errors);
                Ok(false)
            }
        };
    }

    let roots = if paths.is_empty() {
        vec![cwd.to_path_buf()]
    } else {
        paths.iter().map(|p| cwd.join(p)).collect()
    };
    let mut files = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect(&root, &mut files);
        } else if root.is_file() {
            files.push(root);
        } else {
            return Err(format!("{} doesn't exist", root.display()));
        }
    }

    let mut ok = true;
    let mut changed = 0;
    for path in &files {
        let display = path.strip_prefix(cwd).unwrap_or(path).display().to_string();
        let src =
            std::fs::read_to_string(path).map_err(|e| format!("can't read {display}: {e}"))?;
        match format(&src, options) {
            Ok(out) if out == src => {}
            Ok(out) => {
                changed += 1;
                if check {
                    println!("{display}");
                    ok = false;
                } else {
                    std::fs::write(path, out).map_err(|e| format!("can't write {display}: {e}"))?;
                }
            }
            Err(errors) => {
                report(&display, &src, &errors);
                ok = false;
            }
        }
    }
    let verb = if check {
        "would be reformatted"
    } else {
        "reformatted"
    };
    eprintln!("{changed} of {} files {verb}", files.len());
    Ok(ok)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if path.is_dir() {
            collect(&path, out);
        } else if name.ends_with(".styl") {
            out.push(path);
        }
    }
}

fn report(path: &str, src: &str, errors: &[stylet_syntax::SyntaxError]) {
    let index = LineIndex::new(src);
    for error in errors {
        let at = index.line_col(error.range().start());
        eprintln!(
            "{path}:{}:{}: error: {}",
            at.line + 1,
            at.col + 1,
            error.message()
        );
    }
}
