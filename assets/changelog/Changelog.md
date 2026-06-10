# Changelog

All notable changes to `psltools` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project adheres to
[Semantic Versioning](https://semver.org/).

## [0.0.1] — unreleased

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
