// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::ops::Range;

use memchr::memchr;
use rayon::prelude::*;

use crate::PslError;
use crate::model::block::Blocks;

use super::common::{PslMeta, looks_like_record, parse_record, read_line};

/// Parses every record in `buf` across the global rayon pool.
///
/// Because a PSL record is exactly one line, the buffer can be split into
/// line-aligned chunks that each parse independently — no per-record range
/// location is needed (unlike chain's multi-line records). Chunk results are
/// concatenated in input order, with each chunk's block ranges shifted by the
/// running length of the merged arena.
pub(crate) fn parse_psl_parallel(buf: &[u8]) -> Result<(Vec<PslMeta>, Blocks), PslError> {
    let target_chunks = (rayon::current_num_threads() * 4).max(1);
    let chunks = split_line_aligned(buf, target_chunks);

    let parsed: Result<Vec<(Vec<PslMeta>, Blocks)>, PslError> = chunks
        .into_par_iter()
        .map(|chunk| parse_chunk(buf, chunk))
        .collect();
    let parsed = parsed?;

    let total_metas = parsed.iter().map(|(metas, _)| metas.len()).sum();
    let total_blocks = parsed.iter().map(|(_, blocks)| blocks.len()).sum();
    let mut all_metas = Vec::with_capacity(total_metas);
    let mut all_blocks = Blocks::with_capacity(total_blocks);

    for (metas, mut blocks) in parsed {
        let offset = all_blocks.len();
        for mut meta in metas {
            meta.blocks = (meta.blocks.start + offset)..(meta.blocks.end + offset);
            all_metas.push(meta);
        }
        all_blocks.append(&mut blocks);
    }

    Ok((all_metas, all_blocks))
}

/// Parses all record lines within `chunk` of `buf` into a chunk-local arena.
///
/// Name/sequence ranges in the returned metas are absolute offsets into `buf`;
/// block ranges are local to the returned arena and are rebased by the caller.
fn parse_chunk(buf: &[u8], chunk: Range<usize>) -> Result<(Vec<PslMeta>, Blocks), PslError> {
    let slice = &buf[chunk.clone()];
    let mut metas = Vec::new();
    let mut blocks = Blocks::new();
    let mut pos = 0usize;
    let len = slice.len();

    while pos < len {
        let line_start = pos;
        let (next_pos, line) = read_line(slice, pos);
        pos = next_pos;
        if !looks_like_record(line) {
            continue;
        }
        let meta = parse_record(line, chunk.start + line_start, &mut blocks)?;
        metas.push(meta);
    }

    Ok((metas, blocks))
}

/// Splits `buf` into roughly `target` ranges, snapping every boundary forward to
/// just past the next newline so no record line straddles two chunks.
fn split_line_aligned(buf: &[u8], target: usize) -> Vec<Range<usize>> {
    let len = buf.len();
    if len == 0 {
        return Vec::new();
    }
    let approx = len / target.max(1);
    if approx == 0 {
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..len];
    }

    let mut ranges = Vec::with_capacity(target);
    let mut start = 0usize;
    while start < len {
        let mut end = (start + approx).min(len);
        if end < len {
            match memchr(b'\n', &buf[end..]) {
                Some(rel) => end += rel + 1,
                None => end = len,
            }
        }
        ranges.push(start..end);
        start = end;
    }
    ranges
}
