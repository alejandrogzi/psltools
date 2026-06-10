# psltools check

Validate the structural invariants of every record (UCSC `pslCheck`-style) and
report violations as `file:record: query -> reference: message` on stderr.

```
psltools check [-c "IN.psl ..."] [--warn-only]
```

| Flag          | Meaning                                                      |
|---------------|--------------------------------------------------------------|
| `-c, --psl`   | Input PSL (default stdin).                                   |
| `--warn-only` | Report violations but always exit 0.                         |

Checks include: block list lengths agree with `blockCount`; coordinate bounds
(`start <= end <= size`); block monotonicity; and span consistency
(`qEnd-qStart == Σ blockSizes + qBaseInsert`, and the reference side with
`sizeMul`).

Exit code is `1` if any record fails (unless `--warn-only`), so it composes in
pipelines and CI.

```bash
psltools check -c in.psl && echo OK
psltools filter -c in.psl --min-score 1000 | psltools check --warn-only
```
