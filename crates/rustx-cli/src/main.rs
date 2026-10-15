//! `rustx-cli`: command-line tools over the cargo-rustx desugarer.

mod diagnostic;
mod expand;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Command-line tools for cargo-rustx sources.
#[derive(Parser)]
#[command(name = "rustx-cli", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the plain Rust that a file desugars to.
    Expand {
        /// A `.rustx` file, which is sugar throughout, or a Rust file whose `sugar! { .. }`
        /// invocations are expanded in place.
        file: PathBuf,
    },
    /// Print a plain Rust file rewritten into sugar, as the text of a `.rustx` file.
    Sugar {
        /// A Rust file; only code that sugar expands back to exactly is rewritten.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            // Nothing is left to report to if stderr itself fails, so its error is dropped.
            let _ = writeln!(std::io::stderr(), "{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let output = match cli.command {
        Command::Expand { file } => convert(&file, |source| {
            expand::expand_source(source, expand::Kind::of(&file))
        })?,
        Command::Sugar { file } => convert(&file, rustx_core::sugar)?,
    };
    // `print!` panics on a closed pipe (`rustx-cli expand f | head`); a write error is
    // reported instead.
    std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|err| format!("error: cannot write the output: {err}"))
}

/// Reads `file` and converts its text, rendering errors with the file's positions.
fn convert(
    file: &Path,
    convert: impl FnOnce(&str) -> syn::Result<String>,
) -> Result<String, String> {
    let source =
        std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
    convert(&source).map_err(|err| diagnostic::render(file, &source, err))
}
