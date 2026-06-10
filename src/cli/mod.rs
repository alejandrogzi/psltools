// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

pub mod check;
pub mod convert;
pub mod filter;
pub mod merge;
pub mod score;
pub mod sort;
mod sort_core;
pub mod split;
pub mod stats;
pub mod swap;

use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::Path;

use clap::{Parser, Subcommand};
use log::LevelFilter;
use psltools::{PslError, PslRecord};

/// Output buffer capacity for file/stdout writers.
pub(crate) const OUTPUT_BUFFER_CAPACITY: usize = 1024 * 1024;

/// Command-line interface for psltools.
#[derive(Debug, Parser)]
#[command(name = "psltools")]
#[command(about = "work with .psl files in rust")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
pub struct Cli {
    #[arg(
        short = 't',
        long,
        global = true,
        value_name = "N",
        help_heading = "Global Options",
        default_value_t = num_cpus::get()
    )]
    threads: usize,

    #[arg(
        short = 'L',
        long,
        global = true,
        value_name = "LEVEL",
        help = "Set CLI logging level: off, error, warn, info, debug, trace",
        help_heading = "Global Options"
    )]
    level: Option<LevelFilter>,

    #[command(subcommand)]
    command: Command,
}

/// Subcommands available in the psltools CLI.
// Constructed exactly once per process from parsed args; the per-variant size
// difference is irrelevant to a top-level dispatch enum.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Validate PSL records against structural invariants")]
    Check(check::CheckArgs),
    #[command(about = "Convert PSL records to another format (BED12)")]
    Convert(convert::ConvertArgs),
    #[command(about = "Filter PSL records by score, identity, names, region, and more")]
    Filter(filter::FilterArgs),
    #[command(about = "Merge (optionally pre-sorted) PSL files")]
    Merge(merge::MergeArgs),
    #[command(about = "Report computed scores / identity (PSL has no score column)")]
    Score(score::ScoreArgs),
    #[command(about = "Sort PSL records by reference, query, score, or size")]
    Sort(sort::SortArgs),
    #[command(about = "Split PSL files by sequence, chunk count, records, or bytes")]
    Split(split::SplitArgs),
    #[command(about = "Summarize a PSL file")]
    Stats(stats::StatsArgs),
    #[command(about = "Swap query and reference (UCSC pslSwap-compatible)")]
    Swap(swap::SwapArgs),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Check(_) => f.write_str("check"),
            Command::Convert(_) => f.write_str("convert"),
            Command::Filter(_) => f.write_str("filter"),
            Command::Merge(_) => f.write_str("merge"),
            Command::Score(_) => f.write_str("score"),
            Command::Sort(_) => f.write_str("sort"),
            Command::Split(_) => f.write_str("split"),
            Command::Stats(_) => f.write_str("stats"),
            Command::Swap(_) => f.write_str("swap"),
        }
    }
}

/// Errors that can occur during CLI execution.
#[derive(Debug)]
pub enum CliError {
    Message(String),
    Io(io::Error),
    Psl(PslError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Message(message) => f.write_str(message),
            CliError::Io(err) => write!(f, "I/O error: {err}"),
            CliError::Psl(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Io(err) => Some(err),
            CliError::Psl(err) => Some(err),
            CliError::Message(_) => None,
        }
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        CliError::Io(value)
    }
}

impl From<PslError> for CliError {
    fn from(value: PslError) -> Self {
        CliError::Psl(value)
    }
}

/// Main CLI entry point. Returns the process exit code.
pub fn run<R, W, E>(
    cli: Cli,
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    configure_threads(cli.threads)?;
    configure_logging(resolve_log_level(cli.level))?;

    log::info!("psltools [{}] v{}", &cli.command, env!("CARGO_PKG_VERSION"));
    let start = std::time::Instant::now();

    let code = match cli.command {
        Command::Check(args) => check::run(args, stdin, stdout, stderr)?,
        Command::Convert(args) => {
            convert::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Filter(args) => {
            filter::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Merge(args) => {
            merge::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Score(args) => {
            score::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Sort(args) => {
            sort::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Split(args) => {
            split::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Stats(args) => {
            stats::run(args, stdin, stdout, stderr)?;
            0
        }
        Command::Swap(args) => {
            swap::run(args, stdin, stdout, stderr)?;
            0
        }
    };

    log::info!("Execution time: {:?}", start.elapsed());
    Ok(code)
}

/// Logging is verbose by default (Info), emitted on stderr so it never corrupts
/// the stdout PSL stream. An explicit `--level` is always honored.
fn resolve_log_level(requested: Option<LevelFilter>) -> LevelFilter {
    requested.unwrap_or(LevelFilter::Info)
}

/// Emits a uniform end-of-run summary at Info: `"{tool} complete: k=v, ..."`.
pub(crate) fn log_summary(tool: &str, fields: &[(&str, u64)]) {
    if !log::log_enabled!(log::Level::Info) {
        return;
    }
    let rendered = fields
        .iter()
        .map(|(label, value)| format!("{label}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    log::info!("{tool} complete: {rendered}");
}

/// Verifies that the given input files exist (pre-flight check).
pub(crate) fn ensure_inputs_exist(paths: &[&Path]) -> Result<(), CliError> {
    for path in paths {
        match path.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Err(CliError::Message(format!(
                    "input file does not exist: {}",
                    path.display()
                )));
            }
            Err(err) => {
                return Err(CliError::Message(format!(
                    "cannot access input file {}: {err}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn configure_logging(level: LevelFilter) -> Result<(), CliError> {
    log::set_max_level(level);
    if level == LevelFilter::Off {
        return Ok(());
    }
    // Ignore an already-initialized logger (e.g. across tests in one process).
    let _ = simple_logger::SimpleLogger::new().with_level(level).init();
    log::set_max_level(level);
    Ok(())
}

fn configure_threads(threads: usize) -> Result<(), CliError> {
    // Ignore failure: the global pool may already be configured in-process.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
    Ok(())
}

/// Opens the configured output (a file, or `stdout`), optionally gzip-wrapped,
/// and runs `write_body` against it, flushing/finishing on completion.
pub(crate) fn write_output<W: Write>(
    out_path: Option<&Path>,
    gzip: bool,
    stdout: &mut W,
    write_body: impl FnOnce(&mut dyn Write) -> Result<(), CliError>,
) -> Result<(), CliError> {
    #[cfg(not(feature = "gzip"))]
    if gzip {
        return Err(CliError::Message(
            "--gzip requires psltools to be built with the `gzip` feature".to_owned(),
        ));
    }
    match out_path {
        Some(path) => {
            let base = BufWriter::with_capacity(OUTPUT_BUFFER_CAPACITY, File::create(path)?);
            finish_output(gzip, base, write_body)
        }
        None => finish_output(gzip, &mut *stdout, write_body),
    }
}

fn finish_output<B: Write>(
    gzip: bool,
    mut base: B,
    write_body: impl FnOnce(&mut dyn Write) -> Result<(), CliError>,
) -> Result<(), CliError> {
    if gzip {
        #[cfg(feature = "gzip")]
        {
            let mut encoder = flate2::write::GzEncoder::new(&mut base, flate2::Compression::fast());
            write_body(&mut encoder)?;
            encoder.try_finish()?;
            drop(encoder);
            base.flush()?;
            return Ok(());
        }
        #[cfg(not(feature = "gzip"))]
        {
            unreachable!("gzip guarded in write_output");
        }
    }
    write_body(&mut base)?;
    base.flush()?;
    Ok(())
}

/// Writes one record to a `&mut dyn Write` (works around the unsized-`dyn`
/// bound on [`psltools::write_psl`]'s generic writer).
pub(crate) fn emit_record<P: PslRecord>(mut w: &mut dyn Write, record: &P) -> io::Result<()> {
    psltools::write_psl(&mut w, record)
}

/// Validates that `--gzip` is usable in this build.
pub(crate) fn ensure_gzip_available(gzip: bool) -> Result<(), CliError> {
    #[cfg(not(feature = "gzip"))]
    if gzip {
        return Err(CliError::Message(
            "--gzip requires psltools to be built with the `gzip` feature".to_owned(),
        ));
    }
    let _ = gzip;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_info_when_unspecified() {
        assert_eq!(resolve_log_level(None), LevelFilter::Info);
    }

    #[test]
    fn honors_requested_level() {
        assert_eq!(
            resolve_log_level(Some(LevelFilter::Debug)),
            LevelFilter::Debug
        );
    }

    #[test]
    fn parses_globals_before_and_after_subcommand() {
        let cli = Cli::try_parse_from(["psltools", "--threads", "4", "sort"]).unwrap();
        assert_eq!(cli.threads, 4);
        let cli = Cli::try_parse_from(["psltools", "sort", "--threads", "2"]).unwrap();
        assert_eq!(cli.threads, 2);
    }
}
