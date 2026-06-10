// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use psltools::{OwnedPsl, StreamingReader};

use super::{CliError, ensure_inputs_exist, write_output};

/// The reported metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Metric {
    /// UCSC `pslScore`.
    Score,
    /// UCSC `pslCalcMilliBad`.
    MilliBad,
    /// Percent identity (`100 - milliBad * 0.1`).
    PercentId,
}

/// Arguments for the `score` subcommand.
#[derive(Debug, Args)]
pub struct ScoreArgs {
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
        help = "Output TSV path. If omitted, write to standard output."
    )]
    out: Option<PathBuf>,

    #[arg(short = 'G', long = "gzip", help = "Compress output with gzip.")]
    gzip: bool,

    #[arg(
        long = "metric",
        value_enum,
        default_value_t = Metric::Score,
        help = "Which metric to report."
    )]
    metric: Metric,

    #[arg(
        long = "mrna",
        help = "Treat alignments as mRNA for identity (affects milliBad/percentId)."
    )]
    mrna: bool,

    #[arg(
        long = "sort-by-score",
        help = "Emit rows ordered by computed pslScore, descending."
    )]
    sort_by_score: bool,
}

/// Runs the `score` subcommand. Emits `queryName<TAB>referenceName<TAB>value`.
pub fn run<R, W, E>(
    args: ScoreArgs,
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

    let mut rows = 0u64;
    write_output(args.out.as_deref(), args.gzip, stdout, |w| {
        if args.sort_by_score {
            let mut records = Vec::new();
            collect(&args, stdin, &mut records)?;
            records.sort_unstable_by_key(|r: &OwnedPsl| std::cmp::Reverse(r.score()));
            for record in &records {
                write_row(w, record, &args)?;
                rows += 1;
            }
        } else if args.inputs.is_empty() {
            let mut reader = StreamingReader::new(stdin);
            rows += stream_rows(&mut reader, w, &args)?;
        } else {
            for input in &args.inputs {
                let mut reader = StreamingReader::from_path(input)?;
                rows += stream_rows(&mut reader, w, &args)?;
            }
        }
        Ok(())
    })?;

    super::log_summary("score", &[("rows", rows)]);
    Ok(())
}

fn collect<R: BufRead>(
    args: &ScoreArgs,
    stdin: &mut R,
    out: &mut Vec<OwnedPsl>,
) -> Result<(), CliError> {
    if args.inputs.is_empty() {
        let mut reader = StreamingReader::new(stdin);
        while let Some(record) = reader.next_record()? {
            out.push(record);
        }
    } else {
        for input in &args.inputs {
            let mut reader = StreamingReader::from_path(input)?;
            while let Some(record) = reader.next_record()? {
                out.push(record);
            }
        }
    }
    Ok(())
}

fn stream_rows<R: BufRead>(
    reader: &mut StreamingReader<R>,
    w: &mut dyn Write,
    args: &ScoreArgs,
) -> Result<u64, CliError> {
    let mut count = 0u64;
    while let Some(record) = reader.next_record()? {
        write_row(w, &record, args)?;
        count += 1;
    }
    Ok(count)
}

fn write_row(w: &mut dyn Write, record: &OwnedPsl, args: &ScoreArgs) -> Result<(), CliError> {
    w.write_all(&record.query_name)?;
    w.write_all(b"\t")?;
    w.write_all(&record.reference_name)?;
    w.write_all(b"\t")?;
    match args.metric {
        Metric::Score => writeln!(w, "{}", record.score())?,
        Metric::MilliBad => writeln!(w, "{}", record.milli_bad(args.mrna))?,
        Metric::PercentId => writeln!(w, "{:.1}", record.percent_id(args.mrna))?,
    }
    Ok(())
}
