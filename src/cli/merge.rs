// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use psltools::{OwnedPsl, StreamingReader, write_psl, write_psl_header};

use super::sort_core::{SortCriterion, compare_records};
use super::{CliError, emit_record, ensure_inputs_exist, write_output};

const COPY_BUFFER_CAPACITY: usize = 1024 * 1024;

/// The key on which inputs are already sorted (enables a streaming k-way merge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortedBy {
    Reference,
    Query,
    Score,
    Size,
}

impl SortedBy {
    fn criterion(self) -> SortCriterion {
        match self {
            SortedBy::Reference => SortCriterion::Reference,
            SortedBy::Query => SortCriterion::Query,
            SortedBy::Score => SortCriterion::Score,
            SortedBy::Size => SortCriterion::Size,
        }
    }
}

/// Arguments for the `merge` subcommand.
#[derive(Debug, Args)]
pub struct MergeArgs {
    #[arg(
        short = 'c',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl file(s). If omitted, read from standard input.",
        value_delimiter = ' ',
        num_args = 1..,
    )]
    inputs: Option<Vec<PathBuf>>,

    #[arg(
        short = 'f',
        long = "file",
        value_name = "PATH",
        conflicts_with = "inputs",
        required_unless_present = "inputs",
        help = "Path to a file listing one input chain path per line"
    )]
    file: Option<PathBuf>,

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
        long = "sorted-by",
        value_enum,
        help = "Inputs are pre-sorted on this key; do an O(1)-memory streaming k-way merge."
    )]
    sorted_by: Option<SortedBy>,

    #[arg(
        long = "dedup",
        help = "Drop records identical to the previously emitted one."
    )]
    dedup: bool,

    #[arg(
        long = "header",
        help = "Emit a psLayout v3 header once before the records."
    )]
    header: bool,
}

/// Runs the `merge` subcommand.
pub fn run<R, W, E>(
    args: MergeArgs,
    stdin: &mut R,
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    // let input_refs: Vec<&std::path::Path> = args.inputs.iter().map(PathBuf::as_path).collect();
    let inputs = collect_input_paths(&args)?;
    let input_refs: Vec<&std::path::Path> = inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;

    let mut written = 0u64;
    write_output(args.out.as_deref(), args.gzip, stdout, |mut w| {
        if args.header {
            write_psl_header(&mut w)?;
        }
        let mut dedup = DedupState::new(args.dedup);
        match args.sorted_by {
            Some(key) if args.inputs.is_some() => {
                written += kway_merge(&inputs, key.criterion(), &mut w, &mut dedup)?;
            }
            _ => {
                if args.inputs.is_none() {
                    let mut reader = StreamingReader::new(stdin);
                    written += concat(&mut reader, &mut w, &mut dedup)?;
                } else {
                    for input in &inputs {
                        let mut reader = StreamingReader::from_path(input)?;
                        written += concat(&mut reader, &mut w, &mut dedup)?;
                    }
                }
            }
        }
        Ok(())
    })?;

    super::log_summary("merge", &[("written", written)]);
    Ok(())
}

/// Collects input chain file paths from arguments.
///
/// Reads paths from --psl directly or from a file listing paths.
///
/// # Arguments
///
/// * `args` - Merge command arguments
///
/// # Output
///
/// Returns `Ok(Vec<PathBuf>)` with input paths or `Err(CliError)` on failure
fn collect_input_paths(args: &MergeArgs) -> Result<Vec<PathBuf>, CliError> {
    if let Some(paths) = &args.inputs {
        return Ok(paths.clone());
    }

    let list_path = args
        .file
        .as_ref()
        .expect("clap enforces either --psl or --file");
    let file = File::open(list_path)?;
    let mut reader = BufReader::with_capacity(COPY_BUFFER_CAPACITY, file);
    let mut line = String::new();
    let mut paths = Vec::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            continue;
        }
        paths.push(PathBuf::from(trimmed.trim()));
    }

    if paths.is_empty() {
        return Err(CliError::Message(format!(
            "{} does not list any input chain files",
            list_path.display()
        )));
    }

    Ok(paths)
}

/// Tracks the last-emitted serialized record for adjacent deduplication.
struct DedupState {
    enabled: bool,
    last: Vec<u8>,
    scratch: Vec<u8>,
}

impl DedupState {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Returns `true` if the record should be emitted (not a duplicate).
    fn accept(&mut self, record: &OwnedPsl) -> Result<bool, CliError> {
        if !self.enabled {
            return Ok(true);
        }
        self.scratch.clear();
        write_psl(&mut self.scratch, record)?;
        if self.scratch == self.last {
            return Ok(false);
        }
        std::mem::swap(&mut self.last, &mut self.scratch);
        Ok(true)
    }
}

fn concat<R: BufRead>(
    reader: &mut StreamingReader<R>,
    w: &mut dyn Write,
    dedup: &mut DedupState,
) -> Result<u64, CliError> {
    let mut written = 0u64;
    while let Some(record) = reader.next_record()? {
        if dedup.accept(&record)? {
            emit_record(&mut *w, &record)?;
            written += 1;
        }
    }
    Ok(written)
}

/// Heap node for the k-way merge.
struct Head {
    record: OwnedPsl,
    reader_index: usize,
    criterion: SortCriterion,
}

impl Ord for Head {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed for a min-heap on the sort key.
        compare_records(&other.record, &self.record, self.criterion)
            .then_with(|| other.reader_index.cmp(&self.reader_index))
    }
}
impl PartialOrd for Head {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Head {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Head {}

fn kway_merge(
    inputs: &[PathBuf],
    criterion: SortCriterion,
    w: &mut dyn Write,
    dedup: &mut DedupState,
) -> Result<u64, CliError> {
    let mut readers: Vec<StreamingReader<Box<dyn BufRead>>> = Vec::with_capacity(inputs.len());
    let mut heap = BinaryHeap::with_capacity(inputs.len());

    for (reader_index, input) in inputs.iter().enumerate() {
        let mut reader = StreamingReader::from_path(input)?;
        if let Some(record) = reader.next_record()? {
            heap.push(Head {
                record,
                reader_index,
                criterion,
            });
        }
        readers.push(reader);
    }

    let mut written = 0u64;
    while let Some(head) = heap.pop() {
        if dedup.accept(&head.record)? {
            emit_record(&mut *w, &head.record)?;
            written += 1;
        }
        if let Some(record) = readers[head.reader_index].next_record()? {
            heap.push(Head {
                record,
                reader_index: head.reader_index,
                criterion,
            });
        }
    }
    Ok(written)
}
