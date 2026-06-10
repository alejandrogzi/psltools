// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use psltools::{Psl, PslRecord, Reader, check, swap, to_bed12};

fn same_record<A: PslRecord, B: PslRecord>(a: &A, b: &B) -> bool {
    a.matches() == b.matches()
        && a.mismatches() == b.mismatches()
        && a.rep_matches() == b.rep_matches()
        && a.n_count() == b.n_count()
        && a.query_num_insert() == b.query_num_insert()
        && a.query_base_insert() == b.query_base_insert()
        && a.reference_num_insert() == b.reference_num_insert()
        && a.reference_base_insert() == b.reference_base_insert()
        && a.strands() == b.strands()
        && a.query_name() == b.query_name()
        && a.query_size() == b.query_size()
        && a.query_start() == b.query_start()
        && a.query_end() == b.query_end()
        && a.reference_name() == b.reference_name()
        && a.reference_size() == b.reference_size()
        && a.reference_start() == b.reference_start()
        && a.reference_end() == b.reference_end()
        && a.block_sizes() == b.block_sizes()
        && a.query_starts() == b.query_starts()
        && a.reference_starts() == b.reference_starts()
}

fn first(path: &str) -> Psl {
    Reader::<Psl>::from_path(path).expect("read").as_slice()[0].clone()
}

#[test]
fn swap_exchanges_query_and_reference() {
    let psl = first("tests/data/basic.psl"); // q1/chr1, + strand
    let swapped = swap(&psl);
    assert_eq!(swapped.query_name, b"chr1");
    assert_eq!(swapped.reference_name, b"q1");
    assert_eq!(swapped.query_start, psl.reference_start);
    assert_eq!(swapped.reference_start, psl.query_start);
    assert_eq!(swapped.query_size, psl.reference_size);
}

#[test]
fn swap_is_involution_for_plus_strand() {
    for idx in 0..2 {
        let reader = Reader::<Psl>::from_path("tests/data/basic.psl").expect("read");
        let psl = &reader.as_slice()[idx];
        let twice = swap(&swap(psl));
        assert!(same_record(psl, &twice), "swap^2 != id for basic[{idx}]");
    }
}

#[test]
fn swap_is_involution_for_minus_strand() {
    let psl = first("tests/data/neg_strand.psl");
    let twice = swap(&swap(&psl));
    assert!(same_record(&psl, &twice), "swap^2 != id for minus strand");
}

#[test]
fn swap_is_involution_for_translated() {
    let psl = first("tests/data/translated.psl");
    let twice = swap(&swap(&psl));
    assert!(same_record(&psl, &twice), "swap^2 != id for translated");
}

#[test]
fn check_passes_on_valid_records() {
    for path in [
        "tests/data/basic.psl",
        "tests/data/neg_strand.psl",
        "tests/data/translated.psl",
        "tests/data/pslx.psl",
    ] {
        let reader = Reader::<Psl>::from_path(path).expect("read");
        for psl in reader.records() {
            let report = check(psl);
            assert!(
                report.is_ok(),
                "check failed for {path}: {:?}",
                report.violations
            );
        }
    }
}

#[test]
fn check_flags_span_inconsistency() {
    // qEnd-qStart (50) != sum(blockSizes=10) + qBaseInsert(0).
    let psl = Reader::<Psl>::from_owned_bytes(
        "10\t0\t0\t0\t0\t0\t0\t0\t+\tq\t100\t0\t50\tc\t100\t0\t10\t1\t10,\t0,\t0,\n"
            .as_bytes()
            .to_vec(),
    )
    .expect("parse")
    .as_slice()[0]
        .clone();
    let report = check(&psl);
    assert!(!report.is_ok());
    assert!(report.violations.iter().any(|v| v.contains("qEnd-qStart")));
}

#[test]
fn to_bed12_basic() {
    let psl = first("tests/data/basic.psl");
    // genepred BED12: chrom start end name score(0) strand thickStart thickEnd
    // itemRgb(0,0,0) blockCount blockSizes blockStarts (no trailing newline).
    let line = String::from_utf8(to_bed12(&psl)).unwrap();
    assert_eq!(line, "chr1\t5\t15\tq1\t0\t+\t5\t15\t0,0,0\t1\t10,\t0,");
}

#[test]
fn to_bed6_basic() {
    let psl = first("tests/data/basic.psl");
    let line = String::from_utf8(psltools::to_bed::<_, psltools::genepred::Bed6>(&psl)).unwrap();
    assert_eq!(line, "chr1\t5\t15\tq1\t0\t+");
}

#[test]
fn to_bed12_multiblock_uses_reference_intervals() {
    // basic.psl[1]: 2 blocks at reference 10..15 and 18..23 -> sizes 5,5; starts 0,8.
    let psl = Reader::<Psl>::from_path("tests/data/basic.psl")
        .expect("read")
        .as_slice()[1]
        .clone();
    let line = String::from_utf8(to_bed12(&psl)).unwrap();
    assert_eq!(line, "chr1\t10\t23\tq2\t0\t+\t10\t23\t0,0,0\t2\t5,5,\t0,8,");
}
