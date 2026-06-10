# psltools sort

Sort PSL records by a primary key. Sorts fully in memory when under the memory
budget (parallel sort), otherwise spills sorted runs to temporary files and does
a bounded k-way merge — so multi-GB inputs sort in bounded memory.

```
psltools sort [-c IN.psl] [-o OUT.psl] [-S KEY] [-M GB] [-I INDEX] [-G]
```

| Flag                | Default     | Meaning                                                       |
|---------------------|-------------|---------------------------------------------------------------|
| `-c, --psl`         | stdin       | Input PSL.                                                    |
| `-o, --out-psl`     | stdout      | Output PSL.                                                   |
| `-S, --sort-by`     | `reference` | `reference` (name, start), `query` (name, start), `score` (desc), `size` (reference span, desc). |
| `-M, --max-gb`      | `16`        | Memory budget before spilling sorted runs to disk.           |
| `-I, --out-index`   | —           | Write `hexoffset<TAB>key` at each primary-key group boundary. |
| `-G, --gzip`        | off         | Compress output (incompatible with `--out-index`).           |

Non-record lines (`psLayout` header, comments) are preserved and emitted first.

```bash
psltools sort -c in.psl -S score -o by_score.psl
psltools sort -c huge.psl --max-gb 2 -o sorted.psl       # out-of-core
psltools sort -c in.psl -S reference -I sorted.idx -o sorted.psl
```
