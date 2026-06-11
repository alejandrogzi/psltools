// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use psltools::{OwnedPsl, StreamingReader, write_psl};

use super::{CliError, OUTPUT_BUFFER_CAPACITY, ensure_gzip_available, ensure_inputs_exist};

const COPY_BUFFER_CAPACITY: usize = 1024 * 1024;

/// Split a PSL by sequence name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum By {
    Reference,
    Query,
}

/// Arguments for the `split` subcommand. Exactly one mode must be chosen.
#[derive(Debug, Args)]
pub struct SplitArgs {
    #[arg(
        short = 'c',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl file(s). If omitted, read from standard input.",
        value_delimiter = ' ',
        num_args = 1..,
    )]
    inputs: Option<Vec<PathBuf>>,

    #[arg(
        short = 'f',
        long = "file",
        value_name = "PATH",
        conflicts_with = "inputs",
        help = "Path to a file listing one input PSL path per line."
    )]
    file: Option<PathBuf>,

    #[arg(
        short = 'p',
        long = "out-prefix",
        value_name = "PREFIX",
        help = "Output filename prefix; outputs are PREFIX.<key>.psl. If omitted, outputs are <key>.psl."
    )]
    out_prefix: Option<String>,

    #[arg(short = 'G', long = "gzip", help = "Compress each output with gzip.")]
    gzip: bool,

    #[arg(
        long = "by",
        value_enum,
        help = "Split into one file per reference or query name."
    )]
    by: Option<By>,

    #[arg(
        long = "chunks",
        value_name = "N",
        help = "Split round-robin into N files."
    )]
    chunks: Option<usize>,

    #[arg(
        long = "max-records",
        value_name = "N",
        help = "Start a new file every N records."
    )]
    max_records: Option<u64>,

    #[arg(
        long = "max-bytes",
        value_name = "N",
        help = "Start a new file when it would exceed N uncompressed bytes."
    )]
    max_bytes: Option<u64>,
}

/// Output file, plain or gzip, with an explicit finish.
enum OutFile {
    Plain(BufWriter<File>),
    #[cfg(feature = "gzip")]
    Gz(Box<flate2::write::GzEncoder<BufWriter<File>>>),
}

impl Write for OutFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            OutFile::Plain(w) => w.write(buf),
            #[cfg(feature = "gzip")]
            OutFile::Gz(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            OutFile::Plain(w) => w.flush(),
            #[cfg(feature = "gzip")]
            OutFile::Gz(w) => w.flush(),
        }
    }
}

impl OutFile {
    fn create(path: &str, gzip: bool) -> io::Result<Self> {
        let file = File::create(path)?;
        let base = BufWriter::with_capacity(OUTPUT_BUFFER_CAPACITY, file);
        if gzip {
            #[cfg(feature = "gzip")]
            {
                return Ok(OutFile::Gz(Box::new(flate2::write::GzEncoder::new(
                    base,
                    flate2::Compression::fast(),
                ))));
            }
            #[cfg(not(feature = "gzip"))]
            {
                unreachable!("gzip guarded by ensure_gzip_available");
            }
        }
        Ok(OutFile::Plain(base))
    }

    fn finish(self) -> io::Result<()> {
        match self {
            OutFile::Plain(mut w) => w.flush(),
            #[cfg(feature = "gzip")]
            OutFile::Gz(w) => {
                let mut base = w.finish()?;
                base.flush()
            }
        }
    }
}

/// Runs the `split` subcommand.
pub fn run<R, W, E>(
    args: SplitArgs,
    stdin: &mut R,
    _stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    ensure_gzip_available(args.gzip)?;
    let inputs = collect_input_paths(&args)?;
    let input_refs: Vec<&std::path::Path> = inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;
    validate_mode(&args)?;

    let mut splitter = Splitter::new(&args);
    if inputs.is_empty() {
        let mut reader = StreamingReader::new(stdin);
        splitter.run(&mut reader)?;
    } else {
        for input in &inputs {
            let mut reader = StreamingReader::from_path(input)?;
            splitter.run(&mut reader)?;
        }
    }
    let (records, files) = splitter.finish()?;

    super::log_summary("split", &[("records", records), ("files", files)]);
    Ok(())
}

/// Collects input PSL file paths from --psl or from a file listing paths.
fn collect_input_paths(args: &SplitArgs) -> Result<Vec<PathBuf>, CliError> {
    if let Some(paths) = &args.inputs {
        return Ok(paths.clone());
    }

    let Some(list_path) = &args.file else {
        return Ok(Vec::new());
    };
    let file = File::open(list_path)?;
    let mut reader = BufReader::with_capacity(COPY_BUFFER_CAPACITY, file);
    let mut line = String::new();
    let mut paths = Vec::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            continue;
        }
        paths.push(PathBuf::from(trimmed.trim()));
    }

    if paths.is_empty() {
        return Err(CliError::Message(format!(
            "{} does not list any input PSL files",
            list_path.display()
        )));
    }

    Ok(paths)
}

fn validate_mode(args: &SplitArgs) -> Result<(), CliError> {
    let modes = [
        args.by.is_some(),
        args.chunks.is_some(),
        args.max_records.is_some(),
        args.max_bytes.is_some(),
    ];
    match modes.iter().filter(|&&set| set).count() {
        1 => Ok(()),
        0 => Err(CliError::Message(
            "choose a split mode: --by, --chunks, --max-records, or --max-bytes".to_owned(),
        )),
        _ => Err(CliError::Message(
            "split modes are mutually exclusive; choose exactly one".to_owned(),
        )),
    }
}

struct Splitter<'a> {
    args: &'a SplitArgs,
    by_name: HashMap<Vec<u8>, OutFile>,
    indexed: Vec<OutFile>, // for chunks: fixed; for max-*: grows
    current_records: u64,
    current_bytes: u64,
    records: u64,
    scratch: Vec<u8>,
}

impl<'a> Splitter<'a> {
    fn new(args: &'a SplitArgs) -> Self {
        Self {
            args,
            by_name: HashMap::new(),
            indexed: Vec::new(),
            current_records: 0,
            current_bytes: 0,
            records: 0,
            scratch: Vec::with_capacity(256),
        }
    }

    fn run<R: BufRead>(&mut self, reader: &mut StreamingReader<R>) -> Result<(), CliError> {
        while let Some(record) = reader.next_record()? {
            self.scratch.clear();
            write_psl(&mut self.scratch, &record).map_err(CliError::from)?;
            self.route(&record)?;
            self.records += 1;
        }
        Ok(())
    }

    fn route(&mut self, record: &OwnedPsl) -> Result<(), CliError> {
        if let Some(by) = self.args.by {
            let key = match by {
                By::Reference => &record.reference_name,
                By::Query => &record.query_name,
            };
            if !self.by_name.contains_key(key) {
                let path = self.path_for_key(key);
                self.by_name
                    .insert(key.clone(), OutFile::create(&path, self.args.gzip)?);
            }
            let writer = self.by_name.get_mut(key).expect("just inserted");
            writer.write_all(&self.scratch)?;
            return Ok(());
        }

        if let Some(n) = self.args.chunks {
            if self.indexed.is_empty() {
                for i in 0..n {
                    let path = self.path_for_index(i);
                    self.indexed.push(OutFile::create(&path, self.args.gzip)?);
                }
            }
            let idx = (self.records as usize) % n;
            self.indexed[idx].write_all(&self.scratch)?;
            return Ok(());
        }

        // max-records / max-bytes: roll over into a new file as needed.
        let record_len = self.scratch.len() as u64;
        let need_new = self.indexed.is_empty()
            || self
                .args
                .max_records
                .is_some_and(|max| self.current_records >= max)
            || self.args.max_bytes.is_some_and(|max| {
                self.current_records > 0 && self.current_bytes + record_len > max
            });
        if need_new {
            let idx = self.indexed.len();
            let path = self.path_for_index(idx);
            self.indexed.push(OutFile::create(&path, self.args.gzip)?);
            self.current_records = 0;
            self.current_bytes = 0;
        }
        let writer = self.indexed.last_mut().expect("file present");
        writer.write_all(&self.scratch)?;
        self.current_records += 1;
        self.current_bytes += record_len;
        Ok(())
    }

    fn path_for_key(&self, key: &[u8]) -> String {
        let name = String::from_utf8_lossy(key).replace(['/', '\\'], "_");
        let suffix = if self.args.gzip { ".psl.gz" } else { ".psl" };
        match &self.args.out_prefix {
            Some(prefix) => format!("{prefix}.{name}{suffix}"),
            None => format!("{name}{suffix}"),
        }
    }

    fn path_for_index(&self, idx: usize) -> String {
        let suffix = if self.args.gzip { ".psl.gz" } else { ".psl" };
        match &self.args.out_prefix {
            Some(prefix) => format!("{prefix}.{idx:04}{suffix}"),
            None => format!("{idx:04}{suffix}"),
        }
    }

    fn finish(self) -> Result<(u64, u64), CliError> {
        let mut files = 0u64;
        for (_, writer) in self.by_name {
            writer.finish()?;
            files += 1;
        }
        for writer in self.indexed {
            writer.finish()?;
            files += 1;
        }
        Ok((self.records, files))
    }
}
