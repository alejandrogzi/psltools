// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Record-level operations shared by library users and the CLI.
//!
//! Pure functions over any [`PslRecord`](crate::model::psl::PslRecord):
//! [`score`] (Kent-exact scoring/identity), [`swap`] (query/reference swap),
//! [`region`] (coordinate and overlap helpers), [`convert`] (BED12), and
//! [`check`] (invariant validation).

pub mod check;
pub mod convert;
pub mod region;
pub mod score;
pub mod swap;
