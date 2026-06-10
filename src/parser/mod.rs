// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! PSL parsing primitives.
//!
//! Turns raw bytes into [`PslMeta`](common::PslMeta) records plus a shared SoA
//! block arena. The [`sequential`] path parses line by line; the [`parallel`]
//! path (feature `parallel`) splits the buffer into line-aligned chunks and
//! merges the results in order.

pub(crate) mod common;
pub(crate) mod sequential;

#[cfg(feature = "parallel")]
mod parallel;

pub(crate) use sequential::parse_psl_sequential;

#[cfg(feature = "index")]
pub(crate) use sequential::locate_line_ranges;

#[cfg(feature = "parallel")]
pub(crate) use parallel::parse_psl_parallel;
