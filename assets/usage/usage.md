# psltools — usage

```
psltools <command> [options]
```

`psltools` reads, writes, and manipulates [PSL](https://genome.ucsc.edu/FAQ/FAQformat.html#format2)
files (BLAT / lastz output). It is the PSL sibling of
[`chaintools`](https://github.com/alejandrogzi/chaintools): same zero-copy
backbone, same reader ergonomics, same CLI shape — specialized for the one
structural advantage PSL has over chain, **one record per line**, which makes
chunked parallel parsing, indexing, and streaming simple and fast.

- **Zero-copy parsing** — names and PSLx sequences are views into a shared
  mmap/owned buffer; the three block coordinate lists live in one
  structure-of-arrays arena. The whole-file reader allocates nothing per record.
- **Parallel & streaming** — line-aligned chunked parallel parsing, plus a
  low-memory streaming reader for stdin/pipes and out-of-core sort/merge.
- **Kent-exact scoring** — `pslScore` / `pslCalcMilliBad` / percent identity are
  transcribed bit-for-bit from `kent/src/lib/psl.c` (including `repMatch >> 1`,
  the derived `sizeMul`, and `i32` integer semantics).
- **Correct on the hard corners** — negative-strand `qStarts`/`tStarts`,
  translated (two-char) strands, PSLx sequence columns, and the optional
  `psLayout` header are handled from day one.

> **Naming.** The Rust API uses `reference_*` / `query_*` (matching `chaintools`
> and `make_lastz_chains`). PSL's on-disk `tName/tStart/...` map to
> `reference_*`; `qName/qStart/...` map to `query_*`.

## Library

```toml
[dependencies]
psltools = "0.0.1"
```

```rust
use psltools::Reader;

// Whole file, memory-mapped by default.
let reader = Reader::<psltools::Psl>::from_path("example.psl")?;
for psl in reader.records() {
    println!(
        "{} -> {}  score={}  identity={:.1}%",
        psl.query_name_str(),
        psl.reference_name_str(),
        psl.score(),
        psl.percent_id(false),
    );
}
# Ok::<(), psltools::PslError>(())
```

```rust
// Parallel parse (feature = "parallel").
let reader = psltools::Reader::<psltools::Psl>::from_path_parallel("huge.psl")?;

// Low-memory streaming (feature = "gzip" handles .gz transparently).
let mut s = psltools::StreamingReader::from_path("large.psl.gz")?;
while let Some(rec) = s.next_record()? {
    // process rec without holding the whole file
}
# Ok::<(), psltools::PslError>(())
```

### Feature flags

| Feature     | Default | Purpose                                                        |
|-------------|:-------:|----------------------------------------------------------------|
| `mmap`      |   ✅    | Memory-map inputs for zero-copy parsing.                       |
| `cli`       |   ✅    | Build the `psltools` binary (implies `parallel`).              |
| `gzip`      |         | Transparent `.gz` read/write.                                  |
| `parallel`  |   ✅¹   | Multi-threaded parsing and `par_records`.                      |
| `index`     |         | Record-offset and interval indexes.                            |
| `serde`     |         | `Serialize`/`Deserialize` on the owned types.                  |
| `bigcoords` |         | Widen `Coord` to `u64` for assemblies > 4.29 Gbp per scaffold. |

¹ enabled transitively by `cli`.




## Global options

| Flag             | Default        | Meaning                                              |
|------------------|----------------|------------------------------------------------------|
| `-t, --threads`  | logical CPUs   | Size of the rayon thread pool.                       |
| `-L, --level`    | `info`         | Log level: `off`, `error`, `warn`, `info`, `debug`, `trace`. |

## I/O conventions

- **Data on stdout, logs on stderr.** Logging is verbose (`info`) by default and
  always goes to stderr, so it never corrupts a piped PSL stream. Silence it with
  `-L off`.
- **Inputs default to stdin**, outputs default to stdout. Most commands accept
  one or more `-c/--psl PATH` inputs; `sort`/`split` take a single `-c/--psl`.
- **`.gz` is transparent on read** (with the `gzip` feature). `-G/--gzip`
  compresses output where supported.

## Commands

| Command  | Summary                                                                  |
|----------|--------------------------------------------------------------------------|
| `sort`   | Sort by reference (default), query, score, or size; external-merge spill.|
| `merge`  | Concatenate, or k-way merge pre-sorted inputs; optional dedup.           |
| `split`  | One file per name, N round-robin chunks, or by max records/bytes.        |
| `filter` | AND-combined predicates over score, identity, names, region, gaps, …     |
| `score`  | Report `pslScore` / `milliBad` / `percentId` as TSV.                     |
| `swap`   | Swap query and reference (UCSC `pslSwap`).                               |
| `check`  | Validate structural invariants; non-zero exit on failure.               |
| `stats`  | Summarize counts, scores, identity histogram, per-reference coverage.    |
| `convert`| Convert to BED (`--type 3/4/5/6/8/9/12`, default 12) via `genepred`.      |

See `assets/tools/<command>.md` for per-command flags and examples.

## Notes on PSL semantics

- The Rust/CLI naming is `reference_*` (PSL `t*`) and `query_*` (PSL `q*`).
- Coordinates are 0-based, half-open. `qStarts`/`tStarts` are in
  reverse-complement coordinates when the corresponding strand is `-`; the
  library exposes forward-strand helpers.
- **Protein/`sizeMul` is derived** from each record (`pslIsProtein`), never a
  flag. The only identity knob is `--mrna`.

## Example

```bash
# Keep high-scoring, high-identity alignments overlapping a region, then sort.
psltools filter -c in.psl --min-score 4000 --min-identity 95 --region chr2:1000000-2000000 \
  | psltools sort -S score -o top.psl

# Split for parallel fan-out (e.g. make_lastz_chains).
psltools split -c in.psl --chunks 64 -p shards/part

# Validate; non-zero exit if any record fails.
psltools check -c in.psl
```
