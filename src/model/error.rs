// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::{borrow::Cow, fmt};

/// Error types for PSL parsing and processing.
///
/// Represents the conditions that can occur while parsing PSL/PSLx files,
/// performing I/O, validating invariants, or attempting unsupported features.
///
/// # Variants
///
/// * `Io` - I/O errors from file operations
/// * `Format` - Parsing errors carrying a byte offset and a message
/// * `Unsupported` - Feature or format not supported (e.g. gzip without the feature)
/// * `Invariant` - A record violated a structural invariant (used by `check`)
///
/// # Examples
///
/// ```
/// use psltools::PslError;
/// use std::io;
///
/// let io_err = PslError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
/// let format_err = PslError::Format {
///     offset: 100,
///     msg: "invalid psl record".into(),
/// };
/// ```
#[derive(Debug)]
pub enum PslError {
    Io(std::io::Error),
    Format {
        offset: usize,
        msg: Cow<'static, str>,
    },
    Unsupported {
        msg: Cow<'static, str>,
    },
    Invariant {
        line: usize,
        msg: Cow<'static, str>,
    },
}

impl From<std::io::Error> for PslError {
    /// Converts I/O errors into the [`PslError::Io`] variant.
    fn from(value: std::io::Error) -> Self {
        PslError::Io(value)
    }
}

impl fmt::Display for PslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PslError::Io(err) => write!(f, "I/O error: {err}"),
            PslError::Format { offset, msg } => {
                write!(f, "format error at byte {offset}: {msg}")
            }
            PslError::Unsupported { msg } => write!(f, "unsupported: {msg}"),
            PslError::Invariant { line, msg } => {
                write!(f, "invariant violation at line {line}: {msg}")
            }
        }
    }
}

impl std::error::Error for PslError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PslError::Io(err) => Some(err),
            _ => None,
        }
    }
}
