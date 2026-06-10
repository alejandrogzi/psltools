// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use psltools::{StreamingReader, check};

use super::{CliError, ensure_inputs_exist};

/// Arguments for the `check` subcommand.
#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(
        short = 'c',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl file(s). If omitted, read from standard input.",
        value_delimiter = ' ',
        num_args = 1..,
    )]
    inputs: Vec<PathBuf>,

    #[arg(
        long = "warn-only",
        help = "Report violations but always exit 0 (otherwise exit 1 when any record fails)."
    )]
    warn_only: bool,
}

/// Runs the `check` subcommand. Returns the process exit code.
pub fn run<R, W, E>(
    args: CheckArgs,
    stdin: &mut R,
    _stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    let input_refs: Vec<&std::path::Path> = args.inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;

    let mut checked = 0u64;
    let mut failed = 0u64;

    if args.inputs.is_empty() {
        let mut reader = StreamingReader::new(stdin);
        process(&mut reader, "<stdin>", stderr, &mut checked, &mut failed)?;
    } else {
        for input in &args.inputs {
            let label = input.display().to_string();
            let mut reader = StreamingReader::from_path(input)?;
            process(&mut reader, &label, stderr, &mut checked, &mut failed)?;
        }
    }

    super::log_summary("check", &[("checked", checked), ("failed", failed)]);
    if failed > 0 && !args.warn_only {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn process<R: BufRead, E: Write>(
    reader: &mut StreamingReader<R>,
    label: &str,
    stderr: &mut E,
    checked: &mut u64,
    failed: &mut u64,
) -> Result<(), CliError> {
    let mut record_no = 0u64;
    while let Some(record) = reader.next_record()? {
        record_no += 1;
        *checked += 1;
        let report = check(&record);
        if !report.is_ok() {
            *failed += 1;
            let qname = String::from_utf8_lossy(&record.query_name);
            let rname = String::from_utf8_lossy(&record.reference_name);
            for violation in &report.violations {
                writeln!(
                    stderr,
                    "{label}:{record_no}: {qname} -> {rname}: {violation}"
                )?;
            }
        }
    }
    Ok(())
}
