// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::ops::Range;
use std::sync::Arc;

use crate::model::Coord;

/// One alignment block of a PSL record.
///
/// A PSL block is described by three parallel coordinate lists in the file
/// (`blockSizes`, `qStarts`, `tStarts`); a `Block` materializes one entry of
/// each. `query_start`/`reference_start` are stored exactly as they appear in
/// the file — i.e. in reverse-complement coordinates when the corresponding
/// strand is `-`. Use [`crate::Psl::query_block_forward`] /
/// [`crate::Psl::reference_block_interval`] for forward-strand intervals.
///
/// # Fields
///
/// * `size` - Block length (in amino acids when the query is a protein)
/// * `query_start` - Start of the block on the query (raw `qStarts[i]`)
/// * `reference_start` - Start of the block on the reference (raw `tStarts[i]`)
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub size: Coord,
    pub query_start: Coord,
    pub reference_start: Coord,
}

/// Structure-of-arrays arena holding the block lists of many records.
///
/// All records parsed by one [`crate::Reader`] share a single `Blocks` arena
/// (behind an `Arc`). Storing the three lists as separate contiguous arrays is
/// cache-friendly for the common scans (sort/stats/score frequently touch only
/// one list) and makes block-list writing a tight per-array loop.
#[derive(Debug, Default)]
pub struct Blocks {
    sizes: Vec<Coord>,
    query_starts: Vec<Coord>,
    reference_starts: Vec<Coord>,
}

impl Blocks {
    /// Creates an empty arena.
    pub fn new() -> Self {
        Blocks::default()
    }

    /// Creates an arena with capacity reserved for `n` block entries.
    pub fn with_capacity(n: usize) -> Self {
        Blocks {
            sizes: Vec::with_capacity(n),
            query_starts: Vec::with_capacity(n),
            reference_starts: Vec::with_capacity(n),
        }
    }

    /// Appends one block entry, returning the index it was stored at.
    pub fn push(&mut self, size: Coord, query_start: Coord, reference_start: Coord) -> usize {
        let idx = self.sizes.len();
        self.sizes.push(size);
        self.query_starts.push(query_start);
        self.reference_starts.push(reference_start);
        idx
    }

    /// Number of block entries in the arena.
    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    /// Returns `true` when the arena holds no blocks.
    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    /// Appends every entry of `other` to this arena.
    ///
    /// Used by the parallel parser to concatenate per-chunk arenas into one
    /// global arena while preserving order.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    pub(crate) fn append(&mut self, other: &mut Blocks) {
        self.sizes.append(&mut other.sizes);
        self.query_starts.append(&mut other.query_starts);
        self.reference_starts.append(&mut other.reference_starts);
    }

    /// Mutable access to the `blockSizes` column (parser fill path).
    pub(crate) fn sizes_mut(&mut self) -> &mut Vec<Coord> {
        &mut self.sizes
    }

    /// Mutable access to the `qStarts` column (parser fill path).
    pub(crate) fn query_starts_mut(&mut self) -> &mut Vec<Coord> {
        &mut self.query_starts
    }

    /// Mutable access to the `tStarts` column (parser fill path).
    pub(crate) fn reference_starts_mut(&mut self) -> &mut Vec<Coord> {
        &mut self.reference_starts
    }
}

/// A record's view into the shared [`Blocks`] arena.
///
/// Holds an `Arc<Blocks>` plus the half-open range of indices belonging to one
/// record. Cloning is cheap (a pointer bump and a range copy).
#[derive(Debug, Clone)]
pub struct BlockSlice {
    storage: Arc<Blocks>,
    range: Range<usize>,
}

impl BlockSlice {
    /// Creates a slice over `range` within `storage`.
    pub fn new(storage: Arc<Blocks>, range: Range<usize>) -> Self {
        BlockSlice { storage, range }
    }

    /// Number of blocks in this record.
    pub fn len(&self) -> usize {
        self.range.len()
    }

    /// Returns `true` if the record has no blocks.
    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    /// The `blockSizes` list for this record.
    pub fn sizes(&self) -> &[Coord] {
        &self.storage.sizes[self.range.clone()]
    }

    /// The `qStarts` list for this record (raw, reverse-complement when `-`).
    pub fn query_starts(&self) -> &[Coord] {
        &self.storage.query_starts[self.range.clone()]
    }

    /// The `tStarts` list for this record (raw, reverse-complement when `-`).
    pub fn reference_starts(&self) -> &[Coord] {
        &self.storage.reference_starts[self.range.clone()]
    }

    /// Returns the `i`-th block, or `None` if out of range.
    pub fn get(&self, i: usize) -> Option<Block> {
        if i >= self.len() {
            return None;
        }
        let idx = self.range.start + i;
        Some(Block {
            size: self.storage.sizes[idx],
            query_start: self.storage.query_starts[idx],
            reference_start: self.storage.reference_starts[idx],
        })
    }

    /// Iterates over the blocks of this record.
    pub fn iter(&self) -> impl Iterator<Item = Block> + '_ {
        (0..self.len()).map(move |i| self.get(i).expect("index within range"))
    }
}
