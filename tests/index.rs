// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

#![cfg(feature = "index")]

use psltools::{IntervalIndex, Psl, PslIndex, Reader};

#[test]
fn record_offset_index_round_trips_bytes() {
    let index = PslIndex::from_path("tests/data/basic.psl").expect("index");
    assert_eq!(index.len(), 2);
    let first = index.record_bytes(0).expect("record 0");
    assert!(first.starts_with(b"10\t0\t0"));
    assert!(index.record_bytes(99).is_none());
}

#[test]
fn interval_index_matches_linear_scan() {
    let mut input = String::new();
    // 200 records on chr1 at increasing starts, plus some on chr2.
    for i in 0..200u32 {
        let start = i * 10;
        let end = start + 8;
        input.push_str(&format!(
            "8\t0\t0\t0\t0\t0\t0\t0\t+\tq{i}\t10000\t0\t8\tchr1\t100000\t{start}\t{end}\t1\t8,\t0,\t{start},\n"
        ));
    }
    input.push_str(
        "8\t0\t0\t0\t0\t0\t0\t0\t+\tqz\t10000\t0\t8\tchr2\t100000\t50\t58\t1\t8,\t0,\t50,\n",
    );

    let reader = Reader::<Psl>::from_owned_bytes(input.into_bytes()).expect("read");
    let records = reader.as_slice();
    let index = IntervalIndex::from_records(records);

    let (qs, qe): (psltools::Coord, psltools::Coord) = (45, 75);
    let mut expected: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, p)| p.overlaps_reference(b"chr1", qs, qe))
        .map(|(i, _)| i)
        .collect();
    expected.sort_unstable();

    let mut got = index.query(b"chr1", qs, qe);
    got.sort_unstable();

    assert_eq!(got, expected);
    assert!(!expected.is_empty());
    assert!(index.query(b"chr2", 0, 100).len() == 1);
    assert!(index.query(b"absent", 0, 100).is_empty());
}
