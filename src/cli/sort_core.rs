// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! External merge sort engine for PSL records.
//!
//! Sorts fully in memory when under the `--max-gb` budget (parallel sort with
//! the `parallel` feature), otherwise spills sorted runs to temporary files and
//! performs a bounded k-way heap merge. Ported from `chaintools::cli::sort_core`.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

use psltools::{OwnedPsl, StreamItem, StreamingReader, write_psl};
#[cfg(feature = "parallel")]
use rayon::prelude::ParallelSliceMut;

use super::CliError;

pub(super) const OUTPUT_BUFFER_CAPACITY: usize = 1024 * 1024;
const MAX_OPEN_RUNS: usize = 128;

/// Sort key for PSL records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SortCriterion {
    /// Reference name, then reference start (genomic order; the default).
    Reference,
    /// Query name, then query start.
    Query,
    /// Computed `pslScore`, descending.
    Score,
    /// Reference span (`reference_end - reference_start`), descending.
    Size,
}

/// Sorted input, either fully in memory or spilled to runs needing a merge.
pub(super) enum SortedInput {
    InMemory(Vec<OwnedPsl>),
    Runs(Vec<TempRun>),
}

/// A temporary run file for external sorting; removed on drop.
pub(super) struct TempRun {
    path: PathBuf,
}

impl TempRun {
    pub(super) fn create(
        dir: &Path,
        prefix: &str,
        next_temp_id: &mut u64,
    ) -> Result<(Self, File), CliError> {
        for _ in 0..1024 {
            let path = dir.join(format!(
                ".psltools-sort-{prefix}-{}-{}.tmp",
                std::process::id(),
                *next_temp_id
            ));
            *next_temp_id += 1;
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((Self { path }, file)),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(CliError::Io(err)),
            }
        }
        Err(CliError::Message(format!(
            "failed to create temporary {prefix} file in {}",
            dir.display()
        )))
    }
}

impl Drop for TempRun {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Accumulates records, sorting in memory and spilling sorted runs when the
/// memory budget is exceeded.
pub(super) struct SortAccumulator<'a> {
    sort_by: SortCriterion,
    max_in_memory_bytes: u64,
    temp_dir: &'a Path,
    metadata: Vec<Vec<u8>>,
    records: Vec<OwnedPsl>,
    runs: Vec<TempRun>,
    chunk_bytes: u64,
    next_temp_id: u64,
    records_pushed: u64,
    runs_spilled: u64,
}

impl<'a> SortAccumulator<'a> {
    pub(super) fn new(
        sort_by: SortCriterion,
        max_in_memory_bytes: u64,
        temp_dir: &'a Path,
    ) -> Self {
        Self {
            sort_by,
            max_in_memory_bytes,
            temp_dir,
            metadata: Vec::new(),
            records: Vec::new(),
            runs: Vec::new(),
            chunk_bytes: 0,
            next_temp_id: 0,
            records_pushed: 0,
            runs_spilled: 0,
        }
    }

    pub(super) fn push_stream<R: BufRead>(
        &mut self,
        reader: &mut StreamingReader<R>,
    ) -> Result<(), CliError> {
        while let Some(item) = reader.next_item()? {
            match item {
                StreamItem::MetaLine(line) => self.metadata.push(line),
                StreamItem::Header(header) => {
                    let blocks = reader.read_blocks()?;
                    let record = header.into_psl(blocks);
                    self.chunk_bytes = self.chunk_bytes.saturating_add(estimate_bytes(&record));
                    self.records.push(record);
                    self.records_pushed += 1;
                    if self.chunk_bytes >= self.max_in_memory_bytes && !self.records.is_empty() {
                        self.runs.push(spill_records_to_run(
                            &mut self.records,
                            self.sort_by,
                            self.temp_dir,
                            &mut self.next_temp_id,
                        )?);
                        self.runs_spilled += 1;
                        self.chunk_bytes = 0;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(Vec<Vec<u8>>, SortedInput), CliError> {
        if self.runs.is_empty() {
            sort_records(&mut self.records, self.sort_by);
            return Ok((self.metadata, SortedInput::InMemory(self.records)));
        }
        if !self.records.is_empty() {
            self.runs.push(spill_records_to_run(
                &mut self.records,
                self.sort_by,
                self.temp_dir,
                &mut self.next_temp_id,
            )?);
            self.runs_spilled += 1;
        }
        let reduced = reduce_runs(
            self.runs,
            self.sort_by,
            self.temp_dir,
            &mut self.next_temp_id,
        )?;
        Ok((self.metadata, SortedInput::Runs(reduced)))
    }

    pub(super) fn records_pushed(&self) -> u64 {
        self.records_pushed
    }

    pub(super) fn runs_spilled(&self) -> u64 {
        self.runs_spilled
    }
}

/// Emits sorted records (and any preserved metadata is written separately).
pub(super) fn emit_sorted<W: Write>(
    writer: &mut W,
    sorted: SortedInput,
    sort_by: SortCriterion,
) -> Result<(), CliError> {
    match sorted {
        SortedInput::InMemory(records) => {
            for record in &records {
                write_psl(writer, record)?;
            }
        }
        SortedInput::Runs(runs) => {
            with_merged_runs(&runs, sort_by, |record| {
                write_psl(writer, record).map_err(CliError::from)
            })?;
        }
    }
    Ok(())
}

/// Performs a k-way merge of sorted runs, invoking `emit` for each record.
pub(super) fn with_merged_runs<F>(
    runs: &[TempRun],
    sort_by: SortCriterion,
    mut emit: F,
) -> Result<(), CliError>
where
    F: FnMut(&OwnedPsl) -> Result<(), CliError>,
{
    let mut readers = Vec::with_capacity(runs.len());
    let mut heap = BinaryHeap::with_capacity(runs.len());

    for (run_index, run) in runs.iter().enumerate() {
        let mut reader = StreamingReader::from_path(&run.path)?;
        if let Some(record) = reader.next_record()? {
            heap.push(MergeHead {
                sort_by,
                run_index,
                record,
            });
        }
        readers.push(reader);
    }

    while let Some(head) = heap.pop() {
        emit(&head.record)?;
        if let Some(record) = readers[head.run_index].next_record()? {
            heap.push(MergeHead {
                sort_by,
                run_index: head.run_index,
                record,
            });
        }
    }
    Ok(())
}

/// Writes preserved metadata lines verbatim.
pub(super) fn write_metadata_lines<W: Write>(
    writer: &mut W,
    metadata: &[Vec<u8>],
) -> Result<(), CliError> {
    for line in metadata {
        writer.write_all(line)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

/// Total ordering of two records under `sort_by`.
pub(super) fn compare_records(a: &OwnedPsl, b: &OwnedPsl, sort_by: SortCriterion) -> Ordering {
    let primary = match sort_by {
        SortCriterion::Reference => a
            .reference_name
            .cmp(&b.reference_name)
            .then_with(|| a.reference_start.cmp(&b.reference_start)),
        SortCriterion::Query => a
            .query_name
            .cmp(&b.query_name)
            .then_with(|| a.query_start.cmp(&b.query_start)),
        SortCriterion::Score => b.score().cmp(&a.score()),
        SortCriterion::Size => {
            let sa = a.reference_end.saturating_sub(a.reference_start);
            let sb = b.reference_end.saturating_sub(b.reference_start);
            sb.cmp(&sa)
        }
    };
    primary.then_with(|| tie_break(a, b))
}

/// Deterministic tie-breaker so equal keys sort stably and dedup is well-defined.
fn tie_break(a: &OwnedPsl, b: &OwnedPsl) -> Ordering {
    a.reference_name
        .cmp(&b.reference_name)
        .then_with(|| a.reference_start.cmp(&b.reference_start))
        .then_with(|| a.reference_end.cmp(&b.reference_end))
        .then_with(|| a.query_name.cmp(&b.query_name))
        .then_with(|| a.query_start.cmp(&b.query_start))
        .then_with(|| a.query_end.cmp(&b.query_end))
        .then_with(|| b.score().cmp(&a.score()))
}

fn reduce_runs(
    mut runs: Vec<TempRun>,
    sort_by: SortCriterion,
    temp_dir: &Path,
    next_temp_id: &mut u64,
) -> Result<Vec<TempRun>, CliError> {
    while runs.len() > MAX_OPEN_RUNS {
        let old_runs = std::mem::take(&mut runs);
        let mut next_runs = Vec::new();
        let mut groups = old_runs.into_iter();
        loop {
            let group: Vec<TempRun> = groups.by_ref().take(MAX_OPEN_RUNS).collect();
            if group.is_empty() {
                break;
            }
            let (merged_run, file) = TempRun::create(temp_dir, "merge", next_temp_id)?;
            let mut writer = BufWriter::with_capacity(OUTPUT_BUFFER_CAPACITY, file);
            with_merged_runs(&group, sort_by, |record| {
                write_psl(&mut writer, record).map_err(CliError::from)
            })?;
            writer.flush()?;
            next_runs.push(merged_run);
        }
        runs = next_runs;
    }
    Ok(runs)
}

fn spill_records_to_run(
    records: &mut Vec<OwnedPsl>,
    sort_by: SortCriterion,
    temp_dir: &Path,
    next_temp_id: &mut u64,
) -> Result<TempRun, CliError> {
    let mut chunk = std::mem::take(records);
    sort_records(&mut chunk, sort_by);
    let (run, file) = TempRun::create(temp_dir, "run", next_temp_id)?;
    let mut writer = BufWriter::with_capacity(OUTPUT_BUFFER_CAPACITY, file);
    for record in &chunk {
        write_psl(&mut writer, record)?;
    }
    writer.flush()?;
    Ok(run)
}

fn estimate_bytes(record: &OwnedPsl) -> u64 {
    let blocks = record.block_sizes.len() * 3 * std::mem::size_of::<psltools::Coord>();
    let seq = record.seq.as_ref().map_or(0, |(q, t)| q.len() + t.len());
    (std::mem::size_of::<OwnedPsl>()
        + record.query_name.len()
        + record.reference_name.len()
        + blocks
        + seq) as u64
}

#[cfg(feature = "parallel")]
fn sort_records(records: &mut [OwnedPsl], sort_by: SortCriterion) {
    records.par_sort_unstable_by(|a, b| compare_records(a, b, sort_by));
}

#[cfg(not(feature = "parallel"))]
fn sort_records(records: &mut [OwnedPsl], sort_by: SortCriterion) {
    records.sort_unstable_by(|a, b| compare_records(a, b, sort_by));
}

/// Heap node for the k-way merge (min-heap via reversed comparison).
struct MergeHead {
    sort_by: SortCriterion,
    run_index: usize,
    record: OwnedPsl,
}

impl Ord for MergeHead {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_records(&other.record, &self.record, self.sort_by)
            .then_with(|| other.run_index.cmp(&self.run_index))
    }
}

impl PartialOrd for MergeHead {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MergeHead {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for MergeHead {}
