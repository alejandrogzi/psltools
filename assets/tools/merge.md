# psltools merge

Combine several PSL files. With `--sorted-by`, performs an O(1)-memory streaming
k-way merge assuming the inputs are already sorted on that key; otherwise simply
concatenates them.

```
psltools merge [-c "A.psl B.psl ..."] [-o OUT.psl] [--sorted-by KEY] [--dedup] [--header] [-G]
```

| Flag             | Default | Meaning                                                            |
|------------------|---------|--------------------------------------------------------------------|
| `-c, --psl`      | stdin   | One or more input PSL files.                                       |
| `-o, --out-psl`  | stdout  | Output PSL.                                                        |
| `--sorted-by`    | —       | `reference` / `query` / `score` / `size`; enables streaming merge. |
| `--dedup`        | off     | Drop a record identical to the previously emitted one.            |
| `--header`       | off     | Emit a `psLayout v3` header once before the records.              |
| `-G, --gzip`     | off     | Compress output.                                                   |

`--dedup` removes adjacent duplicates, so it removes all duplicates when the
inputs are sorted (e.g. with `--sorted-by`).

```bash
psltools sort -c a.psl -S reference -o a.sorted.psl
psltools sort -c b.psl -S reference -o b.sorted.psl
psltools merge -c "a.sorted.psl b.sorted.psl" --sorted-by reference --dedup -o all.psl
```
