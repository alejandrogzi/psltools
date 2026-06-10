// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! # psltools
//!
//! A high-performance library and CLI for the PSL alignment format (BLAT/lastz
//! output), modeled on [`chaintools`](https://github.com/alejandrogzi/chaintools).
//!
//! ## Design
//!
//! - **Zero-copy parsing.** Names and PSLx sequence columns are [`ByteSlice`]
//!   views into a shared (memory-mapped or owned) buffer; the three block
//!   coordinate lists live in one structure-of-arrays arena. The whole-file
//!   [`Reader`] allocates nothing per record.
//! - **One record per line.** Unlike chain, a PSL record is a single line, so
//!   parsing chunks in parallel, indexing, and streaming are all simple.
//! - **Kent-exact scoring.** [`psl_score`] / [`milli_bad`] / [`percent_id`]
//!   reproduce `kent/src/lib/psl.c` bit-for-bit (including `repMatch >> 1`,
//!   `sizeMul` derivation, and `i32` integer semantics).
//! - **Correctness on the hard corners.** Negative-strand `qStarts`/`tStarts`,
//!   translated (two-char) strands, and PSLx sequence columns are handled in the
//!   model from the start.
//!
//! ## Quick start
//!
//! ```no_run
//! use psltools::Reader;
//!
//! let reader = Reader::<psltools::Psl>::from_path("example.psl")?;
//! for psl in reader.records() {
//!     println!(
//!         "{} -> {}  score={}",
//!         psl.query_name_str(),
//!         psl.reference_name_str(),
//!         psl.score()
//!     );
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Feature flags
//!
//! - `mmap` (default): memory-map inputs for zero-copy parsing.
//! - `cli` (default): build the `psltools` binary (implies `parallel`).
//! - `gzip`: transparently read/write `.gz` PSL.
//! - `parallel`: multi-threaded parsing and parallel record iteration.
//! - `index`: record-offset and interval indexes.
//! - `serde`: derive `Serialize`/`Deserialize` on the owned types.
//! - `bigcoords`: widen [`Coord`](model::Coord) to `u64` for exotic assemblies.

pub mod io;
pub mod model;
pub mod ops;
pub mod parser;

pub use model::Coord;
pub use model::block::{Block, BlockSlice, Blocks};
pub use model::error::PslError;
pub use model::psl::{Psl, PslFlavor, PslRecord, PslxSeq, Strand, Strands};

pub use io::reader::{Reader, ReaderOptions};
pub use io::storage::{ByteSlice, SharedBytes};
pub use io::stream::{OwnedBlocks, OwnedPsl, OwnedPslHeader, StreamItem, StreamingReader};
pub use io::writer::{PslWriter, write_psl, write_psl_header};

#[cfg(feature = "index")]
pub use io::index::{IntervalIndex, PslIndex, PslSpan};

pub use ops::check::{CheckReport, check};
pub use ops::convert::{to_bed, to_bed12, to_genepred};
pub use ops::score::{ScoreOpts, milli_bad, percent_id, psl_score};
pub use ops::swap::{swap, swap_with};

/// Re-export of the [`genepred`] crate, the engine behind BED conversion. Use
/// its BED markers (`psltools::genepred::Bed6`, etc.) to pick a layout for
/// [`to_bed`].
pub use genepred;
