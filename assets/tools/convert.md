# psltools convert

Convert PSL records to BED over the **reference** sequence. Each record is mapped
to a `genepred::GenePred` (its blocks become exons, with `sizeMul` and the
negative-strand rules applied so exons are forward-strand and ascending), which
`genepred` then renders to the requested BED width.

```
psltools convert [-c "IN.psl ..."] [-o OUT.bed] [--type N] [-G]
```

| Flag         | Default | Meaning                                          |
|--------------|---------|--------------------------------------------------|
| `-c, --psl`  | stdin   | Input PSL file(s).                               |
| `-o, --out`  | stdout  | Output BED.                                      |
| `--type N`   | `12`    | BED layout: `3`, `4`, `5`, `6`, `8`, `9`, or `12`. |
| `-G, --gzip` | off     | Compress output.                                 |

Field mapping: `chrom`/`chromStart`/`chromEnd` = reference; `name` = query;
`strand` = query strand; `thickStart`/`thickEnd` default to the reference span;
`itemRgb` = `0,0,0`; blocks come from the per-block reference intervals. The BED
`score` column is `0` (PSL carries no score; use `psltools score` for that).

```bash
psltools convert -c in.psl > in.bed              # BED12
psltools convert -c in.psl --type 6 -o in.bed6   # BED6
```
