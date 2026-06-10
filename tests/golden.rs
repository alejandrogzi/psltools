// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Kent-parity checks for `pslScore` / `pslCalcMilliBad` / percent identity.
//! Values are computed by hand from `kent/src/lib/psl.c`.

use psltools::{Psl, Reader};

fn single(input: &str) -> Psl {
    let reader = Reader::<Psl>::from_owned_bytes(input.as_bytes().to_vec()).expect("parse");
    assert_eq!(reader.len(), 1);
    reader.as_slice()[0].clone()
}

#[test]
fn score_simple_dna() {
    // matches=10, all else 0, sizeMul=1 => 1*(10+0) - 0 - 0 - 0 = 10
    let reader = Reader::<Psl>::from_path("tests/data/basic.psl").expect("read");
    assert_eq!(reader.as_slice()[0].score(), 10);
}

#[test]
fn score_protein_uses_size_mul_3() {
    // matches=10, protein => 3*(10+0) - 0 - 0 - 0 = 30
    let reader = Reader::<Psl>::from_path("tests/data/translated.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert!(psl.is_protein());
    assert_eq!(psl.score(), 30);
}

#[test]
fn score_with_mismatches_and_perfect_identity_block() {
    // match=90, mis=10, one block of 100, no inserts.
    let psl =
        single("90\t10\t0\t0\t0\t0\t0\t0\t+\tq\t100\t0\t100\tc\t1000\t0\t100\t1\t100,\t0,\t0,\n");
    // score = 1*(90 + 0) - 1*10 - 0 - 0 = 80
    assert_eq!(psl.score(), 80);
    // milliBad = 1000*(10*1 + 0 + round(3*ln(1))) / (90+10) = 1000*10/100 = 100
    assert_eq!(psl.milli_bad(false), 100);
    // percentId = 100 - 100*0.1 = 90
    assert!((psl.percent_id(false) - 90.0).abs() < 1e-9);
}

#[test]
fn milli_bad_exercises_log_term() {
    // 2 blocks of 25 (sum 50); query gap of 10 (qBaseInsert=10, qNumInsert=1).
    // qAliSize=60, tAliSize=50, sizeDif=10, insertFactor=1 (isMrna=false adds tNI=0),
    // total=50, round(3*ln(11))=7 => milliBad = 1000*(0+1+7)/50 = 160, pctId = 84.
    let psl = single(
        "50\t0\t0\t0\t1\t10\t0\t0\t+\tq\t100\t0\t60\tc\t1000\t0\t50\t2\t25,25,\t0,35,\t0,25,\n",
    );
    assert_eq!(psl.score(), 49); // 1*(50) - 0 - qNI(1) - tNI(0)
    assert_eq!(psl.milli_bad(false), 160);
    assert!((psl.percent_id(false) - 84.0).abs() < 1e-9);
}

#[test]
fn perfect_alignment_is_zero_milli_bad() {
    let reader = Reader::<Psl>::from_path("tests/data/basic.psl").expect("read");
    let psl = &reader.as_slice()[0];
    assert_eq!(psl.milli_bad(false), 0);
    assert!((psl.percent_id(false) - 100.0).abs() < 1e-9);
}
