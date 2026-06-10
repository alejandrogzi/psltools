// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Coordinate helpers and interval overlap predicates.
//!
//! These encode the negative-strand / translated coordinate rules from the PSL
//! spec once, so the rest of the crate never inlines them ad hoc. Arithmetic is
//! saturating so malformed records can never panic here.

use std::ops::Range;

use crate::model::Coord;
use crate::model::psl::{PslRecord, Strand};
use crate::ops::score::size_mul;

/// Forward-strand query interval of block `i`.
///
/// `qStarts[i]` is given in reverse-complement coordinates when the query strand
/// is `-`; this returns the interval on the forward strand either way:
///
/// * `+`: `[qStarts[i], qStarts[i] + blockSizes[i])`
/// * `-`: `[qSize - (qStarts[i] + blockSizes[i]), qSize - qStarts[i])`
pub fn query_block_forward<P: PslRecord>(p: &P, i: usize) -> Range<Coord> {
    let start = p.query_starts()[i];
    let size = p.block_sizes()[i];
    match p.strands().query {
        Strand::Forward => start..start.saturating_add(size),
        Strand::Reverse => {
            let q = p.query_size();
            q.saturating_sub(start.saturating_add(size))..q.saturating_sub(start)
        }
    }
}

/// Reference interval of block `i` in nucleotides.
///
/// Applies `sizeMul` (3 for protein queries, else 1) to the block size, and the
/// neg-strand rule when the reference strand is `-`:
///
/// * `+`: `[tStarts[i], tStarts[i] + blockSizes[i] * sizeMul)`
/// * `-`: `[tSize - (tStarts[i] + blockSizes[i] * sizeMul), tSize - tStarts[i])`
pub fn reference_block_interval<P: PslRecord>(p: &P, i: usize) -> Range<Coord> {
    let start = p.reference_starts()[i];
    let size = p.block_sizes()[i].saturating_mul(size_mul(p) as Coord);
    match p.strands().reference_or_forward() {
        Strand::Forward => start..start.saturating_add(size),
        Strand::Reverse => {
            let r = p.reference_size();
            r.saturating_sub(start.saturating_add(size))..r.saturating_sub(start)
        }
    }
}

/// Returns `true` if `[a_start, a_end)` and `[b_start, b_end)` overlap.
pub fn intervals_overlap(a_start: Coord, a_end: Coord, b_start: Coord, b_end: Coord) -> bool {
    a_start < b_end && b_start < a_end
}

/// True if the record's reference span overlaps `[start, end)` on `name`.
pub fn overlaps_reference<P: PslRecord>(p: &P, name: &[u8], start: Coord, end: Coord) -> bool {
    p.reference_name() == name
        && intervals_overlap(p.reference_start(), p.reference_end(), start, end)
}

/// True if the record's query span overlaps `[start, end)` on `name`.
pub fn overlaps_query<P: PslRecord>(p: &P, name: &[u8], start: Coord, end: Coord) -> bool {
    p.query_name() == name && intervals_overlap(p.query_start(), p.query_end(), start, end)
}
