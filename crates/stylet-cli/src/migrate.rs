//! `stylet migrate`.

use std::path::{Path, PathBuf};
use stylet_migrate::{Options, is_gap, issue_link, migrate, summary};
use stylet_resolve::{OsFs, normalize};

pub struct Args {
    pub entries: Vec<PathBuf>,
    pub root: PathBuf,
    /// Write to this directory (mirroring paths below `root`) instead of in place.
    pub out: Option<PathBuf>,
    pub dry_run: bool,
    pub quiet: bool,
    pub options: Options,
}

pub fn run(args: &Args, cwd: &Path) -> Result<bool, String> {
    let entries: Vec<PathBuf> = args
        .entries
        .iter()
        .map(|e| normalize(&cwd.join(e)))
        .collect();
    let migration = migrate(&OsFs, &args.root, &entries, &args.options);
    let display = |p: &Path| p.strip_prefix(cwd).unwrap_or(p).display().to_string();

    if !args.quiet {
        for w in &migration.warnings {
            let at = if w.line > 0 {
                format!(":{}", w.line)
            } else {
                String::new()
            };
            eprintln!(
                "{}{at}: warning[{}]: {}",
                display(&w.path),
                w.category,
                w.message
            );
        }
    }
    for e in &migration.errors {
        eprintln!("error: {e}");
    }
    eprintln!();
    eprintln!(
        "{} files converted, {} warnings",
        migration.files.len(),
        migration.warnings.len()
    );
    for (category, count) in summary(&migration.warnings) {
        eprintln!("  {count:>5}  {category}");
    }
    report_gaps(&migration.warnings);
    if !migration.errors.is_empty() {
        return Ok(false);
    }
    if args.dry_run {
        return Ok(true);
    }
    for (path, text) in &migration.files {
        let target = match &args.out {
            Some(out) => out.join(path.strip_prefix(&args.root).unwrap_or(path)),
            None => path.clone(),
        };
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("can't create {}: {e}", dir.display()))?;
        }
        std::fs::write(&target, text)
            .map_err(|e| format!("can't write {}: {e}", target.display()))?;
    }
    Ok(true)
}

/// Lists Stylus features the migration doesn't support yet, with links to
/// pre-filled issues.
fn report_gaps(warnings: &[stylet_migrate::Warning]) {
    let mut seen = std::collections::HashSet::new();
    let gaps: Vec<_> = warnings
        .iter()
        .filter(|w| is_gap(w) && seen.insert(w.message.clone()))
        .collect();
    if gaps.is_empty() {
        return;
    }
    eprintln!();
    eprintln!("Some Stylus features aren't supported by the migration yet. If you need one,");
    eprintln!("please open an issue (the links are pre-filled):");
    for w in gaps.iter().take(10) {
        let line = std::fs::read_to_string(&w.path).ok().and_then(|text| {
            text.lines()
                .nth(w.line.saturating_sub(1) as usize)
                .map(str::to_string)
        });
        eprintln!("  - {}", w.message);
        eprintln!(
            "    {}",
            issue_link(w, line.as_deref().filter(|_| w.line > 0))
        );
    }
    if gaps.len() > 10 {
        eprintln!("  … and {} more", gaps.len() - 10);
    }
}
