//! Parses every `.styl` file below `$STYLET_CORPUS` with the Stylus parser and
//! lists statements it doesn't understand.

use std::fs;
use std::path::PathBuf;
use stylet_migrate::parse::{Stmt, StmtKind, parse};

fn unknowns(stmts: &[Stmt], out: &mut Vec<(u32, String, String)>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Unknown { text, reason } => {
                out.push((stmt.line, text.clone(), reason.clone()))
            }
            StmtKind::Expr(text) => out.push((stmt.line, text.clone(), "expression".into())),
            StmtKind::Rule { body, .. }
            | StmtKind::Def { body, .. }
            | StmtKind::For { body, .. } => unknowns(body, out),
            StmtKind::AtRule {
                body: Some(body), ..
            } => unknowns(body, out),
            StmtKind::If { branches } => branches.iter().for_each(|b| unknowns(&b.body, out)),
            _ => {}
        }
    }
}

#[test]
#[ignore = "needs STYLET_CORPUS"]
fn corpus() {
    let root = PathBuf::from(std::env::var("STYLET_CORPUS").expect("set STYLET_CORPUS"));
    let mut stack = vec![root.clone()];
    let (mut files, mut total) = (0, 0);
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    stack.push(path);
                }
            } else if name.ends_with(".styl") {
                files += 1;
                let mut found = Vec::new();
                unknowns(&parse(&fs::read_to_string(&path).unwrap()), &mut found);
                for (line, text, reason) in &found {
                    eprintln!(
                        "{}:{line}: {reason}: {text}",
                        path.strip_prefix(&root).unwrap().display()
                    );
                }
                total += found.len();
            }
        }
    }
    eprintln!("{files} files, {total} unknown statements");
}
