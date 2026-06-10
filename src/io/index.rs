// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Random-access indexes (feature `index`).
//!
//! [`PslIndex`] records the byte span of every record line for re-reading a
//! specific record without re-parsing the file. [`IntervalIndex`] groups records
//! by reference sequence and answers `[start, end)` overlap queries in
//! `O(log n + k)` using a sorted-start array bounded by the longest interval.

use std::collections::HashMap;
use std::path::Path;

use crate::PslError;
use crate::io::storage::{SharedBytes, is_gz_path};
use crate::model::Coord;
use crate::model::psl::PslRecord;
use crate::parser::locate_line_ranges;

#[cfg(not(feature = "gzip"))]
use crate::io::storage::gzip_feature_error;
#[cfg(feature = "gzip")]
use flate2::read::MultiGzDecoder;
#[cfg(feature = "mmap")]
use memmap2::MmapOptions;

/// Byte span of a single record within the source buffer.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy)]
pub struct PslSpan {
    pub offset: usize,
    pub len: usize,
}

/// Record-offset index: the byte span of every record line.
pub struct PslIndex {
    bytes: SharedBytes,
    spans: Vec<PslSpan>,
}

impl PslIndex {
    /// Builds an index from a path (mmap when available, decompressing `.gz`).
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, PslError> {
        let path = path.as_ref();
        if is_gz_path(path) {
            #[cfg(feature = "gzip")]
            {
                use std::io::Read;
                let file = std::fs::File::open(path)?;
                let mut decoder = MultiGzDecoder::new(file);
                let mut buffer = Vec::new();
                decoder.read_to_end(&mut buffer)?;
                return Self::from_owned(buffer);
            }
            #[cfg(not(feature = "gzip"))]
            {
                return Err(gzip_feature_error());
            }
        }

        #[cfg(feature = "mmap")]
        {
            let file = std::fs::File::open(path)?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            Self::from_bytes(SharedBytes::from_mmap(mmap))
        }
        #[cfg(not(feature = "mmap"))]
        {
            use std::io::Read;
            let mut buffer = Vec::new();
            std::fs::File::open(path)?.read_to_end(&mut buffer)?;
            Self::from_owned(buffer)
        }
    }

    /// Builds an index from owned bytes.
    pub fn from_owned(bytes: Vec<u8>) -> Result<Self, PslError> {
        Self::from_bytes(SharedBytes::from_owned(bytes))
    }

    /// Builds an index from shared bytes (mmap or owned).
    pub fn from_bytes(bytes: SharedBytes) -> Result<Self, PslError> {
        let spans = locate_line_ranges(bytes.as_slice())
            .into_iter()
            .map(|range| PslSpan {
                offset: range.start,
                len: range.end - range.start,
            })
            .collect();
        Ok(PslIndex { bytes, spans })
    }

    /// All record spans.
    pub fn spans(&self) -> &[PslSpan] {
        &self.spans
    }

    /// Number of indexed records.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// `true` when there are no records.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The raw bytes of record `idx`, or `None` if out of range.
    pub fn record_bytes(&self, idx: usize) -> Option<&[u8]> {
        let span = self.spans.get(idx)?;
        self.bytes
            .as_slice()
            .get(span.offset..span.offset + span.len)
    }
}

/// Per-reference sorted intervals for fast overlap queries.
#[derive(Debug, Default)]
struct Group {
    starts: Vec<Coord>,
    ends: Vec<Coord>,
    ids: Vec<usize>,
    max_len: Coord,
}

/// Interval index over the reference spans of a set of records.
///
/// Built once from a record slice; answers `[start, end)` overlap queries per
/// reference name, returning the indices of overlapping records.
#[derive(Debug, Default)]
pub struct IntervalIndex {
    groups: HashMap<Vec<u8>, Group>,
    len: usize,
}

impl IntervalIndex {
    /// Builds an interval index from `(name, start, end)` triples, where the
    /// `i`-th triple is associated with record id `i`.
    pub fn from_intervals<I>(intervals: I) -> Self
    where
        I: IntoIterator<Item = (Vec<u8>, Coord, Coord)>,
    {
        let mut groups: HashMap<Vec<u8>, Vec<(Coord, Coord, usize)>> = HashMap::new();
        let mut len = 0usize;
        for (id, (name, start, end)) in intervals.into_iter().enumerate() {
            groups.entry(name).or_default().push((start, end, id));
            len += 1;
        }

        let groups = groups
            .into_iter()
            .map(|(name, mut entries)| {
                entries.sort_unstable_by_key(|&(start, _, _)| start);
                let mut group = Group::default();
                for (start, end, id) in entries {
                    group.starts.push(start);
                    group.ends.push(end);
                    group.ids.push(id);
                    let span = end.saturating_sub(start);
                    if span > group.max_len {
                        group.max_len = span;
                    }
                }
                (name, group)
            })
            .collect();

        IntervalIndex { groups, len }
    }

    /// Builds an interval index over the reference spans of `records`.
    pub fn from_records<P: PslRecord>(records: &[P]) -> Self {
        Self::from_intervals(records.iter().map(|p| {
            (
                p.reference_name().to_vec(),
                p.reference_start(),
                p.reference_end(),
            )
        }))
    }

    /// Returns the record ids whose reference span overlaps `[start, end)` on
    /// `name`, in ascending start order.
    pub fn query(&self, name: &[u8], start: Coord, end: Coord) -> Vec<usize> {
        let Some(group) = self.groups.get(name) else {
            return Vec::new();
        };
        // Upper bound: intervals with start >= end cannot overlap.
        let upper = group.starts.partition_point(|&s| s < end);
        // Lower bound: intervals with start + max_len <= query start end before it.
        let lower = group
            .starts
            .partition_point(|&s| s.saturating_add(group.max_len) <= start);

        let mut hits = Vec::new();
        for idx in lower..upper {
            if group.ends[idx] > start {
                hits.push(group.ids[idx]);
            }
        }
        hits
    }

    /// Total number of indexed intervals.
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` when the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
