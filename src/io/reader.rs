// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

#[cfg(feature = "gzip")]
use std::io::BufReader;
#[cfg(any(feature = "gzip", not(feature = "mmap")))]
use std::io::Read;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

#[cfg(not(feature = "gzip"))]
use crate::io::storage::gzip_feature_error;
use crate::io::storage::{ByteSlice, SharedBytes, is_gz_path};
use crate::model::block::{BlockSlice, Blocks};
use crate::model::psl::{Psl, PslxSeq};
use crate::{PslError, parser::parse_psl_sequential};

#[cfg(feature = "parallel")]
use crate::parser::parse_psl_parallel;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[cfg(feature = "gzip")]
use flate2::read::MultiGzDecoder;

#[cfg(feature = "mmap")]
use memmap2::MmapOptions;

/// Reader-wide options.
///
/// `mrna` feeds the identity calculations ([`Psl::milli_bad`] /
/// [`Psl::percent_id`]). Protein/`sizeMul` is intrinsic to each record and
/// therefore deliberately absent here.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReaderOptions {
    pub mrna: bool,
}

/// Whole-file reader for PSL/PSLx files.
///
/// The generic parameter enables the `Reader::<Psl>::from_path(..)` shape. All
/// records share one [`SharedBytes`] buffer (names and PSLx sequences are
/// zero-copy views into it) and one [`Blocks`] arena, so the common path
/// allocates nothing per record.
#[derive(Debug)]
pub struct Reader<T = Psl> {
    _bytes: SharedBytes,
    _blocks: Arc<Blocks>,
    records: Vec<Psl>,
    options: ReaderOptions,
    _marker: PhantomData<T>,
}

impl Reader<Psl> {
    /// Loads a PSL file from a path, using mmap when available and transparently
    /// decompressing `.gz` inputs (with the `gzip` feature).
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, PslError> {
        Self::from_path_with(path, ReaderOptions::default())
    }

    /// Like [`from_path`](Self::from_path) but with explicit [`ReaderOptions`].
    pub fn from_path_with<P: AsRef<Path>>(
        path: P,
        options: ReaderOptions,
    ) -> Result<Self, PslError> {
        let path = path.as_ref();
        if is_gz_path(path) {
            #[cfg(feature = "gzip")]
            {
                let buffer = read_gz(path)?;
                return Self::build(
                    SharedBytes::from_owned(buffer),
                    ParseStrategy::Sequential,
                    options,
                );
            }
            #[cfg(not(feature = "gzip"))]
            {
                return Err(gzip_feature_error());
            }
        }

        #[cfg(feature = "mmap")]
        {
            let file = std::fs::File::open(path)?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            Self::build(
                SharedBytes::from_mmap(mmap),
                ParseStrategy::Sequential,
                options,
            )
        }
        #[cfg(not(feature = "mmap"))]
        {
            let mut data = Vec::new();
            std::fs::File::open(path)?.read_to_end(&mut data)?;
            Self::build(
                SharedBytes::from_owned(data),
                ParseStrategy::Sequential,
                options,
            )
        }
    }

    /// Loads a PSL file using memory mapping (requires the `mmap` feature).
    #[cfg(feature = "mmap")]
    pub fn from_mmap<P: AsRef<Path>>(path: P) -> Result<Self, PslError> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        Self::build(
            SharedBytes::from_mmap(mmap),
            ParseStrategy::Sequential,
            ReaderOptions::default(),
        )
    }

    /// Like [`from_path`](Self::from_path) but always parses in parallel
    /// (requires the `parallel` feature).
    #[cfg(feature = "parallel")]
    pub fn from_path_parallel<P: AsRef<Path>>(path: P) -> Result<Self, PslError> {
        let path = path.as_ref();
        if is_gz_path(path) {
            #[cfg(feature = "gzip")]
            {
                let buffer = read_gz(path)?;
                return Self::build(
                    SharedBytes::from_owned(buffer),
                    ParseStrategy::Parallel,
                    ReaderOptions::default(),
                );
            }
            #[cfg(not(feature = "gzip"))]
            {
                return Err(gzip_feature_error());
            }
        }

        #[cfg(feature = "mmap")]
        {
            let file = std::fs::File::open(path)?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            Self::build(
                SharedBytes::from_mmap(mmap),
                ParseStrategy::Parallel,
                ReaderOptions::default(),
            )
        }
        #[cfg(not(feature = "mmap"))]
        {
            let mut data = Vec::new();
            std::fs::File::open(path)?.read_to_end(&mut data)?;
            Self::build(
                SharedBytes::from_owned(data),
                ParseStrategy::Parallel,
                ReaderOptions::default(),
            )
        }
    }

    /// Constructs a reader from an owned buffer (no mmap), parsing sequentially.
    pub fn from_owned_bytes(data: Vec<u8>) -> Result<Self, PslError> {
        Self::build(
            SharedBytes::from_owned(data),
            ParseStrategy::Sequential,
            ReaderOptions::default(),
        )
    }

    /// Constructs a reader from an owned buffer using parallel parsing (requires
    /// the `parallel` feature).
    #[cfg(feature = "parallel")]
    pub fn from_owned_bytes_parallel(data: Vec<u8>) -> Result<Self, PslError> {
        Self::build(
            SharedBytes::from_owned(data),
            ParseStrategy::Parallel,
            ReaderOptions::default(),
        )
    }

    fn build(
        bytes: SharedBytes,
        strategy: ParseStrategy,
        options: ReaderOptions,
    ) -> Result<Self, PslError> {
        let buf = bytes.as_slice();
        let (metas, blocks) = match strategy {
            ParseStrategy::Sequential => parse_psl_sequential(buf)?,
            #[cfg(feature = "parallel")]
            ParseStrategy::Parallel => parse_psl_parallel(buf)?,
        };
        let blocks_arc = Arc::new(blocks);
        let records = metas
            .into_iter()
            .map(|meta| Psl {
                matches: meta.matches,
                mismatches: meta.mismatches,
                rep_matches: meta.rep_matches,
                n_count: meta.n_count,
                query_num_insert: meta.query_num_insert,
                query_base_insert: meta.query_base_insert,
                reference_num_insert: meta.reference_num_insert,
                reference_base_insert: meta.reference_base_insert,
                query_size: meta.query_size,
                query_start: meta.query_start,
                query_end: meta.query_end,
                reference_size: meta.reference_size,
                reference_start: meta.reference_start,
                reference_end: meta.reference_end,
                strands: meta.strands,
                query_name: ByteSlice::new(bytes.clone(), meta.query_name),
                reference_name: ByteSlice::new(bytes.clone(), meta.reference_name),
                blocks: BlockSlice::new(blocks_arc.clone(), meta.blocks),
                seq: meta.seq.map(|(query, reference)| PslxSeq {
                    query_seq: ByteSlice::new(bytes.clone(), query),
                    reference_seq: ByteSlice::new(bytes.clone(), reference),
                }),
            })
            .collect();

        Ok(Reader {
            _bytes: bytes,
            _blocks: blocks_arc,
            records,
            options,
            _marker: PhantomData,
        })
    }

    /// Iterates over the parsed records.
    pub fn records(&self) -> impl Iterator<Item = &Psl> {
        self.records.iter()
    }

    /// Returns the records as a slice.
    pub fn as_slice(&self) -> &[Psl] {
        &self.records
    }

    /// A parallel iterator over the records (requires the `parallel` feature).
    #[cfg(feature = "parallel")]
    pub fn par_records(&self) -> impl ParallelIterator<Item = &Psl> {
        self.records.par_iter()
    }

    /// The reader-wide options.
    pub fn options(&self) -> ReaderOptions {
        self.options
    }

    /// Number of parsed records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// `true` when there are no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(feature = "gzip")]
fn read_gz(path: &Path) -> Result<Vec<u8>, PslError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut decoder = MultiGzDecoder::new(reader);
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)?;
    Ok(buffer)
}

#[derive(Clone, Copy)]
enum ParseStrategy {
    Sequential,
    #[cfg(feature = "parallel")]
    Parallel,
}
