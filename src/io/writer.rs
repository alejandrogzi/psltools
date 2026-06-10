// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::io::{self, Write};

use crate::model::Coord;
use crate::model::psl::{PslFlavor, PslRecord};

/// The canonical `psLayout version 3` header block (header line, two column-label
/// lines, and the dashes separator).
const PSLAYOUT_HEADER: &[u8] = b"psLayout version 3\n\n\
match\tmis- \trep. \tN's\tQ gap\tQ gap\tT gap\tT gap\tstrand\tQ        \tQ   \tQ    \tQ  \tT        \tT   \tT    \tT  \tblock\tblockSizes \tqStarts\t tStarts\n\
     \tmatch\tmatch\t   \tcount\tbases\tcount\tbases\t      \tname     \tsize\tstart\tend\tname     \tsize\tstart\tend\tcount\n\
---------------------------------------------------------------------------------------------------------------------------------------------------------------\n";

/// Writes the canonical `psLayout version 3` header block to `w`.
pub fn write_psl_header<W: Write>(w: &mut W) -> io::Result<()> {
    w.write_all(PSLAYOUT_HEADER)
}

/// Writes one record in canonical PSL/PSLx form (tab-separated, lists with a
/// trailing comma). The PSLx sequence columns are emitted when the record
/// carries them.
pub fn write_psl<W: Write, P: PslRecord>(w: &mut W, p: &P) -> io::Result<()> {
    write_record_inner(w, p, None)
}

/// A streaming PSL/PSLx writer.
///
/// Emits a `psLayout` header once before the first record when configured with
/// [`with_header`](PslWriter::with_header). The flavor (PSL vs PSLx) is chosen
/// per record from the presence of sequence columns unless forced via
/// [`flavor`](PslWriter::flavor).
pub struct PslWriter<W: Write> {
    inner: W,
    header: bool,
    force_flavor: Option<PslFlavor>,
    header_written: bool,
}

impl<W: Write> PslWriter<W> {
    /// Creates a writer over `w`.
    pub fn new(w: W) -> Self {
        PslWriter {
            inner: w,
            header: false,
            force_flavor: None,
            header_written: false,
        }
    }

    /// Emit the `psLayout` header once before the first record.
    pub fn with_header(mut self, yes: bool) -> Self {
        self.header = yes;
        self
    }

    /// Force a flavor (PSL strips sequence columns; PSLx always emits them).
    pub fn flavor(mut self, flavor: PslFlavor) -> Self {
        self.force_flavor = Some(flavor);
        self
    }

    /// Writes one record.
    pub fn write_record<P: PslRecord>(&mut self, p: &P) -> io::Result<()> {
        if self.header && !self.header_written {
            write_psl_header(&mut self.inner)?;
            self.header_written = true;
        }
        write_record_inner(&mut self.inner, p, self.force_flavor)
    }

    /// Mutable access to the underlying writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Flushes and returns the underlying writer.
    pub fn finish(mut self) -> io::Result<W> {
        self.inner.flush()?;
        Ok(self.inner)
    }
}

fn write_record_inner<W: Write, P: PslRecord>(
    w: &mut W,
    p: &P,
    force_flavor: Option<PslFlavor>,
) -> io::Result<()> {
    let mut int = itoa::Buffer::new();

    macro_rules! field_u {
        ($v:expr) => {{
            w.write_all(int.format($v).as_bytes())?;
            w.write_all(b"\t")?;
        }};
    }

    field_u!(p.matches());
    field_u!(p.mismatches());
    field_u!(p.rep_matches());
    field_u!(p.n_count());
    field_u!(p.query_num_insert());
    field_u!(p.query_base_insert());
    field_u!(p.reference_num_insert());
    field_u!(p.reference_base_insert());

    p.strands().render(w)?;
    w.write_all(b"\t")?;

    w.write_all(p.query_name())?;
    w.write_all(b"\t")?;
    field_u!(p.query_size());
    field_u!(p.query_start());
    field_u!(p.query_end());

    w.write_all(p.reference_name())?;
    w.write_all(b"\t")?;
    field_u!(p.reference_size());
    field_u!(p.reference_start());
    field_u!(p.reference_end());

    field_u!(p.block_count() as u64);

    write_coord_list(w, p.block_sizes())?;
    w.write_all(b"\t")?;
    write_coord_list(w, p.query_starts())?;
    w.write_all(b"\t")?;
    write_coord_list(w, p.reference_starts())?;

    let emit_seq = match force_flavor {
        Some(PslFlavor::Pslx) => true,
        Some(PslFlavor::Psl) => false,
        None => p.query_seq().is_some(),
    };
    if emit_seq {
        w.write_all(b"\t")?;
        write_seq_list(w, p.query_seq(), p.block_count())?;
        w.write_all(b"\t")?;
        write_seq_list(w, p.reference_seq(), p.block_count())?;
    }

    w.write_all(b"\n")
}

/// Writes a coordinate list as comma-separated values with a trailing comma.
fn write_coord_list<W: Write>(w: &mut W, values: &[Coord]) -> io::Result<()> {
    let mut int = itoa::Buffer::new();
    for &v in values {
        w.write_all(int.format(v).as_bytes())?;
        w.write_all(b",")?;
    }
    Ok(())
}

/// Writes a PSLx sequence list verbatim (it already carries its trailing comma).
/// When forcing PSLx output on a record without sequences, emits empty entries.
fn write_seq_list<W: Write>(w: &mut W, seq: Option<&[u8]>, block_count: usize) -> io::Result<()> {
    match seq {
        Some(raw) => w.write_all(raw),
        None => {
            for _ in 0..block_count {
                w.write_all(b",")?;
            }
            Ok(())
        }
    }
}
