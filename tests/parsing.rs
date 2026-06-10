// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use psltools::{PslError, Reader, Strand};

fn parse(input: &str) -> Reader {
    Reader::<psltools::Psl>::from_owned_bytes(input.as_bytes().to_vec()).expect("parse")
}

#[test]
fn parses_basic_records() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/basic.psl").expect("read");
    assert_eq!(reader.len(), 2);

    let first = &reader.as_slice()[0];
    assert_eq!(first.matches, 10);
    assert_eq!(first.query_name_str(), "q1");
    assert_eq!(first.reference_name_str(), "chr1");
    assert_eq!(first.query_start, 0);
    assert_eq!(first.query_end, 10);
    assert_eq!(first.reference_start, 5);
    assert_eq!(first.reference_end, 15);
    assert_eq!(first.strands.query, Strand::Forward);
    assert_eq!(first.strands.reference, None);
    assert_eq!(first.block_count(), 1);
    assert_eq!(first.blocks.sizes(), &[10]);
    assert_eq!(first.blocks.query_starts(), &[0]);
    assert_eq!(first.blocks.reference_starts(), &[5]);

    let second = &reader.as_slice()[1];
    assert_eq!(second.block_count(), 2);
    assert_eq!(second.blocks.sizes(), &[5, 5]);
    assert_eq!(second.blocks.query_starts(), &[0, 7]);
    assert_eq!(second.blocks.reference_starts(), &[10, 18]);
    assert_eq!(second.query_num_insert, 1);
    assert_eq!(second.query_base_insert, 2);
}

#[test]
fn skips_pslayout_header_and_track_lines() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/with_header.psl").expect("read");
    assert_eq!(reader.len(), 1);
    assert_eq!(reader.as_slice()[0].query_name_str(), "q1");
}

#[test]
fn skips_blank_and_browser_lines() {
    let input = "browser position chr1\ntrack name=foo\n\n\
        10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n\n";
    let reader = parse(input);
    assert_eq!(reader.len(), 1);
}

#[test]
fn handles_crlf_line_endings() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\r\n";
    let reader = parse(input);
    assert_eq!(reader.len(), 1);
    assert_eq!(reader.as_slice()[0].reference_name_str(), "chr1");
}

#[test]
fn rejects_blockcount_mismatch() {
    // blockCount says 1 but lists have 2 entries.
    let input =
        "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t5,5,\t0,7,\t10,18,\n";
    let err = Reader::<psltools::Psl>::from_owned_bytes(input.as_bytes().to_vec()).unwrap_err();
    assert!(matches!(err, PslError::Format { .. }), "got {err:?}");
}

#[test]
fn rejects_non_digit_field() {
    let input = "x\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n";
    // First column is non-digit, so the line is treated as a non-record and skipped.
    let reader = parse(input);
    assert_eq!(reader.len(), 0);
}

#[test]
fn rejects_bad_coordinate() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\tNOPE\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n";
    let err = Reader::<psltools::Psl>::from_owned_bytes(input.as_bytes().to_vec()).unwrap_err();
    match err {
        PslError::Format { msg, .. } => assert!(msg.contains("qSize"), "msg={msg}"),
        other => panic!("expected format error, got {other:?}"),
    }
}

#[test]
fn rejects_missing_columns() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t+\tq1\t20\n";
    let err = Reader::<psltools::Psl>::from_owned_bytes(input.as_bytes().to_vec()).unwrap_err();
    assert!(matches!(err, PslError::Format { .. }));
}

#[test]
fn rejects_invalid_strand() {
    let input = "10\t0\t0\t0\t0\t0\t0\t0\t*\tq1\t20\t0\t10\tchr1\t100\t5\t15\t1\t10,\t0,\t5,\n";
    let err = Reader::<psltools::Psl>::from_owned_bytes(input.as_bytes().to_vec()).unwrap_err();
    assert!(matches!(err, PslError::Format { .. }));
}

#[test]
fn parses_translated_two_char_strand() {
    let reader = Reader::<psltools::Psl>::from_path("tests/data/translated.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert_eq!(psl.strands.query, Strand::Forward);
    assert_eq!(psl.strands.reference, Some(Strand::Forward));
    assert!(psl.strands.is_translated());
}

#[test]
fn empty_input_yields_no_records() {
    let reader = parse("");
    assert!(reader.is_empty());
}
