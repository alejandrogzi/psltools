// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use psltools::{Psl, PslRecord, Reader, write_psl};

fn records_equal(a: &Psl, b: &Psl) -> bool {
    a.matches == b.matches
        && a.mismatches == b.mismatches
        && a.rep_matches == b.rep_matches
        && a.n_count == b.n_count
        && a.query_num_insert == b.query_num_insert
        && a.query_base_insert == b.query_base_insert
        && a.reference_num_insert == b.reference_num_insert
        && a.reference_base_insert == b.reference_base_insert
        && a.strands == b.strands
        && a.query_name_bytes() == b.query_name_bytes()
        && a.query_size == b.query_size
        && a.query_start == b.query_start
        && a.query_end == b.query_end
        && a.reference_name_bytes() == b.reference_name_bytes()
        && a.reference_size == b.reference_size
        && a.reference_start == b.reference_start
        && a.reference_end == b.reference_end
        && a.blocks.sizes() == b.blocks.sizes()
        && a.blocks.query_starts() == b.blocks.query_starts()
        && a.blocks.reference_starts() == b.blocks.reference_starts()
        && a.query_seq() == b.query_seq()
        && a.reference_seq() == b.reference_seq()
}

fn roundtrip(path: &str) {
    let reader = Reader::<Psl>::from_path(path).expect("read");
    let mut buf = Vec::new();
    for psl in reader.records() {
        write_psl(&mut buf, psl).expect("write");
    }
    let reparsed = Reader::<Psl>::from_owned_bytes(buf).expect("reparse");

    assert_eq!(reader.len(), reparsed.len(), "record count for {path}");
    for (orig, round) in reader.records().zip(reparsed.records()) {
        assert!(
            records_equal(orig, round),
            "round-trip mismatch in {path}: {} vs {}",
            orig.query_name_str(),
            round.query_name_str()
        );
    }
}

#[test]
fn roundtrip_basic() {
    roundtrip("tests/data/basic.psl");
}

#[test]
fn roundtrip_neg_strand() {
    roundtrip("tests/data/neg_strand.psl");
}

#[test]
fn roundtrip_translated() {
    roundtrip("tests/data/translated.psl");
}

#[test]
fn roundtrip_pslx() {
    roundtrip("tests/data/pslx.psl");
}

#[test]
fn roundtrip_with_header() {
    // The header is skipped on read; the records still round-trip.
    roundtrip("tests/data/with_header.psl");
}

#[test]
fn writer_emits_canonical_tabs_and_trailing_commas() {
    let reader = Reader::<Psl>::from_path("tests/data/basic.psl").expect("read");
    let mut buf = Vec::new();
    write_psl(&mut buf, &reader.as_slice()[0]).expect("write");
    let line = String::from_utf8(buf).unwrap();
    assert_eq!(
        line,
        "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n"
    );
}
