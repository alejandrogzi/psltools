// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Structural validation of a PSL record, mirroring UCSC `pslCheck`.
//!
//! Verifies coordinate bounds, list-length agreement, block monotonicity, and
//! that the block spans reconcile with the alignment span and base-insert
//! counts (applying `sizeMul` on the reference side).

use crate::model::psl::PslRecord;
use crate::ops::score::size_mul;

/// The result of [`check`]: a list of human-readable invariant violations.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub violations: Vec<String>,
}

impl CheckReport {
    /// `true` when no invariants were violated.
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.violations.push(msg.into());
    }
}

/// Validates the structural invariants of a record.
// `Coord as u64` is a real widening for the default `u32` build; it is only a
// no-op under `bigcoords`, so the cast is conditionally necessary.
#[allow(clippy::unnecessary_cast)]
pub fn check<P: PslRecord>(p: &P) -> CheckReport {
    let mut report = CheckReport::default();

    let sizes = p.block_sizes();
    let q_starts = p.query_starts();
    let t_starts = p.reference_starts();

    // List lengths must agree with each other and with blockCount.
    let n = sizes.len();
    if q_starts.len() != n || t_starts.len() != n {
        report.fail(format!(
            "block list lengths disagree: blockSizes={}, qStarts={}, tStarts={}",
            n,
            q_starts.len(),
            t_starts.len()
        ));
        // Without equal lengths the remaining per-block checks are unreliable.
        return report;
    }
    if p.block_count() != n {
        report.fail(format!(
            "blockCount {} disagrees with list length {n}",
            p.block_count()
        ));
    }

    // Coordinate bounds.
    if p.query_start() > p.query_end() {
        report.fail(format!(
            "qStart ({}) > qEnd ({})",
            p.query_start(),
            p.query_end()
        ));
    }
    if p.query_end() > p.query_size() {
        report.fail(format!(
            "qEnd ({}) > qSize ({})",
            p.query_end(),
            p.query_size()
        ));
    }
    if p.reference_start() > p.reference_end() {
        report.fail(format!(
            "tStart ({}) > tEnd ({})",
            p.reference_start(),
            p.reference_end()
        ));
    }
    if p.reference_end() > p.reference_size() {
        report.fail(format!(
            "tEnd ({}) > tSize ({})",
            p.reference_end(),
            p.reference_size()
        ));
    }

    let mul = size_mul(p) as u64;

    // Block monotonicity (raw coordinates increase along the alignment).
    for i in 1..n {
        if (q_starts[i - 1] as u64) + (sizes[i - 1] as u64) > q_starts[i] as u64 {
            report.fail(format!("qStarts not monotonic at block {i}"));
        }
        if (t_starts[i - 1] as u64) + (sizes[i - 1] as u64) * mul > t_starts[i] as u64 {
            report.fail(format!("tStarts not monotonic at block {i}"));
        }
    }

    // Span consistency: span == sum(blockSizes [* sizeMul]) + baseInsert.
    let sum_sizes: u64 = sizes.iter().map(|&s| s as u64).sum();
    let query_span = (p.query_end() as u64).saturating_sub(p.query_start() as u64);
    let expected_query = sum_sizes + p.query_base_insert() as u64;
    if query_span != expected_query {
        report.fail(format!(
            "qEnd-qStart ({query_span}) != sum(blockSizes) + qBaseInsert ({expected_query})"
        ));
    }
    let reference_span = (p.reference_end() as u64).saturating_sub(p.reference_start() as u64);
    let expected_reference = sum_sizes * mul + p.reference_base_insert() as u64;
    if reference_span != expected_reference {
        report.fail(format!(
            "tEnd-tStart ({reference_span}) != sum(blockSizes)*sizeMul + tBaseInsert ({expected_reference})"
        ));
    }

    report
}
