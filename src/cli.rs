use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::collect::collect_final_artifacts;
use crate::export::copy_final_artifacts;

#[derive(Debug, Parser)]
#[command(
    name = "cargo-artifacts",
    bin_name = "cargo-artifacts",
    version,
    about = "List and copy artifacts from a `cargo build` run"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print artifact source paths
    List(ListArgs),
    /// Copy artifacts into a flat output directory.
    Copy(CopyArgs),
}

#[derive(Debug, Args)]
struct InputArgs {
    /// Read the `cargo build` message stream from this path. Use '-' for stdin.
    #[arg(short = 'i', long, value_name = "PATH")]
    input: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[command(flatten)]
    input: InputArgs,
}

#[derive(Debug, Args)]
struct CopyArgs {
    #[command(flatten)]
    input: InputArgs,
    /// Output directory for copied artifacts.
    #[arg(long, value_name = "DIR")]
    out_dir: PathBuf,
}

pub fn run() -> Result<()> {
    let args = normalize_args(std::env::args_os());
    let cli = Cli::parse_from(args);
    let stdin_is_terminal = io::stdin().is_terminal();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    dispatch(cli, &mut stdin, &mut stdout, &mut stderr, stdin_is_terminal)
}

pub fn run_from<I, S>(
    args: I,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    stdin_is_terminal: bool,
) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args = normalize_args(args);
    let cli = Cli::try_parse_from(args)?;

    dispatch(cli, stdin, stdout, stderr, stdin_is_terminal)
}

pub fn normalize_args<I, S>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    if args
        .get(1)
        .is_some_and(|arg| arg == OsStr::new("artifacts"))
    {
        args.remove(1);
    }
    args
}

fn dispatch(
    cli: Cli,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    stdin_is_terminal: bool,
) -> Result<()> {
    match cli.command {
        Commands::List(args) => {
            let artifacts = collect_from_input(&args.input, stdin, stdin_is_terminal)?;
            for artifact in artifacts {
                writeln!(stdout, "{}", artifact.source_path())?;
            }
            Ok(())
        }
        Commands::Copy(args) => {
            let artifacts = collect_from_input(&args.input, stdin, stdin_is_terminal)?;
            let warnings = copy_final_artifacts(&artifacts, &args.out_dir)?;
            for warning in warnings {
                writeln!(stderr, "warning: {warning}")?;
            }
            Ok(())
        }
    }
}

fn collect_from_input(
    args: &InputArgs,
    stdin: &mut dyn Read,
    stdin_is_terminal: bool,
) -> Result<Vec<crate::FinalArtifact>> {
    match args.input.as_deref() {
        Some(path) if path != Path::new("-") => {
            let file = File::open(path)
                .with_context(|| format!("failed to open input {}", path.display()))?;
            collect_final_artifacts(BufReader::new(file)).map_err(Into::into)
        }
        _ => {
            if args.input.is_none() && stdin_is_terminal {
                bail!(
                    "stdin is a terminal; pass --input <path> or pipe a Cargo Build Message Stream"
                );
            }

            collect_final_artifacts(BufReader::new(stdin)).map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_cargo_subcommand_shim_argument() {
        let args = normalize_args(["cargo-artifacts", "artifacts", "list"]);

        assert_eq!(
            args,
            vec![OsString::from("cargo-artifacts"), OsString::from("list"),]
        );
    }

    #[test]
    fn leaves_direct_invocation_arguments_unchanged() {
        let args = normalize_args(["cargo-artifacts", "list"]);

        assert_eq!(
            args,
            vec![OsString::from("cargo-artifacts"), OsString::from("list"),]
        );
    }

    #[test]
    fn terminal_stdin_without_input_fails_fast() {
        let mut stdin = io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let error = run_from(
            ["cargo-artifacts", "list"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            true,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("stdin is a terminal; pass --input <path>")
        );
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }
}
