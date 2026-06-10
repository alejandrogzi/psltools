// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Query/reference swap, transcribed from UCSC `pslSwap`.
//!
//! Produces a valid PSL with query and target exchanged, handling the four
//! cases `pslSwap` distinguishes: translated, untranslated-keep-strand
//! (`no_rc`), and untranslated `+`/`-` (the last reverse-complements the blocks
//! so the new target stays on the `+` strand). Swapping is an involution for
//! the canonical DNA/translated strand forms.

use crate::OwnedPsl;
use crate::model::Coord;
use crate::model::psl::{PslRecord, Strand, Strands};

/// Swaps query and reference, reverse-complementing untranslated minus-strand
/// records so the new target stays on `+` (UCSC `pslSwap` default).
pub fn swap<P: PslRecord>(p: &P) -> OwnedPsl {
    swap_with(p, false)
}

/// Swaps query and reference.
///
/// When `no_rc` is `true`, an untranslated record is not reverse-complemented;
/// instead the target strand is made explicit (matching UCSC `pslSwap`'s
/// `noRc` argument).
pub fn swap_with<P: PslRecord>(p: &P, no_rc: bool) -> OwnedPsl {
    // Unconditional scalar swaps (qName<->tName, sizes, starts, ends, inserts).
    let query_name = p.reference_name().to_vec();
    let reference_name = p.query_name().to_vec();
    let query_size = p.reference_size();
    let reference_size = p.query_size();
    let query_start = p.reference_start();
    let reference_start = p.query_start();
    let query_end = p.reference_end();
    let reference_end = p.query_end();
    let query_num_insert = p.reference_num_insert();
    let reference_num_insert = p.query_num_insert();
    let query_base_insert = p.reference_base_insert();
    let reference_base_insert = p.query_base_insert();

    let sizes = p.block_sizes();
    let q_starts = p.query_starts();
    let t_starts = p.reference_starts();
    let query_seq = p.query_seq();
    let reference_seq = p.reference_seq();

    let (strands, block_sizes, new_query_starts, new_reference_starts, seq);

    if p.strands().reference.is_some() {
        // Translated: swap strand chars and swap the start lists in place.
        strands = Strands {
            query: p.strands().reference_or_forward(),
            reference: Some(p.strands().query),
        };
        block_sizes = sizes.to_vec();
        new_query_starts = t_starts.to_vec();
        new_reference_starts = q_starts.to_vec();
        seq = swap_seq(query_seq, reference_seq);
    } else if no_rc {
        // Untranslated, keep orientation: make the target strand explicit.
        strands = Strands {
            query: Strand::Forward,
            reference: Some(p.strands().query),
        };
        block_sizes = sizes.to_vec();
        new_query_starts = t_starts.to_vec();
        new_reference_starts = q_starts.to_vec();
        seq = swap_seq(query_seq, reference_seq);
    } else if p.strands().query == Strand::Forward {
        // Untranslated +: strand unchanged, swap the start lists.
        strands = p.strands();
        block_sizes = sizes.to_vec();
        new_query_starts = t_starts.to_vec();
        new_reference_starts = q_starts.to_vec();
        seq = swap_seq(query_seq, reference_seq);
    } else {
        // Untranslated -: reverse-complement the blocks (strand unchanged).
        strands = p.strands();
        let n = sizes.len();
        let mut rev_sizes = sizes.to_vec();
        rev_sizes.reverse();
        // After reversing all three lists and swapping the start arrays, the new
        // query starts come from the reversed reference list and vice versa.
        let mut new_q: Vec<Coord> = t_starts.iter().rev().copied().collect();
        let mut new_t: Vec<Coord> = q_starts.iter().rev().copied().collect();
        for i in 0..n {
            new_q[i] = query_size.saturating_sub(new_q[i].saturating_add(rev_sizes[i]));
            new_t[i] = reference_size.saturating_sub(new_t[i].saturating_add(rev_sizes[i]));
        }
        block_sizes = rev_sizes;
        new_query_starts = new_q;
        new_reference_starts = new_t;
        seq = swap_rc_seq(query_seq, reference_seq);
    }

    OwnedPsl {
        matches: p.matches(),
        mismatches: p.mismatches(),
        rep_matches: p.rep_matches(),
        n_count: p.n_count(),
        query_num_insert,
        query_base_insert,
        reference_num_insert,
        reference_base_insert,
        strands,
        query_name,
        query_size,
        query_start,
        query_end,
        reference_name,
        reference_size,
        reference_start,
        reference_end,
        block_sizes: block_sizes.into(),
        query_starts: new_query_starts.into(),
        reference_starts: new_reference_starts.into(),
        seq,
    }
}

/// Swaps the two PSLx sequence lists wholesale (the non-RC block swap is an
/// element-wise swap, which equals swapping the lists for equal-length lists).
fn swap_seq(query_seq: Option<&[u8]>, reference_seq: Option<&[u8]>) -> Option<(Vec<u8>, Vec<u8>)> {
    match (query_seq, reference_seq) {
        (Some(q), Some(t)) => Some((t.to_vec(), q.to_vec())),
        _ => None,
    }
}

/// Reverse-complements both PSLx sequence lists (reversing block order and each
/// sequence) then swaps them, matching `swapRCBlocks` + `rcSeqs`.
fn swap_rc_seq(
    query_seq: Option<&[u8]>,
    reference_seq: Option<&[u8]>,
) -> Option<(Vec<u8>, Vec<u8>)> {
    match (query_seq, reference_seq) {
        (Some(q), Some(t)) => Some((rc_seq_list(t), rc_seq_list(q))),
        _ => None,
    }
}

/// Reverses the entry order of a comma-separated sequence list and
/// reverse-complements each entry.
fn rc_seq_list(raw: &[u8]) -> Vec<u8> {
    let body = raw.strip_suffix(b",").unwrap_or(raw);
    let mut out = Vec::with_capacity(raw.len());
    if body.is_empty() {
        return out;
    }
    let entries: Vec<&[u8]> = body.split(|&b| b == b',').collect();
    for entry in entries.iter().rev() {
        for &base in entry.iter().rev() {
            out.push(complement(base));
        }
        out.push(b',');
    }
    out
}

/// IUPAC-aware nucleotide complement, preserving case; non-nucleotide bytes are
/// returned unchanged.
fn complement(b: u8) -> u8 {
    match b {
        b'A' => b'T',
        b'C' => b'G',
        b'G' => b'C',
        b'T' => b'A',
        b'U' => b'A',
        b'a' => b't',
        b'c' => b'g',
        b'g' => b'c',
        b't' => b'a',
        b'u' => b'a',
        b'R' => b'Y',
        b'Y' => b'R',
        b'r' => b'y',
        b'y' => b'r',
        b'K' => b'M',
        b'M' => b'K',
        b'k' => b'm',
        b'm' => b'k',
        b'B' => b'V',
        b'V' => b'B',
        b'b' => b'v',
        b'v' => b'b',
        b'D' => b'H',
        b'H' => b'D',
        b'd' => b'h',
        b'h' => b'd',
        other => other,
    }
}
