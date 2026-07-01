//! `nasmlint` — the nasm-lint command-line interface.
//!
//! Responsibilities are deliberately thin: parse arguments, load config, discover
//! files, feed each to `nasmlint_core::analyze`, render, and choose an exit code.
//! All analysis logic lives in the core crate so the LSP can reuse it verbatim.

mod discover;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use nasmlint_core::{analyze, Config, Severity, SourceFile};

use render::FileResult;

/// Default config filename searched for in the current directory.
const CONFIG_FILENAME: &str = ".nasmlint.toml";

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
    Sarif,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SeverityArg {
    MustFix,
    ShouldFix,
    Consider,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::MustFix => Severity::MustFix,
            SeverityArg::ShouldFix => Severity::ShouldFix,
            SeverityArg::Consider => Severity::Consider,
        }
    }
}

/// Static analysis for NASM assembly.
#[derive(Debug, Parser)]
#[command(name = "nasmlint", version, about)]
struct Cli {
    /// Files or directories to analyze (directories are walked for .asm/.nasm/.s/.inc).
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value = "human")]
    format: Format,

    /// Path to a config file (defaults to ./.nasmlint.toml if present).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Exit non-zero when any finding is at least this severe.
    #[arg(long, value_enum, default_value = "must-fix")]
    max_severity: SeverityArg,
}

fn load_config(explicit: Option<&PathBuf>) -> Result<Config> {
    let path = match explicit {
        Some(p) => Some(p.clone()),
        None => {
            let default = PathBuf::from(CONFIG_FILENAME);
            default.is_file().then_some(default)
        }
    };
    match path {
        Some(p) => {
            let text = std::fs::read_to_string(&p)
                .with_context(|| format!("reading config {}", p.display()))?;
            Config::from_toml(&text).with_context(|| format!("parsing config {}", p.display()))
        }
        None => Ok(Config::default()),
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let config = load_config(cli.config.as_ref())?;
    let ignore = discover::build_ignore(&config.ignore)?;
    let files = discover::collect_files(&cli.paths, &ignore)?;

    let mut results = Vec::with_capacity(files.len());
    for path in &files {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let source = SourceFile::new(path.clone(), text);
        let diagnostics = analyze(&source, &config);
        results.push(FileResult { path, diagnostics });
    }

    let output = match cli.format {
        Format::Human => render::human(&results),
        Format::Json => render::json(&results),
        Format::Sarif => render::sarif(&results),
    };
    print!("{output}");

    // Gate: fail only if a finding meets the configured threshold.
    let gate: Severity = cli.max_severity.into();
    let failed = render::max_severity(&results).is_some_and(|worst| worst.meets(gate));
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("nasmlint: {err:#}");
            ExitCode::FAILURE
        }
    }
}
