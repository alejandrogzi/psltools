// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

#[cfg(feature = "index")]
use std::ops::Range;

use crate::PslError;
use crate::model::block::Blocks;

use super::common::{PslMeta, looks_like_record, parse_record, read_line};

/// Parses every PSL/PSLx record in `buf` sequentially.
///
/// Non-record lines (the `psLayout` header block, `track`/`browser` lines, blank
/// lines, and comments) are skipped via the cheap [`looks_like_record`]
/// predicate. Returns one [`PslMeta`] per record plus the shared SoA block arena
/// they all index into.
pub(crate) fn parse_psl_sequential(buf: &[u8]) -> Result<(Vec<PslMeta>, Blocks), PslError> {
    let mut metas = Vec::new();
    let mut blocks = Blocks::new();
    let mut pos = 0usize;
    let len = buf.len();

    while pos < len {
        let line_start = pos;
        let (next_pos, line) = read_line(buf, pos);
        pos = next_pos;
        if !looks_like_record(line) {
            continue;
        }
        let meta = parse_record(line, line_start, &mut blocks)?;
        metas.push(meta);
    }

    Ok((metas, blocks))
}

/// Locates the byte range of every record line in `buf`.
///
/// Each range covers one record's content (without the trailing newline/CR).
/// Used by the record-offset index. Non-record lines are skipped, matching
/// [`parse_psl_sequential`].
#[cfg(feature = "index")]
pub(crate) fn locate_line_ranges(buf: &[u8]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut pos = 0usize;
    let len = buf.len();

    while pos < len {
        let line_start = pos;
        let (next_pos, line) = read_line(buf, pos);
        pos = next_pos;
        if !looks_like_record(line) {
            continue;
        }
        ranges.push(line_start..(line_start + line.len()));
    }

    ranges
}
