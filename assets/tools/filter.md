# psltools filter

Keep records matching all the given predicates (AND-combined). `--invert` negates
the combined result. Header-only predicates reject records before their block
lists are even parsed.

```
psltools filter [-c "IN.psl ..."] [-o OUT.psl] [predicates...] [--invert] [--mrna] [-G]
```

| Flag                                   | Predicate                                              |
|----------------------------------------|--------------------------------------------------------|
| `--min-match N`                        | `matches >= N`                                         |
| `--min-score N`                        | `pslScore >= N`                                        |
| `--min-identity PCT`                   | percent identity `>= PCT` (honors `--mrna`)            |
| `--min-query-size` / `--max-query-size`| query size bounds                                      |
| `--min-ref-size` / `--max-ref-size`    | reference size bounds                                  |
| `--strand +`\|`-`                      | query strand                                           |
| `--query-name NAME` (repeatable)       | keep only these query names                            |
| `--ref-name NAME` (repeatable)         | keep only these reference names                        |
| `--query-name-exclude` / `--ref-name-exclude` | drop these names                               |
| `--region chrN:start-end`              | reference overlaps the region                          |
| `--min-blocks N`                       | at least `N` blocks                                    |
| `--max-query-gaps N` / `--max-ref-gaps N` | `qNumInsert` / `tNumInsert` `<= N`                  |
| `--drop-self`                          | drop records where query name == reference name        |
| `--invert`                             | negate the combined predicate                          |
| `--mrna`                               | treat alignments as mRNA for identity                  |
| `-G, --gzip`                           | compress output                                        |

```bash
psltools filter -c in.psl --min-score 4000 --min-identity 96 --region chr2:1000000-2000000
psltools filter -c in.psl --drop-self --max-query-gaps 5 -o clean.psl
```
