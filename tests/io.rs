// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use psltools::{Psl, Reader};

#[cfg(feature = "gzip")]
fn names(reader: &Reader<Psl>) -> Vec<(Vec<u8>, psltools::Coord)> {
    reader
        .records()
        .map(|p| (p.query_name_bytes().to_vec(), p.query_start))
        .collect()
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_matches_sequential() {
    // Build a larger input so the parallel chunker actually splits.
    let mut input = String::new();
    for i in 0..5000u32 {
        input.push_str(&format!(
            "10\t0\t0\t0\t0\t0\t0\t0\t+\tq{i}\t{}\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n",
            i + 20
        ));
    }
    let seq = Reader::<Psl>::from_owned_bytes(input.as_bytes().to_vec()).expect("seq");
    let par = Reader::<Psl>::from_owned_bytes_parallel(input.into_bytes()).expect("par");
    assert_eq!(seq.len(), par.len());
    for (a, b) in seq.records().zip(par.records()) {
        assert_eq!(a.query_name_bytes(), b.query_name_bytes());
        assert_eq!(a.blocks.sizes(), b.blocks.sizes());
        assert_eq!(a.reference_start, b.reference_start);
    }
}

#[cfg(feature = "gzip")]
#[test]
fn reads_gzip_transparently() {
    let plain = Reader::<Psl>::from_path("tests/data/basic.psl").expect("plain");
    let gz = Reader::<Psl>::from_path("tests/data/basic.psl.gz").expect("gz");
    assert_eq!(names(&plain), names(&gz));
}

#[test]
fn options_default_mrna_false() {
    let reader = Reader::<Psl>::from_path("tests/data/basic.psl").expect("read");
    assert!(!reader.options().mrna);
}
