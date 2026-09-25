//! Prints diagnostics with a source excerpt.

use std::fmt::Write;
use std::path::Path;
use stylet_compile::{Diagnostic, Severity};
use stylet_resolve::Loader;

pub fn render<F>(loader: &Loader<F>, diagnostic: &Diagnostic, cwd: &Path) -> String {
    let label = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let message = diagnostic
        .message
        .replace(&format!("{}/", cwd.display()), "");
    let mut out = format!("{label}: {message}\n");
    let Some(file) = diagnostic.file else {
        return out;
    };
    let file = loader.file(file);
    let start = file.line_index.line_col(diagnostic.range.start());
    let end = file.line_index.line_col(diagnostic.range.end());
    let path = file.path.strip_prefix(cwd).unwrap_or(&file.path);
    let line_no = (start.line + 1).to_string();
    let pad = " ".repeat(line_no.len());
    let _ = writeln!(
        out,
        "{pad}--> {}:{}:{}",
        path.display(),
        start.line + 1,
        start.col + 1
    );
    let text = file.line_index.line_text(start.line);
    let before: String = text
        .encode_utf16()
        .take(start.col as usize)
        .map(|_| ' ')
        .collect();
    let width = if end.line == start.line {
        (end.col - start.col).max(1) as usize
    } else {
        text.encode_utf16()
            .count()
            .saturating_sub(start.col as usize)
            .max(1)
    };
    let _ = writeln!(out, "{pad} |");
    let _ = writeln!(out, "{line_no} | {text}");
    let _ = writeln!(out, "{pad} | {before}{}", "^".repeat(width));
    out
}
