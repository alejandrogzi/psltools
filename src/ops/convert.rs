// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Conversions out of the PSL model.
//!
//! A PSL record is mapped to a [`genepred::GenePred`] (its blocks become the
//! exons, with `sizeMul` and the negative-strand coordinate rules applied so the
//! exons are always forward-strand and ascending). From there `genepred`'s
//! [`GenePred::to_bed`] derives any BED layout (`Bed3`/`4`/`5`/`6`/`8`/`9`/`12`),
//! so callers can emit whichever width they want.

use genepred::genepred::Extras;
use genepred::{Bed12, BedFormat, GenePred, Strand as GpStrand};

use crate::model::psl::{PslRecord, Strand};
use crate::ops::region::reference_block_interval;

/// Builds a [`GenePred`] over the reference sequence from a PSL record.
///
/// `chrom`/`start`/`end` come from the reference; `name` from the query; `strand`
/// from the query strand; exons come from the per-block reference intervals
/// (forward-strand, `sizeMul`-aware, sorted ascending).
// `Coord as u64` is a real widening for the default `u32` build (genepred uses
// `u64`); it is only a no-op under `bigcoords`.
#[allow(clippy::unnecessary_cast)]
pub fn to_genepred<P: PslRecord>(p: &P) -> GenePred {
    let chrom = p.reference_name().to_vec();
    let start = p.reference_start() as u64;
    let end = p.reference_end() as u64;

    let mut gene = GenePred::from_coords(chrom, start, end, Extras::new());
    gene.set_name(Some(p.query_name().to_vec()));
    gene.set_strand(Some(match p.strands().query {
        Strand::Forward => GpStrand::Forward,
        Strand::Reverse => GpStrand::Reverse,
    }));

    let mut intervals: Vec<(u64, u64)> = (0..p.block_count())
        .map(|i| {
            let r = reference_block_interval(p, i);
            (r.start as u64, r.end as u64)
        })
        .collect();
    intervals.sort_unstable_by_key(|&(s, _)| s);

    gene.set_block_count(Some(intervals.len() as u32));
    gene.set_block_starts(Some(intervals.iter().map(|&(s, _)| s).collect()));
    gene.set_block_ends(Some(intervals.iter().map(|&(_, e)| e).collect()));
    gene
}

/// Renders a PSL record as a BED line of layout `K` (no trailing newline).
///
/// `K` is one of `genepred`'s BED markers (`Bed3`, `Bed4`, `Bed5`, `Bed6`,
/// `Bed8`, `Bed9`, `Bed12`).
pub fn to_bed<P: PslRecord, K: BedFormat>(p: &P) -> Vec<u8> {
    to_genepred(p).to_bed::<K>()
}

/// Convenience: render a PSL record as a BED12 line (no trailing newline).
pub fn to_bed12<P: PslRecord>(p: &P) -> Vec<u8> {
    to_bed::<P, Bed12>(p)
}
