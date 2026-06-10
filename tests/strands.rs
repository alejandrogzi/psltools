// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Negative-strand and translated coordinate rules (§1.3 of the plan).

use psltools::{Reader, Strand};

#[test]
fn negative_strand_query_block_forward_intervals() {
    // qStrand=-, qSize=61, two blocks with raw (reverse-complement) qStarts 0,10
    // and sizes 8,10. The forward-strand intervals follow
    // [qSize - (qStarts[i] + size), qSize - qStarts[i]).
    let reader = Reader::<psltools::Psl>::from_path("tests/data/neg_strand.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert_eq!(psl.strands.query, Strand::Reverse);

    // block 0: [61 - (0 + 8), 61 - 0) = [53, 61)
    assert_eq!(psl.query_block_forward(0), 53..61);
    // block 1: [61 - (10 + 10), 61 - 10) = [41, 51)
    assert_eq!(psl.query_block_forward(1), 41..51);

    // The forward query span equals [qStart, qEnd).
    assert_eq!(psl.query_start, 41);
    assert_eq!(psl.query_end, 61);
}

#[test]
fn positive_strand_query_block_forward_is_identity() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/basic.psl").expect("read");
    let psl = &reader.as_slice()[1]; // two blocks, + strand
    assert_eq!(psl.query_block_forward(0), 0..5);
    assert_eq!(psl.query_block_forward(1), 7..12);
}

#[test]
fn translated_protein_is_detected_and_uses_size_mul_3() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/translated.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert!(
        psl.is_protein(),
        "++ record with tEnd == 3*size should be protein"
    );
    assert_eq!(psl.size_mul(), 3);
    // Reference block interval applies sizeMul: [0, 0 + 10*3) = [0, 30)
    assert_eq!(psl.reference_block_interval(0), 0..30);
}

#[test]
fn plain_dna_is_not_protein() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/basic.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert!(!psl.is_protein());
    assert_eq!(psl.size_mul(), 1);
}

#[test]
fn overlaps_reference_region() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/basic.psl").expect("read");
    let psl = &reader.as_slice()[0]; // chr1 [5, 15)
    assert!(psl.overlaps_reference(b"chr1", 10, 20));
    assert!(!psl.overlaps_reference(b"chr1", 15, 20)); // half-open: no overlap at end
    assert!(!psl.overlaps_reference(b"chr2", 0, 100));
}
