//! `stylet build`.

use crate::config::Config;
use crate::report;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use stylet_compile::{Options, compile_file};
use stylet_resolve::{Loader, OsFs, normalize};

pub struct Job {
    pub input: PathBuf,
    /// `None` writes to stdout.
    pub output: Option<PathBuf>,
}

pub struct Build {
    pub jobs: Vec<Job>,
    pub options: Options,
    pub cwd: PathBuf,
}

impl Build {
    /// Builds every job; returns whether all succeeded, and every file read.
    fn run(&self, loader: &mut Loader<OsFs>) -> (bool, HashSet<PathBuf>) {
        let mut ok = true;
        let mut dependencies = HashSet::new();
        for job in &self.jobs {
            let start = Instant::now();
            let options = Options {
                output: job.output.clone(),
                source_map: self.options.source_map && job.output.is_some(),
                ..self.options.clone()
            };
            let out = compile_file(loader, &job.input, &options);
            dependencies.extend(out.dependencies.iter().cloned());
            for diagnostic in &out.diagnostics {
                eprint!("{}", report::render(loader, diagnostic, &self.cwd));
            }
            if out.has_errors() {
                eprintln!("✗ {}", self.display(&job.input));
                ok = false;
                continue;
            }
            let written = match &job.output {
                Some(path) => write_output(path, &out.css, out.source_map.as_deref()),
                None => std::io::stdout()
                    .write_all(out.css.as_bytes())
                    .map_err(|e| e.to_string()),
            };
            match (written, &job.output) {
                (Err(e), _) => {
                    eprintln!("error: {e}");
                    ok = false;
                }
                (Ok(()), Some(path)) => {
                    eprintln!(
                        "✓ {} → {} ({:.0?})",
                        self.display(&job.input),
                        self.display(path),
                        start.elapsed()
                    );
                }
                (Ok(()), None) => {}
            }
        }
        (ok, dependencies)
    }

    fn display(&self, path: &Path) -> String {
        match path.strip_prefix(&self.cwd) {
            Ok(p) if p.as_os_str().is_empty() => ".".into(),
            Ok(p) => p.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }
}

fn write_output(path: &Path, css: &str, source_map: Option<&str>) -> Result<(), String> {
    let error = |e: std::io::Error| format!("can't write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(error)?;
    }
    let mut css = css.to_string();
    if let Some(map) = source_map {
        let map_path = PathBuf::from(format!("{}.map", path.display()));
        std::fs::write(&map_path, map).map_err(error)?;
        let name = map_path.file_name().unwrap_or_default().to_string_lossy();
        css += &format!("/*# sourceMappingURL={name} */\n");
    }
    std::fs::write(path, css).map_err(error)
}

pub fn build(config: &Config, build: &Build) -> bool {
    let mut loader = Loader::new(OsFs, config.resolve());
    build.run(&mut loader).0
}

/// Builds, then rebuilds whenever an input changes. Runs until interrupted.
pub fn watch(config: &Config, build: &Build) -> Result<(), String> {
    use notify::{RecursiveMode, Watcher};

    let mut loader = Loader::new(OsFs, config.resolve());
    let (_, mut dependencies) = build.run(&mut loader);
    let outputs: HashSet<PathBuf> = build
        .jobs
        .iter()
        .filter_map(|j| j.output.clone())
        .flat_map(|o| [PathBuf::from(format!("{}.map", o.display())), o])
        .collect();

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
    let root = config.root();
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| format!("can't watch {}: {e}", root.display()))?;
    eprintln!("watching {} for changes…", build.display(&root));

    loop {
        let Ok(first) = rx.recv() else { return Ok(()) };
        // Collect a burst of events (editors often write several times).
        let mut changed: HashSet<PathBuf> = HashSet::new();
        let mut add = |event: notify::Result<notify::Event>| {
            if let Ok(event) = event
                && !event.kind.is_access()
            {
                changed.extend(event.paths.iter().map(|p| normalize(p)));
            }
        };
        add(first);
        while let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            add(event);
        }
        changed.retain(|p| !outputs.contains(p));
        let relevant = changed.iter().any(|p| {
            dependencies.contains(p) || p.extension().is_some_and(|e| e == "styl" || e == "css")
        });
        if !relevant {
            continue;
        }
        for path in &changed {
            loader.invalidate(path);
        }
        dependencies = build.run(&mut loader).1;
    }
}
