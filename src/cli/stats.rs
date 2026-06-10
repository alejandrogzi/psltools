// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use psltools::{OwnedPsl, StreamingReader};

use super::{CliError, ensure_inputs_exist};

/// Arguments for the `stats` subcommand.
#[derive(Debug, Args)]
pub struct StatsArgs {
    #[arg(
        short = 'c',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl file(s). If omitted, read from standard input.",
        value_delimiter = ' ',
        num_args = 1..,
    )]
    inputs: Vec<PathBuf>,

    #[arg(long = "mrna", help = "Treat alignments as mRNA for identity bins.")]
    mrna: bool,

    #[arg(long = "json", help = "Emit JSON instead of a human-readable table.")]
    json: bool,
}

#[derive(Default)]
struct Stats {
    count: u64,
    scores: Vec<i64>,
    score_total: i128,
    per_reference: BTreeMap<Vec<u8>, (u64, u64)>, // name -> (records, covered bases)
    block_counts: BTreeMap<usize, u64>,
    identity_bins: [u64; 11], // 0..=10 deciles; bin 10 == exactly 100%
}

/// Runs the `stats` subcommand.
pub fn run<R, W, E>(
    args: StatsArgs,
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

    let mut stats = Stats::default();
    if args.inputs.is_empty() {
        let mut reader = StreamingReader::new(stdin);
        accumulate(&mut reader, &mut stats, args.mrna)?;
    } else {
        for input in &args.inputs {
            let mut reader = StreamingReader::from_path(input)?;
            accumulate(&mut reader, &mut stats, args.mrna)?;
        }
    }

    if args.json {
        write_json(stdout, &stats)?;
    } else {
        write_table(stdout, &stats)?;
    }
    super::log_summary("stats", &[("records", stats.count)]);
    Ok(())
}

fn accumulate<R: BufRead>(
    reader: &mut StreamingReader<R>,
    stats: &mut Stats,
    mrna: bool,
) -> Result<(), CliError> {
    while let Some(record) = reader.next_record()? {
        stats.count += 1;
        let score = record.score();
        stats.scores.push(score);
        stats.score_total += score as i128;

        let covered: u64 = covered_bases(&record);
        let entry = stats
            .per_reference
            .entry(record.reference_name.clone())
            .or_insert((0, 0));
        entry.0 += 1;
        entry.1 += covered;

        *stats.block_counts.entry(record.block_count()).or_insert(0) += 1;

        let pct = record.percent_id(mrna).clamp(0.0, 100.0);
        let bin = (pct / 10.0).floor() as usize;
        stats.identity_bins[bin.min(10)] += 1;
    }
    Ok(())
}

// `Coord as u64` is a real widening for the default `u32` build.
#[allow(clippy::unnecessary_cast)]
fn covered_bases(record: &OwnedPsl) -> u64 {
    let mul = record.size_mul() as u64;
    record.block_sizes.iter().map(|&s| s as u64 * mul).sum()
}

fn median(scores: &mut [i64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    scores.sort_unstable();
    let mid = scores.len() / 2;
    if scores.len() % 2 == 1 {
        scores[mid] as f64
    } else {
        (scores[mid - 1] as f64 + scores[mid] as f64) / 2.0
    }
}

fn write_table<W: Write>(w: &mut W, stats: &Stats) -> Result<(), CliError> {
    let mut scores = stats.scores.clone();
    let mean = if stats.count > 0 {
        stats.score_total as f64 / stats.count as f64
    } else {
        0.0
    };
    writeln!(w, "records\t{}", stats.count)?;
    writeln!(w, "score_total\t{}", stats.score_total)?;
    writeln!(w, "score_mean\t{mean:.2}")?;
    writeln!(w, "score_median\t{:.1}", median(&mut scores))?;

    writeln!(w, "# identity histogram (deciles of percent identity)")?;
    for (i, count) in stats.identity_bins.iter().enumerate() {
        let lo = i * 10;
        let label = if i == 10 {
            "100".to_string()
        } else {
            format!("{lo}-{}", lo + 10)
        };
        writeln!(w, "identity[{label}]\t{count}")?;
    }

    writeln!(w, "# block-count distribution")?;
    for (blocks, count) in &stats.block_counts {
        writeln!(w, "blocks[{blocks}]\t{count}")?;
    }

    writeln!(w, "# per-reference (records, covered bases)")?;
    for (name, (records, covered)) in &stats.per_reference {
        let name = String::from_utf8_lossy(name);
        writeln!(w, "ref[{name}]\t{records}\t{covered}")?;
    }
    Ok(())
}

fn write_json<W: Write>(w: &mut W, stats: &Stats) -> Result<(), CliError> {
    let mut scores = stats.scores.clone();
    let mean = if stats.count > 0 {
        stats.score_total as f64 / stats.count as f64
    } else {
        0.0
    };
    write!(w, "{{")?;
    write!(w, "\"records\":{},", stats.count)?;
    write!(w, "\"score_total\":{},", stats.score_total)?;
    write!(w, "\"score_mean\":{mean:.4},", mean = mean)?;
    write!(w, "\"score_median\":{:.4},", median(&mut scores))?;

    write!(w, "\"identity_bins\":[")?;
    for (i, count) in stats.identity_bins.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{count}")?;
    }
    write!(w, "],")?;

    write!(w, "\"block_counts\":{{")?;
    for (i, (blocks, count)) in stats.block_counts.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "\"{blocks}\":{count}")?;
    }
    write!(w, "}},")?;

    write!(w, "\"per_reference\":{{")?;
    for (i, (name, (records, covered))) in stats.per_reference.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        let name = String::from_utf8_lossy(name);
        let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
        write!(
            w,
            "\"{escaped}\":{{\"records\":{records},\"covered\":{covered}}}"
        )?;
    }
    write!(w, "}}}}")?;
    writeln!(w)?;
    Ok(())
}
