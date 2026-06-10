// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::ops::Range;
use std::sync::Arc;

use crate::PslError;

#[cfg(feature = "mmap")]
use memmap2::Mmap;

/// Shared byte storage for zero-copy parsing with lifetime safety.
///
/// Wraps either a memory-mapped file or owned bytes in an `Arc`, allowing
/// multiple zero-copy references to share the same underlying storage safely
/// without lifetime concerns.
///
/// # Variants
///
/// * `Mmap` - Memory-mapped file (requires the `mmap` feature)
/// * `Owned` - Owned buffer behind an `Arc`
#[derive(Debug, Clone)]
pub enum SharedBytes {
    #[cfg(feature = "mmap")]
    Mmap(Arc<Mmap>),
    Owned(Arc<Vec<u8>>),
}

impl SharedBytes {
    /// Returns the entire buffer as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            #[cfg(feature = "mmap")]
            SharedBytes::Mmap(m) => &m[..],
            SharedBytes::Owned(buf) => buf.as_slice(),
        }
    }

    /// Creates a `SharedBytes` from a memory map (requires the `mmap` feature).
    #[cfg(feature = "mmap")]
    pub(crate) fn from_mmap(mmap: Mmap) -> Self {
        SharedBytes::Mmap(Arc::new(mmap))
    }

    /// Creates a `SharedBytes` from an owned `Vec<u8>`.
    pub fn from_owned(data: Vec<u8>) -> Self {
        SharedBytes::Owned(Arc::new(data))
    }
}

/// A lightweight, clonable view into a subsection of a `SharedBytes` buffer.
///
/// Holds a reference-counted pointer to the underlying storage plus a `Range`
/// describing the slice it represents, so cloning is cheap and never copies the
/// underlying bytes.
///
/// # Examples
///
/// ```
/// use psltools::io::storage::{ByteSlice, SharedBytes};
///
/// let storage = SharedBytes::from_owned(b"hello world".to_vec());
/// let slice = ByteSlice::new(storage, 6..11);
/// assert_eq!(slice.as_bytes(), b"world");
/// assert_eq!(slice.as_str(), Some("world"));
/// ```
#[derive(Debug, Clone)]
pub struct ByteSlice {
    storage: SharedBytes,
    range: Range<usize>,
}

impl ByteSlice {
    /// Creates a new `ByteSlice` over `range` within `storage`.
    pub fn new(storage: SharedBytes, range: Range<usize>) -> Self {
        ByteSlice { storage, range }
    }

    /// Returns the bytes represented by this slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.storage.as_slice()[self.range.clone()]
    }

    /// Attempts to interpret the slice as UTF-8, returning `None` if invalid.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }

    /// Returns the number of bytes in the slice.
    pub fn len(&self) -> usize {
        self.range.len()
    }

    /// Returns `true` if the slice is empty.
    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }
}

/// Checks if a file path has a `.gz` extension.
pub fn is_gz_path(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| ext == "gz")
}

/// Constructs a [`PslError::Unsupported`] for when the `gzip` feature is needed
/// but was not compiled in.
#[cfg_attr(feature = "gzip", allow(dead_code))]
pub fn gzip_feature_error() -> PslError {
    PslError::Unsupported {
        msg: "gzip support disabled; enable the `gzip` feature".into(),
    }
}
