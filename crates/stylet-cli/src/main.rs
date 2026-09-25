mod build;
mod config;
mod fmt;
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
        Command::Fmt { paths, check } => {
            let options = config.fmt.options()?;
            fmt::run(&paths, check, &options, &cwd)
        }
    }
}
