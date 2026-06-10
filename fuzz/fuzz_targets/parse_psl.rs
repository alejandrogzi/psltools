// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Fuzz the PSL parser on arbitrary bytes. The contract is that parsing must
//! only ever return `Ok` or `Err(PslError)` — never panic, loop forever, or
//! invoke undefined behavior.
//!
//! Run with: `cargo +nightly fuzz run parse_psl`

#![no_main]

use std::io::BufReader;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Whole-file parser (sequential and parallel must agree on never panicking).
    let _ = psltools::Reader::<psltools::Psl>::from_owned_bytes(data.to_vec());
    let _ = psltools::Reader::<psltools::Psl>::from_owned_bytes_parallel(data.to_vec());

    // Low-memory streaming parser.
    let mut reader = psltools::StreamingReader::new(BufReader::new(data));
    loop {
        match reader.next_record() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
});
