// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Core PSL data model.
//!
//! The fundamental, dependency-light types shared by the rest of the crate: the
//! [`psl`] record ([`Psl`](psl::Psl), [`Strand`](psl::Strand),
//! [`Strands`](psl::Strands), [`PslFlavor`](psl::PslFlavor)), the [`block`]
//! arena ([`Block`](block::Block), [`Blocks`](block::Blocks),
//! [`BlockSlice`](block::BlockSlice)), and the crate-wide [`error`] type
//! ([`PslError`](error::PslError)).

pub mod block;
pub mod error;
pub mod psl;

/// The scalar type used for genomic coordinates and sequence sizes.
///
/// Defaults to `u32`, which keeps the dominant block-list arena compact. Build
/// with the `bigcoords` feature to widen it to `u64` for exotic assemblies whose
/// single scaffolds approach or exceed 4.29 Gbp. Counts (`matches`, …) are
/// always `u32` regardless of this feature.
#[cfg(not(feature = "bigcoords"))]
pub type Coord = u32;

/// The scalar type used for genomic coordinates and sequence sizes (`u64` under
/// the `bigcoords` feature).
#[cfg(feature = "bigcoords")]
pub type Coord = u64;
