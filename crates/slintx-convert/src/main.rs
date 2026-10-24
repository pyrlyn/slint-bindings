//! `slint-convert`: the command-line front end. Argument parsing, file I/O and
//! printing diagnostics only; the conversion lives in the library.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use slintx_convert::{ConvertError, Format, Options, Platform};

/// Convert a .slint file to XML, HTML, an Apple Storyboard or GTK Blueprint.
#[derive(Debug, Parser)]
#[command(name = "slint-convert", version)]
struct Cli {
    /// The .slint file to convert.
    input: PathBuf,
    /// Output format.
    #[arg(long = "to", value_enum)]
    to: Target,
    /// Write here instead of standard output.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Storyboard framework.
    #[arg(long, value_enum, default_value_t = Apple::Uikit)]
    platform: Apple,
    /// Treat every warning as an error.
    #[arg(long)]
    strict: bool,
    /// Tag every element with its .slint line and column (GTK widget name,
    /// AppKit identifier, UIKit accessibility identifier).
    #[arg(long)]
    source_tags: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Target {
    Xml,
    Html,
    Storyboard,
    Blueprint,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Apple {
    Uikit,
    Appkit,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let source = std::fs::read_to_string(&cli.input)
        .with_context(|| format!("reading {}", cli.input.display()))?;
    let format = match cli.to {
        Target::Xml => Format::Xml,
        Target::Html => Format::Html,
        Target::Storyboard => Format::Storyboard,
        Target::Blueprint => Format::Blueprint,
    };
    let options = Options {
        platform: match cli.platform {
            Apple::Uikit => Platform::UiKit,
            Apple::Appkit => Platform::AppKit,
        },
        strict: cli.strict,
        source_tags: cli.source_tags,
    };
    let file = cli.input.display().to_string();
    match slintx_convert::convert(&file, &source, format, &options) {
        Ok(conversion) => {
            for w in &conversion.warnings {
                eprintln!("{w}");
            }
            match &cli.output {
                Some(path) => std::fs::write(path, &conversion.output)
                    .with_context(|| format!("writing {}", path.display()))?,
                None => print!("{}", conversion.output),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(ConvertError::Failed { diagnostics }) => {
            for d in &diagnostics {
                eprintln!("{d}");
            }
            let errors = diagnostics
                .iter()
                .filter(|d| d.severity == slintx_convert::Severity::Error)
                .count();
            eprintln!("error: conversion failed with {errors} error(s)");
            Ok(ExitCode::FAILURE)
        }
    }
}
