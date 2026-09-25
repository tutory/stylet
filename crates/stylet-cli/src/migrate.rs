//! `stylet migrate`.

use std::path::{Path, PathBuf};
use stylet_migrate::{Options, migrate, summary};
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
