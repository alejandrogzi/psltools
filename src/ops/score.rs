// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Scoring and identity, pinned bit-for-bit to `kent/src/lib/psl.c`.
//!
//! Every formula here reproduces the UCSC integer semantics exactly (C `int` ==
//! `i32`), including the `repMatch >> 1` half-weighting, `sizeMul` applying only
//! to match/repMatch/misMatch, and the `round(3*log(1+sizeDif))` term that goes
//! through `f64`. The only goal beyond parity is speed.

use crate::model::psl::{PslRecord, Strand};

/// External knobs for identity calculations.
///
/// `protein`/`sizeMul` is intrinsic to a record (derived from its geometry), so
/// it is deliberately *not* here. The only caller-supplied knob is `is_mrna`,
/// which affects [`milli_bad`] / [`percent_id`] only.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScoreOpts {
    pub is_mrna: bool,
}

/// Implements UCSC `pslIsProtein`.
///
/// A record is protein/translated when its strand column is two characters and
/// the last block's reference span satisfies the amino-acid `×3` rule against
/// `reference_end` (`tStrand == +`) or `reference_start` (`tStrand == -`).
pub fn is_protein<P: PslRecord>(p: &P) -> bool {
    let n = p.block_count();
    if n == 0 {
        return false;
    }
    let Some(reference_strand) = p.strands().reference else {
        // A single-char DNA strand has no strand[1] => never protein.
        return false;
    };
    let last = n - 1;
    let last_start = p.reference_starts()[last] as i64;
    let last_size = p.block_sizes()[last] as i64;
    let term = last_start + 3 * last_size;
    match reference_strand {
        Strand::Forward => p.reference_end() as i64 == term,
        Strand::Reverse => p.reference_start() as i64 == p.reference_size() as i64 - term,
    }
}

/// Returns `3` for protein/translated records, otherwise `1`.
pub fn size_mul<P: PslRecord>(p: &P) -> u32 {
    if is_protein(p) { 3 } else { 1 }
}

/// Implements UCSC `pslScore` in `i32` arithmetic, widened to `i64` on return.
pub fn psl_score<P: PslRecord>(p: &P) -> i64 {
    let size_mul = if is_protein(p) { 3i32 } else { 1i32 };
    let matches = p.matches() as i32;
    let rep_matches = p.rep_matches() as i32;
    let mismatches = p.mismatches() as i32;
    let query_num_insert = p.query_num_insert() as i32;
    let reference_num_insert = p.reference_num_insert() as i32;

    let score = size_mul
        .wrapping_mul(matches.wrapping_add(rep_matches >> 1))
        .wrapping_sub(size_mul.wrapping_mul(mismatches))
        .wrapping_sub(query_num_insert)
        .wrapping_sub(reference_num_insert);
    score as i64
}

/// Implements UCSC `pslCalcMilliBad` in `i32` arithmetic.
pub fn milli_bad<P: PslRecord>(p: &P, opts: ScoreOpts) -> i32 {
    let is_mrna = opts.is_mrna;
    let size_mul = if is_protein(p) { 3i32 } else { 1i32 };
    let query_ali_size =
        size_mul.wrapping_mul((p.query_end() as i32).wrapping_sub(p.query_start() as i32));
    let reference_ali_size = (p.reference_end() as i32).wrapping_sub(p.reference_start() as i32);
    let ali_size = query_ali_size.min(reference_ali_size);
    if ali_size <= 0 {
        return 0;
    }
    let mut size_dif = query_ali_size - reference_ali_size;
    if size_dif < 0 {
        size_dif = if is_mrna { 0 } else { -size_dif };
    }
    let mut insert_factor = p.query_num_insert() as i32;
    if !is_mrna {
        insert_factor = insert_factor.wrapping_add(p.reference_num_insert() as i32);
    }
    let total = size_mul.wrapping_mul(
        (p.matches() as i32)
            .wrapping_add(p.rep_matches() as i32)
            .wrapping_add(p.mismatches() as i32),
    );
    if total == 0 {
        return 0;
    }
    let log_term = (3.0 * (1.0 + size_dif as f64).ln()).round() as i32;
    let numerator = 1000i32.wrapping_mul(
        (p.mismatches() as i32)
            .wrapping_mul(size_mul)
            .wrapping_add(insert_factor)
            .wrapping_add(log_term),
    );
    numerator / total
}

/// Percent identity: `100.0 - milliBad * 0.1`.
pub fn percent_id<P: PslRecord>(p: &P, opts: ScoreOpts) -> f64 {
    100.0 - milli_bad(p, opts) as f64 * 0.1
}
