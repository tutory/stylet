mod build;
mod config;
mod fmt;
mod migrate;
mod report;

use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use std::process::ExitCode;
use stylet_compile::Options;

#[derive(Parser)]
#[command(name = "stylet", version, about = "A small successor to Stylus")]
struct Cli {
    /// Path to stylet.toml (default: searched from the current directory upwards).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile to CSS. Without inputs, builds the `[[entry]]`s of stylet.toml.
    Build {
        /// Entry files.
        inputs: Vec<PathBuf>,
        /// Output file (only with a single input; default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Strip whitespace and comments.
        #[arg(long)]
        minify: bool,
        /// Write `<output>.map` source maps.
        #[arg(long)]
        source_map: bool,
        /// Substitute `@custom-media` references.
        #[arg(long)]
        resolve_custom_media: bool,
        /// Rebuild when inputs change.
        #[arg(short, long)]
        watch: bool,
    },
    /// Convert Stylus files to stylet, following the imports of the given entries.
    /// Files are rewritten in place unless `--out` is given.
    Migrate {
        /// Stylus entry files, e.g. `client/index.styl`.
        #[arg(required = true)]
        entries: Vec<PathBuf>,
        /// Directory `/`-prefixed imports resolve against (default: stylet.toml root or cwd).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Write converted files below this directory instead of in place.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only report; don't write anything.
        #[arg(long)]
        dry_run: bool,
        /// Only print the summary, not every warning.
        #[arg(short, long)]
        quiet: bool,
        /// `props`: global variables become custom properties; `inline`: always inline.
        #[arg(long, default_value = "props", value_parser = ["props", "inline"])]
        vars: String,
        /// Prefix for generated custom property names.
        #[arg(long, default_value = "")]
        var_prefix: String,
        /// Unroll `for` loops instead of commenting them out.
        #[arg(long)]
        unroll_loops: bool,
        /// Turn variables in media conditions into `@custom-media` (build with
        /// `resolve_custom_media = true` until browsers support it).
        #[arg(long)]
        custom_media: bool,
        /// Value for an identifier Stylus got from JS, e.g. `isDevelopment=false`.
        #[arg(long = "define", value_name = "NAME=VALUE")]
        defines: Vec<String>,
        /// Stylus file whose mixins and functions are available but which isn't
        /// converted, e.g. `node_modules/axis/axis/index.styl`.
        #[arg(long)]
        preload: Vec<PathBuf>,
    },
    /// Format files in place. Directories are searched for `.styl` files;
    /// `-` formats stdin to stdout.
    Fmt {
        /// Files or directories (default: the current directory).
        paths: Vec<PathBuf>,
        /// Only report files that aren't formatted; exit with 1 if there are any.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<bool, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = Config::load(cli.config.as_deref(), &cwd)?;
    match cli.command {
        Command::Build {
            inputs,
            output,
            minify,
            source_map,
            resolve_custom_media,
            watch,
        } => {
            let jobs = if inputs.is_empty() {
                if output.is_some() {
                    return Err("`--output` needs an input file".into());
                }
                if config.entries.is_empty() {
                    return Err(format!(
                        "no inputs given and no [[entry]] in {}",
                        config::FILE_NAME
                    ));
                }
                config
                    .entries
                    .iter()
                    .map(|e| build::Job {
                        input: config.path(&e.input),
                        output: Some(config.path(&e.output)),
                    })
                    .collect()
            } else {
                if output.is_some() && inputs.len() > 1 {
                    return Err("`--output` only works with a single input".into());
                }
                if output.is_none() && inputs.len() > 1 {
                    return Err(
                        "several inputs need [[entry]]s in stylet.toml or one `--output` each"
                            .into(),
                    );
                }
                inputs
                    .iter()
                    .map(|input| build::Job {
                        input: stylet_resolve::normalize(&cwd.join(input)),
                        output: output
                            .as_ref()
                            .map(|o| stylet_resolve::normalize(&cwd.join(o))),
                    })
                    .collect()
            };
            let build = build::Build {
                jobs,
                options: Options {
                    minify: minify || config.build.minify,
                    source_map: source_map || config.build.source_map,
                    resolve_custom_media: resolve_custom_media || config.build.resolve_custom_media,
                    output: None,
                },
                cwd,
            };
            if watch {
                build::watch(&config, &build)?;
                Ok(true)
            } else {
                Ok(build::build(&config, &build))
            }
        }
        Command::Migrate {
            entries,
            root,
            out,
            dry_run,
            quiet,
            vars,
            var_prefix,
            unroll_loops,
            custom_media,
            defines,
            preload,
        } => {
            let defines = defines
                .iter()
                .map(|d| {
                    d.split_once('=')
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .ok_or_else(|| format!("`--define {d}`: expected NAME=VALUE"))
                })
                .collect::<Result<_, _>>()?;
            let args = migrate::Args {
                entries,
                root: root.map_or_else(
                    || config.root(),
                    |r| stylet_resolve::normalize(&cwd.join(r)),
                ),
                out: out.map(|o| stylet_resolve::normalize(&cwd.join(o))),
                dry_run,
                quiet,
                options: stylet_migrate::Options {
                    vars: if vars == "inline" {
                        stylet_migrate::VarMode::Inline
                    } else {
                        stylet_migrate::VarMode::Props
                    },
                    var_prefix,
                    unroll_loops,
                    custom_media,
                    defines,
                    preload: preload
                        .iter()
                        .map(|p| stylet_resolve::normalize(&cwd.join(p)))
                        .collect(),
                },
            };
            migrate::run(&args, &cwd)
        }
        Command::Fmt { paths, check } => {
            let options = config.fmt.options()?;
            fmt::run(&paths, check, &options, &cwd)
        }
    }
}
