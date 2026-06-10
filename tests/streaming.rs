// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufReader, Cursor};

use psltools::{OwnedPsl, StreamingReader};

fn stream_all(input: &str) -> Vec<OwnedPsl> {
    let mut reader = StreamingReader::new(BufReader::new(Cursor::new(input.as_bytes().to_vec())));
    let mut out = Vec::new();
    while let Some(rec) = reader.next_record().expect("next") {
        out.push(rec);
    }
    out
}

#[test]
fn streams_records() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n\
        10\t0\t0\t0\t1\t2\t1\t3\t+\tq2\t20\t0\t12\tchr1\t100\t10\t23\t2\t5,5,\t0,7,\t10,18,\n";
    let recs = stream_all(input);
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].query_name, b"q1");
    assert_eq!(recs[0].block_sizes.as_slice(), &[10]);
    assert_eq!(recs[1].block_sizes.as_slice(), &[5, 5]);
    assert_eq!(recs[1].query_starts.as_slice(), &[0, 7]);
    assert_eq!(recs[0].score(), 10);
}

#[test]
fn streams_pslx_sequences() {
    let input = "5\t0\t0\t0\t0\t0\t0\t0\t+\tqx\t10\t0\t5\tchrx\t100\t20\t25\t1\t5,\t0,\t20,\tACGTA,\tACGTA,\n";
    let recs = stream_all(input);
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].seq.as_ref().unwrap().0, b"ACGTA,");
    assert_eq!(recs[0].seq.as_ref().unwrap().1, b"ACGTA,");
}

#[test]
fn header_first_filtering_skips_blocks() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n\
        10\t0\t0\t0\t0\t0\t0\t0\t+\tq2\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n";
    let mut reader = StreamingReader::new(BufReader::new(Cursor::new(input.as_bytes().to_vec())));
    let mut kept = Vec::new();
    while let Some(header) = reader.next_header().expect("header") {
        if header.query_name == b"q2" {
            let blocks = reader.read_blocks().expect("blocks");
            kept.push(header.into_psl(blocks));
        } else {
            reader.skip_blocks();
        }
    }
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].query_name, b"q2");
    assert_eq!(kept[0].block_sizes.as_slice(), &[10]);
}

#[test]
fn skips_header_and_blank_lines() {
    let input = "psLayout version 3\n\n\
        10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n";
    let recs = stream_all(input);
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].query_name, b"q1");
}
