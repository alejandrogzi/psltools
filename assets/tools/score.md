# psltools score

PSL has no score column, so `score` is a reporting / sort-key tool. It prints
`queryName<TAB>referenceName<TAB>value` for a chosen metric.

```
psltools score [-c "IN.psl ..."] [-o OUT.tsv] [--metric M] [--mrna] [--sort-by-score] [-G]
```

| Flag                | Default | Meaning                                                   |
|---------------------|---------|-----------------------------------------------------------|
| `--metric`          | `score` | `score` (`pslScore`), `milli-bad`, or `percent-id`.       |
| `--mrna`            | off     | Treat alignments as mRNA (affects `milli-bad`/`percent-id`).|
| `--sort-by-score`   | off     | Emit rows ordered by computed `pslScore`, descending.     |
| `-G, --gzip`        | off     | Compress output.                                          |

All metrics reproduce UCSC `kent/src/lib/psl.c` integer semantics exactly; the
protein `sizeMul` is derived per record.

```bash
psltools score -c in.psl --metric percent-id --mrna
psltools score -c in.psl --sort-by-score | head
```
