// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::ops::Range;

use crate::model::Coord;
use crate::model::block::BlockSlice;
use crate::ops::region;
use crate::ops::score::{self, ScoreOpts};
use crate::{Block, ByteSlice};

/// Strand orientation of one sequence in an alignment.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strand {
    /// Forward / plus strand (`+`).
    Forward,
    /// Reverse / minus strand (`-`).
    Reverse,
}

impl Strand {
    /// Returns the single-byte representation (`b'+'` or `b'-'`).
    pub fn as_byte(self) -> u8 {
        match self {
            Strand::Forward => b'+',
            Strand::Reverse => b'-',
        }
    }
}

/// The strand column of a PSL record.
///
/// The query strand is always present. The reference strand is present only for
/// translated/protein alignments, where the strand column is two characters
/// (`qStrand` then `tStrand`, e.g. `+-`). For ordinary DNA alignments the column
/// is a single character describing the query strand and the reference strand is
/// implicitly `+`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strands {
    pub query: Strand,
    pub reference: Option<Strand>,
}

impl Strands {
    /// Returns `true` for translated alignments (two-character strand column).
    pub fn is_translated(&self) -> bool {
        self.reference.is_some()
    }

    /// The effective reference strand (`Forward` when implicit).
    pub fn reference_or_forward(&self) -> Strand {
        self.reference.unwrap_or(Strand::Forward)
    }

    /// Writes the strand column (`"+"`, `"-"`, `"+-"`, ...) to `out`.
    pub fn render<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        out.write_all(&[self.query.as_byte()])?;
        if let Some(reference) = self.reference {
            out.write_all(&[reference.as_byte()])?;
        }
        Ok(())
    }
}

/// Whether a record is plain PSL (21 columns) or PSLx (23 columns).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PslFlavor {
    Psl,
    Pslx,
}

/// The PSLx sequence columns, stored as raw comma-separated lists.
///
/// These are the two trailing columns of a PSLx record (`qSeq`, `tSeq`). They
/// are kept verbatim (one comma-separated entry per block, trailing comma) and
/// split lazily by the consumer when needed.
#[derive(Debug, Clone)]
pub struct PslxSeq {
    pub query_seq: ByteSlice,
    pub reference_seq: ByteSlice,
}

/// A parsed PSL record with zero-copy references into shared storage.
///
/// Names and PSLx sequence columns are [`ByteSlice`] views into the file
/// buffer; the three block coordinate lists live in a shared structure-of-arrays
/// arena referenced by [`BlockSlice`]. The on-disk `blockCount` column is not
/// stored — it is validated against the list lengths at parse time and is always
/// equal to `blocks.len()`.
///
/// PSL's on-disk columns are named `t*` (target) and `q*` (query); this API maps
/// the target columns to `reference_*` to match `chaintools`. So `tName` is
/// [`reference_name`](Psl::reference_name), `tStart` is `reference_start`, etc.
#[derive(Debug, Clone)]
pub struct Psl {
    pub matches: u32,
    pub mismatches: u32,
    pub rep_matches: u32,
    pub n_count: u32,
    pub query_num_insert: u32,
    pub query_base_insert: u32,
    pub reference_num_insert: u32,
    pub reference_base_insert: u32,
    pub query_size: Coord,
    pub query_start: Coord,
    pub query_end: Coord,
    pub reference_size: Coord,
    pub reference_start: Coord,
    pub reference_end: Coord,
    pub strands: Strands,
    pub query_name: ByteSlice,
    pub reference_name: ByteSlice,
    pub blocks: BlockSlice,
    pub seq: Option<PslxSeq>,
}

impl Psl {
    /// The query name as UTF-8, or `""` if the bytes are not valid UTF-8.
    pub fn query_name_str(&self) -> &str {
        self.query_name.as_str().unwrap_or("")
    }

    /// The reference name as UTF-8, or `""` if the bytes are not valid UTF-8.
    pub fn reference_name_str(&self) -> &str {
        self.reference_name.as_str().unwrap_or("")
    }

    /// The query name bytes.
    pub fn query_name_bytes(&self) -> &[u8] {
        self.query_name.as_bytes()
    }

    /// The reference name bytes.
    pub fn reference_name_bytes(&self) -> &[u8] {
        self.reference_name.as_bytes()
    }

    /// The number of alignment blocks (the on-disk `blockCount`).
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// The flavor of this record, derived from the presence of PSLx columns.
    pub fn flavor(&self) -> PslFlavor {
        if self.seq.is_some() {
            PslFlavor::Pslx
        } else {
            PslFlavor::Psl
        }
    }

    /// Whether this record is a protein/translated alignment.
    ///
    /// Derived from the record geometry exactly as UCSC's `pslIsProtein`: a
    /// two-character (translated) strand whose last block's reference span obeys
    /// the amino-acid `×3` rule against `reference_end`/`reference_start`. There
    /// is no external "protein" flag — it is intrinsic to each record.
    pub fn is_protein(&self) -> bool {
        score::is_protein(self)
    }

    /// `3` for protein/translated alignments, otherwise `1`.
    pub fn size_mul(&self) -> u32 {
        score::size_mul(self)
    }

    /// UCSC `pslScore` for this record (exact Kent integer parity).
    pub fn score(&self) -> i64 {
        score::psl_score(self)
    }

    /// UCSC `pslCalcMilliBad` for this record (`i32` to match Kent semantics).
    pub fn milli_bad(&self, is_mrna: bool) -> i32 {
        score::milli_bad(self, ScoreOpts { is_mrna })
    }

    /// Percent identity (`100.0 - milliBad * 0.1`).
    pub fn percent_id(&self, is_mrna: bool) -> f64 {
        score::percent_id(self, ScoreOpts { is_mrna })
    }

    /// The `i`-th block as a [`Block`], or `None` if out of range.
    pub fn block(&self, i: usize) -> Option<Block> {
        self.blocks.get(i)
    }

    /// Forward-strand query interval of block `i` (applies the neg-strand rule).
    pub fn query_block_forward(&self, i: usize) -> Range<Coord> {
        region::query_block_forward(self, i)
    }

    /// Reference interval of block `i` in nucleotides (applies `sizeMul` and the
    /// neg-strand rule).
    pub fn reference_block_interval(&self, i: usize) -> Range<Coord> {
        region::reference_block_interval(self, i)
    }

    /// True if this record overlaps reference region `[start, end)` on `name`.
    pub fn overlaps_reference(&self, name: &[u8], start: Coord, end: Coord) -> bool {
        region::overlaps_reference(self, name, start, end)
    }
}

/// Shared read access to the fields of a PSL record.
///
/// Implemented by both the zero-copy [`Psl`] and the owned
/// [`OwnedPsl`](crate::OwnedPsl), so operations in [`crate::ops`] and the
/// [`writer`](crate::io::writer) work uniformly over either representation.
pub trait PslRecord {
    fn matches(&self) -> u32;
    fn mismatches(&self) -> u32;
    fn rep_matches(&self) -> u32;
    fn n_count(&self) -> u32;
    fn query_num_insert(&self) -> u32;
    fn query_base_insert(&self) -> u32;
    fn reference_num_insert(&self) -> u32;
    fn reference_base_insert(&self) -> u32;
    fn strands(&self) -> Strands;
    fn query_name(&self) -> &[u8];
    fn query_size(&self) -> Coord;
    fn query_start(&self) -> Coord;
    fn query_end(&self) -> Coord;
    fn reference_name(&self) -> &[u8];
    fn reference_size(&self) -> Coord;
    fn reference_start(&self) -> Coord;
    fn reference_end(&self) -> Coord;
    /// Number of blocks (the on-disk `blockCount`).
    fn block_count(&self) -> usize;
    fn block_sizes(&self) -> &[Coord];
    fn query_starts(&self) -> &[Coord];
    fn reference_starts(&self) -> &[Coord];
    /// Raw `qSeq` comma-list for PSLx records, else `None`.
    fn query_seq(&self) -> Option<&[u8]>;
    /// Raw `tSeq` comma-list for PSLx records, else `None`.
    fn reference_seq(&self) -> Option<&[u8]>;
}

impl PslRecord for Psl {
    fn matches(&self) -> u32 {
        self.matches
    }
    fn mismatches(&self) -> u32 {
        self.mismatches
    }
    fn rep_matches(&self) -> u32 {
        self.rep_matches
    }
    fn n_count(&self) -> u32 {
        self.n_count
    }
    fn query_num_insert(&self) -> u32 {
        self.query_num_insert
    }
    fn query_base_insert(&self) -> u32 {
        self.query_base_insert
    }
    fn reference_num_insert(&self) -> u32 {
        self.reference_num_insert
    }
    fn reference_base_insert(&self) -> u32 {
        self.reference_base_insert
    }
    fn strands(&self) -> Strands {
        self.strands
    }
    fn query_name(&self) -> &[u8] {
        self.query_name.as_bytes()
    }
    fn query_size(&self) -> Coord {
        self.query_size
    }
    fn query_start(&self) -> Coord {
        self.query_start
    }
    fn query_end(&self) -> Coord {
        self.query_end
    }
    fn reference_name(&self) -> &[u8] {
        self.reference_name.as_bytes()
    }
    fn reference_size(&self) -> Coord {
        self.reference_size
    }
    fn reference_start(&self) -> Coord {
        self.reference_start
    }
    fn reference_end(&self) -> Coord {
        self.reference_end
    }
    fn block_count(&self) -> usize {
        self.blocks.len()
    }
    fn block_sizes(&self) -> &[Coord] {
        self.blocks.sizes()
    }
    fn query_starts(&self) -> &[Coord] {
        self.blocks.query_starts()
    }
    fn reference_starts(&self) -> &[Coord] {
        self.blocks.reference_starts()
    }
    fn query_seq(&self) -> Option<&[u8]> {
        self.seq.as_ref().map(|s| s.query_seq.as_bytes())
    }
    fn reference_seq(&self) -> Option<&[u8]> {
        self.seq.as_ref().map(|s| s.reference_seq.as_bytes())
    }
}
