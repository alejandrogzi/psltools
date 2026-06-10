// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use psltools::{OwnedPsl, StreamingReader, write_psl};

use super::sort_core::{
    OUTPUT_BUFFER_CAPACITY, SortAccumulator, SortCriterion, SortedInput, emit_sorted,
    with_merged_runs, write_metadata_lines,
};
use super::{CliError, ensure_gzip_available, ensure_inputs_exist};

const BYTES_PER_GB: f64 = 1_000_000_000.0;
const DEFAULT_MAX_GB: f64 = 16.0;

/// The primary sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortBy {
    /// Reference name, then reference start (the default; genomic order).
    Reference,
    /// Query name, then query start.
    Query,
    /// Computed `pslScore`, descending.
    Score,
    /// Reference span, descending.
    Size,
}

impl SortBy {
    fn criterion(self) -> SortCriterion {
        match self {
            SortBy::Reference => SortCriterion::Reference,
            SortBy::Query => SortCriterion::Query,
            SortBy::Score => SortCriterion::Score,
            SortBy::Size => SortCriterion::Size,
        }
    }
}

/// Arguments for the `sort` subcommand.
#[derive(Debug, Args)]
pub struct SortArgs {
    #[arg(
        short = 'c',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl (default stdin)."
    )]
    input: Option<PathBuf>,

    #[arg(
        short = 'o',
        long = "out-psl",
        value_name = "PATH",
        help = "Output path (default stdout)."
    )]
    out: Option<PathBuf>,

    #[arg(short = 'G', long = "gzip", help = "Compress output with gzip.")]
    gzip: bool,

    #[arg(short = 'S', long = "sort-by", value_enum, default_value_t = SortBy::Reference, help = "Primary sort key.")]
    sort_by: SortBy,

    #[arg(
        short = 'M',
        long = "max-gb",
        value_name = "GB",
        default_value_t = DEFAULT_MAX_GB,
        help = "Memory budget in GB before spilling sorted runs to temporary files."
    )]
    max_gb: f64,

    #[arg(
        short = 'I',
        long = "out-index",
        value_name = "PATH",
        help = "Write an offset index (hex offset + key) at each primary-key-group boundary."
    )]
    out_index: Option<PathBuf>,
}

/// Runs the `sort` subcommand.
pub fn run<R, W, E>(
    args: SortArgs,
    stdin: &mut R,
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    ensure_gzip_available(args.gzip)?;
    if let Some(path) = &args.input {
        ensure_inputs_exist(&[path.as_path()])?;
    }
    if args.gzip && args.out_index.is_some() {
        return Err(CliError::Message(
            "--out-index cannot be combined with --gzip (index offsets refer to uncompressed bytes)"
                .to_owned(),
        ));
    }

    let max_bytes = max_in_memory_bytes(args.max_gb)?;
    let temp_dir = temp_directory(&args);
    let criterion = args.sort_by.criterion();

    let mut accumulator = SortAccumulator::new(criterion, max_bytes, &temp_dir);
    if let Some(path) = &args.input {
        let mut reader = StreamingReader::from_path(path)?;
        accumulator.push_stream(&mut reader)?;
    } else {
        let mut reader = StreamingReader::new(stdin);
        accumulator.push_stream(&mut reader)?;
    }
    let records = accumulator.records_pushed();
    let runs_spilled = accumulator.runs_spilled();
    let (metadata, sorted) = accumulator.finish()?;

    emit_output(&args, criterion, &metadata, sorted, stdout)?;

    super::log_summary(
        "sort",
        &[
            ("records", records),
            ("metadata", metadata_count(&metadata)),
            ("runs_spilled", runs_spilled),
        ],
    );
    Ok(())
}

fn metadata_count(metadata: &[Vec<u8>]) -> u64 {
    metadata.len() as u64
}

fn emit_output<W: Write>(
    args: &SortArgs,
    criterion: SortCriterion,
    metadata: &[Vec<u8>],
    sorted: SortedInput,
    stdout: &mut W,
) -> Result<(), CliError> {
    // Index path: count bytes, record group boundaries (no gzip in this mode).
    if let Some(index_path) = &args.out_index {
        let mut index = BufWriter::with_capacity(OUTPUT_BUFFER_CAPACITY, File::create(index_path)?);
        if let Some(out) = &args.out {
            let mut counted = CountingWriter::new(BufWriter::with_capacity(
                OUTPUT_BUFFER_CAPACITY,
                File::create(out)?,
            ));
            write_metadata_lines(&mut counted, metadata)?;
            emit_with_index(&mut counted, &mut index, sorted, criterion)?;
            counted.into_inner().flush()?;
        } else {
            let mut counted = CountingWriter::new(&mut *stdout);
            write_metadata_lines(&mut counted, metadata)?;
            emit_with_index(&mut counted, &mut index, sorted, criterion)?;
        }
        index.flush()?;
        return Ok(());
    }

    super::write_output(args.out.as_deref(), args.gzip, stdout, |mut w| {
        write_metadata_lines(&mut w, metadata)?;
        emit_sorted(&mut w, sorted, criterion)
    })
}

fn emit_with_index<W: Write, I: Write>(
    writer: &mut CountingWriter<W>,
    index: &mut I,
    sorted: SortedInput,
    criterion: SortCriterion,
) -> Result<(), CliError> {
    let mut tracker = GroupTracker::new(criterion);
    match sorted {
        SortedInput::InMemory(records) => {
            for record in &records {
                tracker.before(index, writer.position(), record)?;
                write_psl(writer, record)?;
            }
        }
        SortedInput::Runs(runs) => {
            with_merged_runs(&runs, criterion, |record| {
                tracker.before(index, writer.position(), record)?;
                write_psl(writer, record).map_err(CliError::from)
            })?;
        }
    }
    Ok(())
}

/// Tracks primary-key group changes and writes `hexoffset<TAB>key` to the index.
struct GroupTracker {
    criterion: SortCriterion,
    last: Option<Vec<u8>>,
}

impl GroupTracker {
    fn new(criterion: SortCriterion) -> Self {
        Self {
            criterion,
            last: None,
        }
    }

    fn before<I: Write>(
        &mut self,
        index: &mut I,
        offset: u64,
        record: &OwnedPsl,
    ) -> Result<(), CliError> {
        let key: Vec<u8> = match self.criterion {
            SortCriterion::Reference => record.reference_name.clone(),
            SortCriterion::Query => record.query_name.clone(),
            SortCriterion::Score => record.score().to_string().into_bytes(),
            SortCriterion::Size => record
                .reference_end
                .saturating_sub(record.reference_start)
                .to_string()
                .into_bytes(),
        };
        if self.last.as_deref() != Some(key.as_slice()) {
            write!(index, "{offset:x}\t")?;
            index.write_all(&key)?;
            index.write_all(b"\n")?;
            self.last = Some(key);
        }
        Ok(())
    }
}

/// A writer that tracks the number of bytes written (for index offsets).
struct CountingWriter<W> {
    inner: W,
    position: u64,
}

impl<W: Write> CountingWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner, position: 0 }
    }
    fn position(&self) -> u64 {
        self.position
    }
    fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.position += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn max_in_memory_bytes(max_gb: f64) -> Result<u64, CliError> {
    if !max_gb.is_finite() || max_gb <= 0.0 {
        return Err(CliError::Message(
            "--max-gb must be a finite number greater than zero".to_owned(),
        ));
    }
    let bytes = (max_gb * BYTES_PER_GB).ceil();
    if bytes > u64::MAX as f64 {
        return Err(CliError::Message("--max-gb is too large".to_owned()));
    }
    Ok(bytes as u64)
}

fn temp_directory(args: &SortArgs) -> PathBuf {
    if let Some(path) = args.out.as_ref().or(args.input.as_ref()) {
        return path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
    }
    std::env::temp_dir()
}
