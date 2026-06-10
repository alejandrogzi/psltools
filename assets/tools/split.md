# psltools split

Split a PSL into several files. Exactly one mode must be chosen.

```
psltools split -c IN.psl -p PREFIX (--by reference|query | --chunks N | --max-records N | --max-bytes N) [-G]
```

| Flag              | Meaning                                                                 |
|-------------------|-------------------------------------------------------------------------|
| `-c, --psl`       | Input PSL (default stdin).                                              |
| `-p, --out-prefix`| Output prefix; files are `PREFIX.<key>.psl` (`.gz` with `-G`).          |
| `--by`            | One file per `reference` or `query` name (`PREFIX.<name>.psl`).         |
| `--chunks N`      | Round-robin into `N` files (`PREFIX.0000.psl` …), balanced by count.    |
| `--max-records N` | Start a new file every `N` records.                                     |
| `--max-bytes N`   | Start a new file when it would exceed `N` uncompressed bytes.           |
| `-G, --gzip`      | Compress each output.                                                    |

```bash
psltools split -c in.psl --chunks 64 -p shards/part     # parallel fan-out
psltools split -c in.psl --by reference -p perchrom/
psltools split -c in.psl --max-records 100000 -p batch
```
