// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use genepred::{Bed3, Bed4, Bed5, Bed6, Bed8, Bed9, Bed12, GenePred};
use psltools::{StreamingReader, to_genepred};

use super::{CliError, ensure_inputs_exist, write_output};

/// Arguments for the `convert` subcommand.
#[derive(Debug, Args)]
pub struct ConvertArgs {
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
        long = "out",
        value_name = "PATH",
        help = "Output path (default stdout)."
    )]
    out: Option<PathBuf>,

    #[arg(short = 'G', long = "gzip", help = "Compress output with gzip.")]
    gzip: bool,

    #[arg(
        long = "type",
        value_name = "N",
        default_value_t = 12,
        help = "BED layout to emit: 3, 4, 5, 6, 8, 9, or 12."
    )]
    bed_type: u8,
}

/// Runs the `convert` subcommand. Emits one BED line per record (over the
/// reference sequence), at the requested BED width.
pub fn run<R, W, E>(
    args: ConvertArgs,
    stdin: &mut R,
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    validate_bed_type(args.bed_type)?;
    let input_refs: Vec<&std::path::Path> = args.inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;

    let mut written = 0u64;
    write_output(args.out.as_deref(), args.gzip, stdout, |w| {
        if args.inputs.is_empty() {
            let mut reader = StreamingReader::new(stdin);
            written += convert(&mut reader, w, args.bed_type)?;
        } else {
            for input in &args.inputs {
                let mut reader = StreamingReader::from_path(input)?;
                written += convert(&mut reader, w, args.bed_type)?;
            }
        }
        Ok(())
    })?;

    super::log_summary("convert", &[("written", written)]);
    Ok(())
}

fn validate_bed_type(bed_type: u8) -> Result<(), CliError> {
    match bed_type {
        3 | 4 | 5 | 6 | 8 | 9 | 12 => Ok(()),
        n => Err(CliError::Message(format!(
            "unsupported --type {n}; choose one of 3, 4, 5, 6, 8, 9, 12"
        ))),
    }
}

fn convert<R: BufRead>(
    reader: &mut StreamingReader<R>,
    w: &mut dyn Write,
    bed_type: u8,
) -> Result<u64, CliError> {
    let mut count = 0u64;
    while let Some(record) = reader.next_record()? {
        let gene = to_genepred(&record);
        w.write_all(&render(&gene, bed_type))?;
        w.write_all(b"\n")?;
        count += 1;
    }
    Ok(count)
}

/// Renders a `GenePred` as a BED line of the requested width.
fn render(gene: &GenePred, bed_type: u8) -> Vec<u8> {
    match bed_type {
        3 => gene.to_bed::<Bed3>(),
        4 => gene.to_bed::<Bed4>(),
        5 => gene.to_bed::<Bed5>(),
        6 => gene.to_bed::<Bed6>(),
        8 => gene.to_bed::<Bed8>(),
        9 => gene.to_bed::<Bed9>(),
        _ => gene.to_bed::<Bed12>(),
    }
}
