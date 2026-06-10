# psltools swap

Swap query and reference, producing a valid PSL with the alignment preserved.
Transcribed from UCSC `pslSwap`: translated records swap their strand chars and
block lists; untranslated minus-strand records are reverse-complemented so the
new target stays on `+` (unless `--no-rc`). Swapping twice is the identity.

```
psltools swap [-c "IN.psl ..."] [-o OUT.psl] [--no-rc] [-G]
```

| Flag           | Meaning                                                                 |
|----------------|-------------------------------------------------------------------------|
| `-c, --psl`    | Input PSL (default stdin).                                              |
| `-o, --out-psl`| Output PSL (default stdout).                                           |
| `--no-rc`      | Don't reverse-complement untranslated minus records; make `tStrand` explicit. |
| `-G, --gzip`   | Compress output.                                                        |

```bash
psltools swap -c query_vs_ref.psl -o ref_vs_query.psl
```
