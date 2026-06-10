// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use psltools::{StreamingReader, swap_with};

use super::{CliError, emit_record, ensure_inputs_exist, write_output};

/// Arguments for the `swap` subcommand.
#[derive(Debug, Args)]
pub struct SwapArgs {
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
        short = 'o',
        long = "out-psl",
        value_name = "PATH",
        help = "Output path. If omitted, write to standard output."
    )]
    out: Option<PathBuf>,

    #[arg(short = 'G', long = "gzip", help = "Compress output with gzip.")]
    gzip: bool,

    #[arg(
        long = "no-rc",
        help = "Do not reverse-complement untranslated minus-strand records; make the target strand explicit instead."
    )]
    no_rc: bool,
}

/// Runs the `swap` subcommand.
pub fn run<R, W, E>(
    args: SwapArgs,
    stdin: &mut R,
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    let input_refs: Vec<&std::path::Path> = args.inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;

    let mut swapped = 0u64;
    write_output(args.out.as_deref(), args.gzip, stdout, |w| {
        if args.inputs.is_empty() {
            let mut reader = StreamingReader::new(stdin);
            swapped += process(&mut reader, w, args.no_rc)?;
        } else {
            for input in &args.inputs {
                let mut reader = StreamingReader::from_path(input)?;
                swapped += process(&mut reader, w, args.no_rc)?;
            }
        }
        Ok(())
    })?;

    super::log_summary("swap", &[("swapped", swapped)]);
    Ok(())
}

fn process<R: BufRead>(
    reader: &mut StreamingReader<R>,
    w: &mut dyn Write,
    no_rc: bool,
) -> Result<u64, CliError> {
    let mut count = 0u64;
    while let Some(record) = reader.next_record()? {
        let swapped = swap_with(&record, no_rc);
        emit_record(&mut *w, &swapped)?;
        count += 1;
    }
    Ok(count)
}
