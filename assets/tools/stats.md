# psltools stats

Summarize a PSL: record count, total/mean/median `pslScore`, a percent-identity
histogram (deciles), the block-count distribution, and per-reference record
counts and covered bases.

```
psltools stats [-c "IN.psl ..."] [--mrna] [--json]
```

| Flag       | Meaning                                                  |
|------------|----------------------------------------------------------|
| `-c, --psl`| Input PSL (default stdin).                               |
| `--mrna`   | Treat alignments as mRNA for the identity histogram.     |
| `--json`   | Emit a single JSON object instead of a TSV-ish table.    |

```bash
psltools stats -c in.psl
psltools stats -c in.psl --json | jq .records
```
