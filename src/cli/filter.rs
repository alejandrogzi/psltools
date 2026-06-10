// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use psltools::{Coord, OwnedPsl, OwnedPslHeader, Strand, StreamingReader};

use super::{CliError, emit_record, ensure_inputs_exist, write_output};

/// Arguments for the `filter` subcommand. All predicates are optional and
/// AND-combined; `--invert` negates the combined result.
#[derive(Debug, Args)]
pub struct FilterArgs {
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
        help = "Output path (default stdout)."
    )]
    out: Option<PathBuf>,

    #[arg(short = 'G', long = "gzip", help = "Compress output with gzip.")]
    gzip: bool,

    #[arg(
        long = "min-match",
        value_name = "N",
        help = "Keep records with matches >= N."
    )]
    min_match: Option<u32>,

    #[arg(
        long = "min-score",
        value_name = "N",
        help = "Keep records with pslScore >= N."
    )]
    min_score: Option<i64>,

    #[arg(
        long = "min-identity",
        value_name = "PCT",
        help = "Keep records with percent identity >= PCT."
    )]
    min_identity: Option<f64>,

    #[arg(long = "min-query-size", value_name = "N")]
    min_query_size: Option<Coord>,
    #[arg(long = "max-query-size", value_name = "N")]
    max_query_size: Option<Coord>,
    #[arg(long = "min-ref-size", value_name = "N")]
    min_ref_size: Option<Coord>,
    #[arg(long = "max-ref-size", value_name = "N")]
    max_ref_size: Option<Coord>,

    #[arg(long = "strand", value_name = "+|-", value_parser = parse_strand, help = "Keep records with this query strand.")]
    strand: Option<Strand>,

    #[arg(
        long = "query-name",
        value_name = "NAME",
        help = "Keep only these query names (repeatable)."
    )]
    query_name: Vec<String>,
    #[arg(
        long = "ref-name",
        value_name = "NAME",
        help = "Keep only these reference names (repeatable)."
    )]
    ref_name: Vec<String>,
    #[arg(
        long = "query-name-exclude",
        value_name = "NAME",
        help = "Drop these query names (repeatable)."
    )]
    query_name_exclude: Vec<String>,
    #[arg(
        long = "ref-name-exclude",
        value_name = "NAME",
        help = "Drop these reference names (repeatable)."
    )]
    ref_name_exclude: Vec<String>,

    #[arg(
        long = "region",
        value_name = "chrN:start-end",
        help = "Keep records overlapping this reference region."
    )]
    region: Option<String>,

    #[arg(long = "min-blocks", value_name = "N")]
    min_blocks: Option<usize>,
    #[arg(
        long = "max-query-gaps",
        value_name = "N",
        help = "Keep records with qNumInsert <= N."
    )]
    max_query_gaps: Option<u32>,
    #[arg(
        long = "max-ref-gaps",
        value_name = "N",
        help = "Keep records with tNumInsert <= N."
    )]
    max_ref_gaps: Option<u32>,

    #[arg(
        long = "drop-self",
        help = "Drop records where query name == reference name."
    )]
    drop_self: bool,

    #[arg(long = "invert", help = "Negate the combined predicate.")]
    invert: bool,

    #[arg(
        long = "mrna",
        help = "Treat alignments as mRNA for the identity calculation."
    )]
    mrna: bool,
}

struct Compiled {
    query_include: HashSet<Vec<u8>>,
    ref_include: HashSet<Vec<u8>>,
    query_exclude: HashSet<Vec<u8>>,
    ref_exclude: HashSet<Vec<u8>>,
    region: Option<(Vec<u8>, Coord, Coord)>,
}

#[derive(Default)]
struct FilterStats {
    read: u64,
    kept: u64,
}

/// Runs the `filter` subcommand.
pub fn run<R, W, E>(
    args: FilterArgs,
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

    let compiled = Compiled {
        query_include: to_set(&args.query_name),
        ref_include: to_set(&args.ref_name),
        query_exclude: to_set(&args.query_name_exclude),
        ref_exclude: to_set(&args.ref_name_exclude),
        region: args.region.as_deref().map(parse_region).transpose()?,
    };
    let needs_record_eval = args.min_score.is_some() || args.min_identity.is_some();

    let mut stats = FilterStats::default();
    write_output(args.out.as_deref(), args.gzip, stdout, |w| {
        if args.inputs.is_empty() {
            let mut reader = StreamingReader::new(stdin);
            process(
                &mut reader,
                w,
                &args,
                &compiled,
                needs_record_eval,
                &mut stats,
            )?;
        } else {
            for input in &args.inputs {
                let mut reader = StreamingReader::from_path(input)?;
                process(
                    &mut reader,
                    w,
                    &args,
                    &compiled,
                    needs_record_eval,
                    &mut stats,
                )?;
            }
        }
        Ok(())
    })?;

    super::log_summary(
        "filter",
        &[
            ("read", stats.read),
            ("kept", stats.kept),
            ("dropped", stats.read - stats.kept),
        ],
    );
    Ok(())
}

fn process<R: BufRead>(
    reader: &mut StreamingReader<R>,
    w: &mut dyn Write,
    args: &FilterArgs,
    compiled: &Compiled,
    needs_record_eval: bool,
    stats: &mut FilterStats,
) -> Result<(), CliError> {
    while let Some(header) = reader.next_header()? {
        stats.read += 1;
        let header_pass = passes_header(args, compiled, &header);
        let need_blocks = header_pass || args.invert;
        if !need_blocks {
            reader.skip_blocks();
            continue;
        }
        let blocks = reader.read_blocks()?;
        let record = header.into_psl(blocks);
        let record_pass = if needs_record_eval {
            passes_record(args, &record)
        } else {
            true
        };
        let pass = header_pass && record_pass;
        let keep = if args.invert { !pass } else { pass };
        if keep {
            emit_record(&mut *w, &record)?;
            stats.kept += 1;
        }
    }
    Ok(())
}

fn passes_header(args: &FilterArgs, compiled: &Compiled, h: &OwnedPslHeader) -> bool {
    if args.min_match.is_some_and(|min| h.matches < min) {
        return false;
    }
    if args.min_query_size.is_some_and(|min| h.query_size < min) {
        return false;
    }
    if args.max_query_size.is_some_and(|max| h.query_size > max) {
        return false;
    }
    if args.min_ref_size.is_some_and(|min| h.reference_size < min) {
        return false;
    }
    if args.max_ref_size.is_some_and(|max| h.reference_size > max) {
        return false;
    }
    if args.strand.is_some_and(|strand| h.strands.query != strand) {
        return false;
    }
    if !compiled.query_include.is_empty() && !compiled.query_include.contains(&h.query_name) {
        return false;
    }
    if !compiled.ref_include.is_empty() && !compiled.ref_include.contains(&h.reference_name) {
        return false;
    }
    if compiled.query_exclude.contains(&h.query_name) {
        return false;
    }
    if compiled.ref_exclude.contains(&h.reference_name) {
        return false;
    }
    let region_ok = compiled.region.as_ref().is_none_or(|(name, start, end)| {
        h.reference_name == *name && h.reference_start < *end && *start < h.reference_end
    });
    if !region_ok {
        return false;
    }
    if args.min_blocks.is_some_and(|min| h.block_count < min) {
        return false;
    }
    if args
        .max_query_gaps
        .is_some_and(|max| h.query_num_insert > max)
    {
        return false;
    }
    if args
        .max_ref_gaps
        .is_some_and(|max| h.reference_num_insert > max)
    {
        return false;
    }
    if args.drop_self && h.query_name == h.reference_name {
        return false;
    }
    true
}

fn passes_record(args: &FilterArgs, record: &OwnedPsl) -> bool {
    if args.min_score.is_some_and(|min| record.score() < min) {
        return false;
    }
    if args
        .min_identity
        .is_some_and(|min| record.percent_id(args.mrna) < min)
    {
        return false;
    }
    true
}

fn to_set(names: &[String]) -> HashSet<Vec<u8>> {
    names.iter().map(|n| n.as_bytes().to_vec()).collect()
}

fn parse_strand(value: &str) -> Result<Strand, String> {
    match value {
        "+" => Ok(Strand::Forward),
        "-" => Ok(Strand::Reverse),
        other => Err(format!("strand must be '+' or '-', got '{other}'")),
    }
}

/// Parses a region string `chrN:start-end` into `(name, start, end)`.
fn parse_region(value: &str) -> Result<(Vec<u8>, Coord, Coord), CliError> {
    let (name, span) = value.rsplit_once(':').ok_or_else(|| {
        CliError::Message(format!(
            "invalid --region '{value}' (expected chrN:start-end)"
        ))
    })?;
    let (start, end) = span.split_once('-').ok_or_else(|| {
        CliError::Message(format!(
            "invalid --region '{value}' (expected chrN:start-end)"
        ))
    })?;
    let start: Coord = start
        .parse()
        .map_err(|_| CliError::Message(format!("invalid region start in '{value}'")))?;
    let end: Coord = end
        .parse()
        .map_err(|_| CliError::Message(format!("invalid region end in '{value}'")))?;
    if start > end {
        return Err(CliError::Message(format!(
            "region start {start} > end {end} in '{value}'"
        )));
    }
    Ok((name.as_bytes().to_vec(), start, end))
}
