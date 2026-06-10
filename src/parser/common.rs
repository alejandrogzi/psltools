// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::borrow::Cow;
use std::ops::Range;

use memchr::memchr;

use crate::PslError;
use crate::model::Coord;
use crate::model::block::Blocks;
use crate::model::psl::{Strand, Strands};

/// Parsed metadata for one PSL record.
///
/// Mirrors the on-disk layout but stores name and PSLx sequence columns as byte
/// ranges into the source buffer (for zero-copy reconstruction) and the three
/// block lists as a single `Range` into the shared SoA arena. `blockCount` is
/// validated against the list lengths at parse time and not stored separately.
#[derive(Debug, Clone)]
pub(crate) struct PslMeta {
    pub matches: u32,
    pub mismatches: u32,
    pub rep_matches: u32,
    pub n_count: u32,
    pub query_num_insert: u32,
    pub query_base_insert: u32,
    pub reference_num_insert: u32,
    pub reference_base_insert: u32,
    pub strands: Strands,
    pub query_name: Range<usize>,
    pub query_size: Coord,
    pub query_start: Coord,
    pub query_end: Coord,
    pub reference_name: Range<usize>,
    pub reference_size: Coord,
    pub reference_start: Coord,
    pub reference_end: Coord,
    pub blocks: Range<usize>,
    pub seq: Option<(Range<usize>, Range<usize>)>,
}

/// Reads a line from `bytes` starting at `start`.
///
/// Returns the position after the newline and the line content without the
/// trailing `\n`/`\r`.
pub(crate) fn read_line(bytes: &[u8], start: usize) -> (usize, &[u8]) {
    if start >= bytes.len() {
        return (bytes.len(), &bytes[bytes.len()..]);
    }
    match memchr(b'\n', &bytes[start..]) {
        Some(rel) => {
            let end = start + rel;
            let mut line = &bytes[start..end];
            if let Some(stripped) = line.strip_suffix(b"\r") {
                line = stripped;
            }
            (end + 1, line)
        }
        None => {
            let mut line = &bytes[start..];
            if let Some(stripped) = line.strip_suffix(b"\r") {
                line = stripped;
            }
            (bytes.len(), line)
        }
    }
}

/// Returns `true` if the line is empty or all-whitespace.
pub(crate) fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|b| b.is_ascii_whitespace())
}

/// Fast predicate: a PSL record line starts with an ASCII digit (the `matches`
/// column). Header/`track`/`browser`/comment lines do not. Used to skip the
/// optional `psLayout` header block and other non-record lines cheaply.
pub(crate) fn looks_like_record(line: &[u8]) -> bool {
    line.first().is_some_and(u8::is_ascii_digit)
}

/// Cursor over the tab-separated fields of a single line.
///
/// Yields `(start, end)` ranges relative to the line. The number of fields is
/// always one more than the number of tabs.
pub(crate) struct FieldCursor<'a> {
    line: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> FieldCursor<'a> {
    pub(crate) fn new(line: &'a [u8]) -> Self {
        FieldCursor {
            line,
            pos: 0,
            done: false,
        }
    }

    /// Returns the next tab-delimited field as a `(start, end)` range, or `None`
    /// once the last field has been yielded.
    pub(crate) fn next(&mut self) -> Option<(usize, usize)> {
        if self.done {
            return None;
        }
        let start = self.pos;
        match memchr(b'\t', &self.line[start..]) {
            Some(rel) => {
                let end = start + rel;
                self.pos = end + 1;
                Some((start, end))
            }
            None => {
                self.done = true;
                Some((start, self.line.len()))
            }
        }
    }
}

/// Returns the next field range or a "missing column" format error.
fn need_field(
    cursor: &mut FieldCursor<'_>,
    line_offset: usize,
    label: &'static str,
) -> Result<(usize, usize), PslError> {
    cursor.next().ok_or_else(|| PslError::Format {
        offset: line_offset,
        msg: Cow::Owned(format!("missing column: {label}")),
    })
}

/// Parses a complete PSL/PSLx record line into a [`PslMeta`], appending its
/// block lists to the shared `blocks` arena.
///
/// `line_offset` is the absolute byte offset of the line in the source buffer;
/// it is used both for error reporting and to record absolute ranges for the
/// name and PSLx sequence columns.
pub(crate) fn parse_record(
    line: &[u8],
    line_offset: usize,
    blocks: &mut Blocks,
) -> Result<PslMeta, PslError> {
    let mut cursor = FieldCursor::new(line);

    let matches = u32_field(&mut cursor, line, line_offset, "matches")?;
    let mismatches = u32_field(&mut cursor, line, line_offset, "misMatches")?;
    let rep_matches = u32_field(&mut cursor, line, line_offset, "repMatches")?;
    let n_count = u32_field(&mut cursor, line, line_offset, "nCount")?;
    let query_num_insert = u32_field(&mut cursor, line, line_offset, "qNumInsert")?;
    let query_base_insert = u32_field(&mut cursor, line, line_offset, "qBaseInsert")?;
    let reference_num_insert = u32_field(&mut cursor, line, line_offset, "tNumInsert")?;
    let reference_base_insert = u32_field(&mut cursor, line, line_offset, "tBaseInsert")?;

    let (s, e) = need_field(&mut cursor, line_offset, "strand")?;
    let strands = parse_strands(&line[s..e], line_offset + s)?;

    let query_name = range_field(&mut cursor, line_offset, "qName")?;
    let query_size = coord_field(&mut cursor, line, line_offset, "qSize")?;
    let query_start = coord_field(&mut cursor, line, line_offset, "qStart")?;
    let query_end = coord_field(&mut cursor, line, line_offset, "qEnd")?;

    let reference_name = range_field(&mut cursor, line_offset, "tName")?;
    let reference_size = coord_field(&mut cursor, line, line_offset, "tSize")?;
    let reference_start = coord_field(&mut cursor, line, line_offset, "tStart")?;
    let reference_end = coord_field(&mut cursor, line, line_offset, "tEnd")?;

    let block_count = u32_field(&mut cursor, line, line_offset, "blockCount")? as usize;

    let block_start = blocks.len();
    let (bs_s, bs_e) = need_field(&mut cursor, line_offset, "blockSizes")?;
    let n_sizes = parse_coord_list(&line[bs_s..bs_e], line_offset + bs_s, "blockSizes", |v| {
        blocks.sizes_mut().push(v)
    })?;
    let (qs_s, qs_e) = need_field(&mut cursor, line_offset, "qStarts")?;
    let n_query = parse_coord_list(&line[qs_s..qs_e], line_offset + qs_s, "qStarts", |v| {
        blocks.query_starts_mut().push(v)
    })?;
    let (ts_s, ts_e) = need_field(&mut cursor, line_offset, "tStarts")?;
    let n_reference = parse_coord_list(&line[ts_s..ts_e], line_offset + ts_s, "tStarts", |v| {
        blocks.reference_starts_mut().push(v)
    })?;

    if n_sizes != block_count || n_query != block_count || n_reference != block_count {
        return Err(PslError::Format {
            offset: line_offset,
            msg: Cow::Owned(format!(
                "blockCount ({block_count}) disagrees with list lengths \
                 (blockSizes={n_sizes}, qStarts={n_query}, tStarts={n_reference})"
            )),
        });
    }
    let blocks_range = block_start..(block_start + n_sizes);

    // Optional PSLx sequence columns.
    let seq = match cursor.next() {
        Some((q_s, q_e)) => {
            let (t_s, t_e) = need_field(&mut cursor, line_offset, "tSeq")?;
            if cursor.next().is_some() {
                return Err(PslError::Format {
                    offset: line_offset,
                    msg: Cow::Borrowed("too many columns (expected 21 for PSL or 23 for PSLx)"),
                });
            }
            Some((
                (line_offset + q_s)..(line_offset + q_e),
                (line_offset + t_s)..(line_offset + t_e),
            ))
        }
        None => None,
    };

    Ok(PslMeta {
        matches,
        mismatches,
        rep_matches,
        n_count,
        query_num_insert,
        query_base_insert,
        reference_num_insert,
        reference_base_insert,
        strands,
        query_name,
        query_size,
        query_start,
        query_end,
        reference_name,
        reference_size,
        reference_start,
        reference_end,
        blocks: blocks_range,
        seq,
    })
}

/// Parses the strand column (`+`, `-`, or two chars `qStrand`+`tStrand`).
fn parse_strands(field: &[u8], offset: usize) -> Result<Strands, PslError> {
    match field.len() {
        1 => Ok(Strands {
            query: parse_strand_byte(field[0], offset)?,
            reference: None,
        }),
        2 => Ok(Strands {
            query: parse_strand_byte(field[0], offset)?,
            reference: Some(parse_strand_byte(field[1], offset + 1)?),
        }),
        _ => Err(PslError::Format {
            offset,
            msg: Cow::Borrowed("strand must be 1 or 2 characters of '+'/'-'"),
        }),
    }
}

fn parse_strand_byte(b: u8, offset: usize) -> Result<Strand, PslError> {
    match b {
        b'+' => Ok(Strand::Forward),
        b'-' => Ok(Strand::Reverse),
        _ => Err(PslError::Format {
            offset,
            msg: Cow::Borrowed("strand character must be '+' or '-'"),
        }),
    }
}

/// Parses a comma-separated coordinate list (with optional trailing comma),
/// invoking `push` for each entry and returning the count. Interior empty
/// entries are errors. The `push` sink lets the whole-file path fill the SoA
/// arena and the streaming path fill a `SmallVec` from the same code.
pub(crate) fn parse_coord_list<F: FnMut(Coord)>(
    field: &[u8],
    base_offset: usize,
    label: &'static str,
    mut push: F,
) -> Result<usize, PslError> {
    let body = field.strip_suffix(b",").unwrap_or(field);
    if body.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    let mut start = 0usize;
    loop {
        let rel = memchr(b',', &body[start..]);
        let end = rel.map_or(body.len(), |r| start + r);
        let token = &body[start..end];
        if token.is_empty() {
            return Err(PslError::Format {
                offset: base_offset + start,
                msg: Cow::Owned(format!("{label} contains an empty entry")),
            });
        }
        push(parse_coord(token, base_offset + start, label)?);
        count += 1;
        match rel {
            Some(_) => start = end + 1,
            None => break,
        }
    }
    Ok(count)
}

/// Header portion of a streamed record: every scalar/name parsed, but the three
/// block lists and PSLx sequence columns located only as line-relative ranges
/// (left unparsed so header-only filters can reject before paying for them).
#[derive(Debug, Clone)]
pub(crate) struct StreamHeader {
    pub matches: u32,
    pub mismatches: u32,
    pub rep_matches: u32,
    pub n_count: u32,
    pub query_num_insert: u32,
    pub query_base_insert: u32,
    pub reference_num_insert: u32,
    pub reference_base_insert: u32,
    pub strands: Strands,
    pub query_name: Range<usize>,
    pub query_size: Coord,
    pub query_start: Coord,
    pub query_end: Coord,
    pub reference_name: Range<usize>,
    pub reference_size: Coord,
    pub reference_start: Coord,
    pub reference_end: Coord,
    pub block_count: usize,
    pub block_sizes: Range<usize>,
    pub query_starts: Range<usize>,
    pub reference_starts: Range<usize>,
    pub seq: Option<(Range<usize>, Range<usize>)>,
}

/// Parses the scalar/name columns of a record line and locates (without parsing)
/// the block-list and PSLx sequence columns. All returned ranges are relative to
/// `line`; `line_offset` is used only for error offsets.
pub(crate) fn parse_stream_record(
    line: &[u8],
    line_offset: usize,
) -> Result<StreamHeader, PslError> {
    let mut cursor = FieldCursor::new(line);

    let matches = u32_field(&mut cursor, line, line_offset, "matches")?;
    let mismatches = u32_field(&mut cursor, line, line_offset, "misMatches")?;
    let rep_matches = u32_field(&mut cursor, line, line_offset, "repMatches")?;
    let n_count = u32_field(&mut cursor, line, line_offset, "nCount")?;
    let query_num_insert = u32_field(&mut cursor, line, line_offset, "qNumInsert")?;
    let query_base_insert = u32_field(&mut cursor, line, line_offset, "qBaseInsert")?;
    let reference_num_insert = u32_field(&mut cursor, line, line_offset, "tNumInsert")?;
    let reference_base_insert = u32_field(&mut cursor, line, line_offset, "tBaseInsert")?;

    let (s, e) = need_field(&mut cursor, line_offset, "strand")?;
    let strands = parse_strands(&line[s..e], line_offset + s)?;

    let query_name = relative_field(&mut cursor, line_offset, "qName")?;
    let query_size = coord_field(&mut cursor, line, line_offset, "qSize")?;
    let query_start = coord_field(&mut cursor, line, line_offset, "qStart")?;
    let query_end = coord_field(&mut cursor, line, line_offset, "qEnd")?;

    let reference_name = relative_field(&mut cursor, line_offset, "tName")?;
    let reference_size = coord_field(&mut cursor, line, line_offset, "tSize")?;
    let reference_start = coord_field(&mut cursor, line, line_offset, "tStart")?;
    let reference_end = coord_field(&mut cursor, line, line_offset, "tEnd")?;

    let block_count = u32_field(&mut cursor, line, line_offset, "blockCount")? as usize;
    let block_sizes = relative_field(&mut cursor, line_offset, "blockSizes")?;
    let query_starts = relative_field(&mut cursor, line_offset, "qStarts")?;
    let reference_starts = relative_field(&mut cursor, line_offset, "tStarts")?;

    let seq = match cursor.next() {
        Some((q_s, q_e)) => {
            let (t_s, t_e) = need_field(&mut cursor, line_offset, "tSeq")?;
            if cursor.next().is_some() {
                return Err(PslError::Format {
                    offset: line_offset,
                    msg: Cow::Borrowed("too many columns (expected 21 for PSL or 23 for PSLx)"),
                });
            }
            Some((q_s..q_e, t_s..t_e))
        }
        None => None,
    };

    Ok(StreamHeader {
        matches,
        mismatches,
        rep_matches,
        n_count,
        query_num_insert,
        query_base_insert,
        reference_num_insert,
        reference_base_insert,
        strands,
        query_name,
        query_size,
        query_start,
        query_end,
        reference_name,
        reference_size,
        reference_start,
        reference_end,
        block_count,
        block_sizes,
        query_starts,
        reference_starts,
        seq,
    })
}

// ---- field helpers ---------------------------------------------------------

fn u32_field(
    cursor: &mut FieldCursor<'_>,
    line: &[u8],
    line_offset: usize,
    label: &'static str,
) -> Result<u32, PslError> {
    let (s, e) = need_field(cursor, line_offset, label)?;
    parse_u32(&line[s..e], line_offset + s, label)
}

fn coord_field(
    cursor: &mut FieldCursor<'_>,
    line: &[u8],
    line_offset: usize,
    label: &'static str,
) -> Result<Coord, PslError> {
    let (s, e) = need_field(cursor, line_offset, label)?;
    parse_coord(&line[s..e], line_offset + s, label)
}

fn range_field(
    cursor: &mut FieldCursor<'_>,
    line_offset: usize,
    label: &'static str,
) -> Result<Range<usize>, PslError> {
    let (s, e) = need_field(cursor, line_offset, label)?;
    Ok((line_offset + s)..(line_offset + e))
}

/// Like [`range_field`] but returns a range relative to the line (used by the
/// streaming parser, whose ranges index back into its own line buffer).
fn relative_field(
    cursor: &mut FieldCursor<'_>,
    line_offset: usize,
    label: &'static str,
) -> Result<Range<usize>, PslError> {
    let (s, e) = need_field(cursor, line_offset, label)?;
    Ok(s..e)
}

// ---- scalar byte parsers (overflow-checked) --------------------------------

/// Parses an unsigned integer from raw bytes into a `u64`.
fn parse_u64(data: &[u8], offset: usize, ctx: &str) -> Result<u64, PslError> {
    if data.is_empty() {
        return Err(PslError::Format {
            offset,
            msg: Cow::Owned(format!("{ctx} is empty")),
        });
    }
    let mut value: u64 = 0;
    for (i, &b) in data.iter().enumerate() {
        let digit = b.wrapping_sub(b'0');
        if digit > 9 {
            return Err(PslError::Format {
                offset: offset + i,
                msg: Cow::Owned(format!("{ctx} contains a non-digit")),
            });
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit as u64))
            .ok_or_else(|| PslError::Format {
                offset: offset + i,
                msg: Cow::Owned(format!("{ctx} overflows u64")),
            })?;
    }
    Ok(value)
}

/// Parses a `u32` from raw bytes.
fn parse_u32(data: &[u8], offset: usize, ctx: &str) -> Result<u32, PslError> {
    let value = parse_u64(data, offset, ctx)?;
    if value > u32::MAX as u64 {
        return Err(PslError::Format {
            offset,
            msg: Cow::Owned(format!("{ctx} exceeds u32")),
        });
    }
    Ok(value as u32)
}

/// Parses a [`Coord`] from raw bytes (`u32` by default, `u64` with `bigcoords`).
#[cfg(not(feature = "bigcoords"))]
fn parse_coord(data: &[u8], offset: usize, ctx: &str) -> Result<Coord, PslError> {
    parse_u32(data, offset, ctx)
}

#[cfg(feature = "bigcoords")]
fn parse_coord(data: &[u8], offset: usize, ctx: &str) -> Result<Coord, PslError> {
    parse_u64(data, offset, ctx)
}
