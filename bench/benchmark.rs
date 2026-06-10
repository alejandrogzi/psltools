// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Minimal throughput benchmark for the PSL parser and writer.
//!
//! Usage: `benchmark <file.psl> [iterations]`. Reports parse throughput (MB/s)
//! sequentially and — when built with the `parallel` feature — in parallel, plus
//! write throughput. Intended as a coarse regression tracker, not a microbench
//! harness.

use std::time::Instant;

use psltools::{Reader, write_psl};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: benchmark <file.psl> [iterations]");
        std::process::exit(2);
    };
    let iterations: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);

    let bytes = std::fs::read(&path).expect("read input");
    let mb = bytes.len() as f64 / 1_000_000.0;
    eprintln!("input: {path} ({mb:.2} MB)");

    for i in 0..iterations {
        let start = Instant::now();
        let reader = Reader::<psltools::Psl>::from_owned_bytes(bytes.clone()).expect("parse");
        let elapsed = start.elapsed();
        eprintln!(
            "[seq  iter {i}] {} records in {elapsed:?} ({:.1} MB/s)",
            reader.len(),
            mb / elapsed.as_secs_f64()
        );
    }

    #[cfg(feature = "parallel")]
    for i in 0..iterations {
        let start = Instant::now();
        let reader =
            Reader::<psltools::Psl>::from_owned_bytes_parallel(bytes.clone()).expect("parse");
        let elapsed = start.elapsed();
        eprintln!(
            "[par  iter {i}] {} records in {elapsed:?} ({:.1} MB/s)",
            reader.len(),
            mb / elapsed.as_secs_f64()
        );
    }

    let reader = Reader::<psltools::Psl>::from_owned_bytes(bytes).expect("parse");
    let mut out = Vec::with_capacity(reader.len() * 64);
    let start = Instant::now();
    for psl in reader.records() {
        write_psl(&mut out, psl).expect("write");
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[write] {} records, {} bytes in {elapsed:?} ({:.1} MB/s)",
        reader.len(),
        out.len(),
        out.len() as f64 / 1_000_000.0 / elapsed.as_secs_f64()
    );
}
