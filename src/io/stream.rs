// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{BufRead, BufReader};
use std::ops::Range;
use std::path::Path;

use smallvec::SmallVec;

#[cfg(feature = "gzip")]
use flate2::read::MultiGzDecoder;

use crate::PslError;
#[cfg(not(feature = "gzip"))]
use crate::io::storage::gzip_feature_error;
use crate::io::storage::is_gz_path;
use crate::model::Coord;
use crate::model::psl::{PslRecord, Strands};
use crate::ops::score::{self, ScoreOpts};
use crate::parser::common::{
    StreamHeader, is_blank, looks_like_record, parse_coord_list, parse_stream_record,
};

/// Capacity of the buffered reader wrapping file/gzip PSL inputs.
const INPUT_BUFFER_CAPACITY: usize = 1 << 20;

/// Inline capacity for the block lists of owned/streamed records.
///
/// Most BLAT/lastz records have only a handful of blocks, so keeping up to this
/// many inline avoids a heap allocation per list per record on the common path.
const INLINE_BLOCKS: usize = 8;

type BlockList = SmallVec<[Coord; INLINE_BLOCKS]>;

/// An owned PSL record for streaming/low-memory processing.
///
/// Stores everything by value (no shared buffer). The three block lists use a
/// small-vector so the common small-block case needs no heap allocation.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct OwnedPsl {
    pub matches: u32,
    pub mismatches: u32,
    pub rep_matches: u32,
    pub n_count: u32,
    pub query_num_insert: u32,
    pub query_base_insert: u32,
    pub reference_num_insert: u32,
    pub reference_base_insert: u32,
    pub strands: Strands,
    pub query_name: Vec<u8>,
    pub query_size: Coord,
    pub query_start: Coord,
    pub query_end: Coord,
    pub reference_name: Vec<u8>,
    pub reference_size: Coord,
    pub reference_start: Coord,
    pub reference_end: Coord,
    pub block_sizes: BlockList,
    pub query_starts: BlockList,
    pub reference_starts: BlockList,
    /// Raw PSLx sequence columns (`qSeq`, `tSeq`), as comma-separated lists.
    pub seq: Option<(Vec<u8>, Vec<u8>)>,
}

impl OwnedPsl {
    /// Number of alignment blocks.
    pub fn block_count(&self) -> usize {
        self.block_sizes.len()
    }

    /// Whether this record is a protein/translated alignment (derived geometry).
    pub fn is_protein(&self) -> bool {
        score::is_protein(self)
    }

    /// `3` for protein/translated alignments, else `1`.
    pub fn size_mul(&self) -> u32 {
        score::size_mul(self)
    }

    /// UCSC `pslScore`.
    pub fn score(&self) -> i64 {
        score::psl_score(self)
    }

    /// UCSC `pslCalcMilliBad`.
    pub fn milli_bad(&self, is_mrna: bool) -> i32 {
        score::milli_bad(self, ScoreOpts { is_mrna })
    }

    /// Percent identity (`100.0 - milliBad * 0.1`).
    pub fn percent_id(&self, is_mrna: bool) -> f64 {
        score::percent_id(self, ScoreOpts { is_mrna })
    }
}

impl PslRecord for OwnedPsl {
    fn matches(&self) -> u32 {
        self.matches
    }
    fn mismatches(&self) -> u32 {
        self.mismatches
    }
    fn rep_matches(&self) -> u32 {
        self.rep_matches
    }
    fn n_count(&self) -> u32 {
        self.n_count
    }
    fn query_num_insert(&self) -> u32 {
        self.query_num_insert
    }
    fn query_base_insert(&self) -> u32 {
        self.query_base_insert
    }
    fn reference_num_insert(&self) -> u32 {
        self.reference_num_insert
    }
    fn reference_base_insert(&self) -> u32 {
        self.reference_base_insert
    }
    fn strands(&self) -> Strands {
        self.strands
    }
    fn query_name(&self) -> &[u8] {
        &self.query_name
    }
    fn query_size(&self) -> Coord {
        self.query_size
    }
    fn query_start(&self) -> Coord {
        self.query_start
    }
    fn query_end(&self) -> Coord {
        self.query_end
    }
    fn reference_name(&self) -> &[u8] {
        &self.reference_name
    }
    fn reference_size(&self) -> Coord {
        self.reference_size
    }
    fn reference_start(&self) -> Coord {
        self.reference_start
    }
    fn reference_end(&self) -> Coord {
        self.reference_end
    }
    fn block_count(&self) -> usize {
        self.block_sizes.len()
    }
    fn block_sizes(&self) -> &[Coord] {
        &self.block_sizes
    }
    fn query_starts(&self) -> &[Coord] {
        &self.query_starts
    }
    fn reference_starts(&self) -> &[Coord] {
        &self.reference_starts
    }
    fn query_seq(&self) -> Option<&[u8]> {
        self.seq.as_ref().map(|(q, _)| q.as_slice())
    }
    fn reference_seq(&self) -> Option<&[u8]> {
        self.seq.as_ref().map(|(_, t)| t.as_slice())
    }
}

/// An owned PSL header (scalars + names, no block lists).
///
/// Lets a caller decide whether to keep a record from header-level fields before
/// paying to parse its comma-separated block lists.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct OwnedPslHeader {
    pub offset: usize,
    pub matches: u32,
    pub mismatches: u32,
    pub rep_matches: u32,
    pub n_count: u32,
    pub query_num_insert: u32,
    pub query_base_insert: u32,
    pub reference_num_insert: u32,
    pub reference_base_insert: u32,
    pub strands: Strands,
    pub query_name: Vec<u8>,
    pub query_size: Coord,
    pub query_start: Coord,
    pub query_end: Coord,
    pub reference_name: Vec<u8>,
    pub reference_size: Coord,
    pub reference_start: Coord,
    pub reference_end: Coord,
    pub block_count: usize,
}

impl OwnedPslHeader {
    /// Combines this header with parsed block lists into a complete record.
    pub fn into_psl(self, blocks: OwnedBlocks) -> OwnedPsl {
        OwnedPsl {
            matches: self.matches,
            mismatches: self.mismatches,
            rep_matches: self.rep_matches,
            n_count: self.n_count,
            query_num_insert: self.query_num_insert,
            query_base_insert: self.query_base_insert,
            reference_num_insert: self.reference_num_insert,
            reference_base_insert: self.reference_base_insert,
            strands: self.strands,
            query_name: self.query_name,
            query_size: self.query_size,
            query_start: self.query_start,
            query_end: self.query_end,
            reference_name: self.reference_name,
            reference_size: self.reference_size,
            reference_start: self.reference_start,
            reference_end: self.reference_end,
            block_sizes: blocks.block_sizes,
            query_starts: blocks.query_starts,
            reference_starts: blocks.reference_starts,
            seq: blocks.seq,
        }
    }
}

/// The block lists and PSLx sequences parsed for one streamed record.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct OwnedBlocks {
    pub block_sizes: BlockList,
    pub query_starts: BlockList,
    pub reference_starts: BlockList,
    pub seq: Option<(Vec<u8>, Vec<u8>)>,
}

/// Next item encountered while streaming PSL text.
///
/// Non-record lines (the `psLayout` header block, `track`/`browser` lines, other
/// comments) are surfaced as [`StreamItem::MetaLine`] so callers like `sort` and
/// `merge` can preserve them; blank lines are skipped.
#[derive(Debug, Clone)]
pub enum StreamItem {
    MetaLine(Vec<u8>),
    Header(OwnedPslHeader),
}

/// Pending block-list locations for the line most recently returned by
/// [`StreamingReader::next_item`], stored as ranges into `buf`.
struct Pending {
    line_offset: usize,
    block_count: usize,
    block_sizes: Range<usize>,
    query_starts: Range<usize>,
    reference_starts: Range<usize>,
    seq: Option<(Range<usize>, Range<usize>)>,
}

/// Streaming reader over any [`BufRead`], suitable for stdin/pipes.
///
/// Parses one record per line with low, bounded memory. Because PSL blocks live
/// on the same line as the header, the header-first path locates the block-list
/// columns but defers parsing them until [`read_blocks`](Self::read_blocks).
pub struct StreamingReader<R: BufRead> {
    reader: R,
    buf: Vec<u8>,
    offset: usize,
    pending: Option<Pending>,
}

impl<R: BufRead> StreamingReader<R> {
    /// Creates a streaming reader over `reader`.
    pub fn new(reader: R) -> Self {
        StreamingReader {
            reader,
            buf: Vec::with_capacity(8 * 1024),
            offset: 0,
            pending: None,
        }
    }

    /// Pulls the next complete record. Returns `Ok(None)` at EOF.
    pub fn next_record(&mut self) -> Result<Option<OwnedPsl>, PslError> {
        let Some(header) = self.next_header()? else {
            return Ok(None);
        };
        let blocks = self.read_blocks()?;
        Ok(Some(header.into_psl(blocks)))
    }

    /// Pulls the next record header, deferring its block lists. After a `Some`,
    /// call [`read_blocks`](Self::read_blocks) or [`skip_blocks`](Self::skip_blocks)
    /// before requesting another item.
    pub fn next_header(&mut self) -> Result<Option<OwnedPslHeader>, PslError> {
        while let Some(item) = self.next_item()? {
            if let StreamItem::Header(header) = item {
                return Ok(Some(header));
            }
        }
        Ok(None)
    }

    /// Pulls the next metadata line or record header.
    pub fn next_item(&mut self) -> Result<Option<StreamItem>, PslError> {
        // A new line read invalidates any pending (same-line) block ranges.
        self.pending = None;
        loop {
            let Some((line_offset, line)) = self.read_trimmed_line()? else {
                return Ok(None);
            };
            if is_blank(line) {
                continue;
            }
            if !looks_like_record(line) {
                return Ok(Some(StreamItem::MetaLine(line.to_vec())));
            }
            let header = parse_stream_record(line, line_offset)?;
            let owned = build_header(line, line_offset, &header);
            self.pending = Some(Pending {
                line_offset,
                block_count: header.block_count,
                block_sizes: header.block_sizes,
                query_starts: header.query_starts,
                reference_starts: header.reference_starts,
                seq: header.seq,
            });
            return Ok(Some(StreamItem::Header(owned)));
        }
    }

    /// Parses the block lists (and PSLx sequences) of the most recent header.
    pub fn read_blocks(&mut self) -> Result<OwnedBlocks, PslError> {
        let pending = self.pending.take().ok_or_else(|| PslError::Format {
            offset: self.offset,
            msg: "read_blocks called without a pending record header".into(),
        })?;

        let mut block_sizes = BlockList::new();
        let mut query_starts = BlockList::new();
        let mut reference_starts = BlockList::new();

        let n_sizes = parse_coord_list(
            &self.buf[pending.block_sizes.clone()],
            pending.line_offset + pending.block_sizes.start,
            "blockSizes",
            |v| block_sizes.push(v),
        )?;
        let n_query = parse_coord_list(
            &self.buf[pending.query_starts.clone()],
            pending.line_offset + pending.query_starts.start,
            "qStarts",
            |v| query_starts.push(v),
        )?;
        let n_reference = parse_coord_list(
            &self.buf[pending.reference_starts.clone()],
            pending.line_offset + pending.reference_starts.start,
            "tStarts",
            |v| reference_starts.push(v),
        )?;

        if n_sizes != pending.block_count
            || n_query != pending.block_count
            || n_reference != pending.block_count
        {
            return Err(PslError::Format {
                offset: pending.line_offset,
                msg: format!(
                    "blockCount ({}) disagrees with list lengths \
                     (blockSizes={n_sizes}, qStarts={n_query}, tStarts={n_reference})",
                    pending.block_count
                )
                .into(),
            });
        }

        let seq = pending
            .seq
            .map(|(q, t)| (self.buf[q].to_vec(), self.buf[t].to_vec()));

        Ok(OwnedBlocks {
            block_sizes,
            query_starts,
            reference_starts,
            seq,
        })
    }

    /// Discards the block lists of the most recent header without parsing them.
    pub fn skip_blocks(&mut self) {
        self.pending = None;
    }

    /// Reads one line, returning its starting byte offset and trimmed content.
    fn read_trimmed_line(&mut self) -> Result<Option<(usize, &[u8])>, PslError> {
        self.buf.clear();
        let start = self.offset;
        let n = self.reader.read_until(b'\n', &mut self.buf)?;
        if n == 0 {
            return Ok(None);
        }
        self.offset += n;
        if let Some(b'\n') = self.buf.last() {
            self.buf.pop();
        }
        if let Some(b'\r') = self.buf.last() {
            self.buf.pop();
        }
        Ok(Some((start, self.buf.as_slice())))
    }
}

/// Builds an [`OwnedPslHeader`] from a parsed [`StreamHeader`], copying names out
/// of the line buffer.
fn build_header(line: &[u8], line_offset: usize, header: &StreamHeader) -> OwnedPslHeader {
    OwnedPslHeader {
        offset: line_offset,
        matches: header.matches,
        mismatches: header.mismatches,
        rep_matches: header.rep_matches,
        n_count: header.n_count,
        query_num_insert: header.query_num_insert,
        query_base_insert: header.query_base_insert,
        reference_num_insert: header.reference_num_insert,
        reference_base_insert: header.reference_base_insert,
        strands: header.strands,
        query_name: line[header.query_name.clone()].to_vec(),
        query_size: header.query_size,
        query_start: header.query_start,
        query_end: header.query_end,
        reference_name: line[header.reference_name.clone()].to_vec(),
        reference_size: header.reference_size,
        reference_start: header.reference_start,
        reference_end: header.reference_end,
        block_count: header.block_count,
    }
}

impl StreamingReader<Box<dyn BufRead>> {
    /// Opens a path for streaming, decompressing gzip when the feature is on.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, PslError> {
        let path = path.as_ref();
        if is_gz_path(path) {
            #[cfg(feature = "gzip")]
            {
                let file = std::fs::File::open(path)?;
                let reader = BufReader::with_capacity(INPUT_BUFFER_CAPACITY, file);
                let decoder = MultiGzDecoder::new(reader);
                return Ok(StreamingReader::new(Box::new(BufReader::with_capacity(
                    INPUT_BUFFER_CAPACITY,
                    decoder,
                ))));
            }
            #[cfg(not(feature = "gzip"))]
            {
                return Err(gzip_feature_error());
            }
        }

        let file = std::fs::File::open(path)?;
        Ok(StreamingReader::new(Box::new(BufReader::with_capacity(
            INPUT_BUFFER_CAPACITY,
            file,
        ))))
    }
}
