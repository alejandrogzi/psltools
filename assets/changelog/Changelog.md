# Changelog

All notable changes to `psltools` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project adheres to
[Semantic Versioning](https://semver.org/).

## [0.0.2] — 2026-06-11

### Added

- **`merge`:** New `--file`/`-f` flag accepts a file listing one input PSL path
  per line, enabling merges of very large input sets without overflowing the
  command line.
- **`split`:** New `--file`/`-f` flag accepts a file listing one input PSL path
  per line, providing the same large-input convenience added to `merge`.
- **`split`:** The `--psl`/`-c` argument now accepts multiple paths (space-
  separated) instead of a single file, allowing batch splits from several
  inputs in one invocation.
- **`split`:** The `--out-prefix`/`-p` argument is now optional. When omitted,
  output files are named `<key>.psl` (or `<key>.psl.gz`) instead of requiring
  a prefix.

### Changed

- **`merge`:** The positional `[INPUTS...]` argument is now optional when
  `--file` is provided; the two options are mutually exclusive.
- **README:** Corrected the project tagline from ".chain" to ".psl" to
  accurately reflect that the library operates on PSL-format files.
- **Logo:** Redesigned the SVG logo with a larger 200×200 canvas and a
  circular border enclosing a refined two-link chain motif. Added
  accessibility metadata (`role="img"`, `<title>`, `<desc>`), updated the
  colour palette for improved contrast, and introduced rounded link ends with
  accent dots.

## [0.0.1] — 2026-06-10

Initial release.

### Library

- Zero-copy `Reader<Psl>` over mmap/owned buffers; names and PSLx sequences are
  `ByteSlice` views, block lists live in a shared structure-of-arrays arena.
- Sequential and line-aligned chunked **parallel** parsing; `par_records`.
- Low-memory `StreamingReader` with header-first filtering (`OwnedPsl`,
  `OwnedPslHeader`), transparent gzip.
- Canonical PSL/PSLx `PslWriter` / `write_psl` (itoa hot path), optional
  `psLayout` header.
- Kent-exact `psl_score` / `milli_bad` / `percent_id` (derived `sizeMul`).
- Operations: `swap` (UCSC `pslSwap`), `to_genepred`/`to_bed`/`to_bed12` (BED of
  any width via the `genepred` crate), `check`, region/coordinate helpers
  honoring the negative-strand and translated rules. `Strand` variants are
  `Forward`/`Reverse`.
- Optional record-offset `PslIndex` and `IntervalIndex` (`index` feature),
  `serde` derives on owned types, `bigcoords` (u64 coordinates).

### CLI

- `sort` (external-merge spill, `--out-index`), `merge` (k-way, `--dedup`),
  `split` (by name / chunks / max records / max bytes), `filter`, `score`,
  `swap`, `check` (non-zero exit on failure), `stats` (table or JSON),
  `convert` (`--type 3/4/5/6/8/9/12`).
- Verbose-by-default `info` logging on stderr; data on stdout; `-G/--gzip`.
