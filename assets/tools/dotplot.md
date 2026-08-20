# psltools dotplot

Render PSL alignment blocks as a reference-vs-query dot plot. One pipeline:
omit `--query`/`--reference` to plot every sequence (whole-genome); pass them to
restrict either axis.

```
psltools dotplot [-p "IN.psl ..."] [-o OUT] [--format svg|png|dot]
                 [--query NAME] [--reference NAME]
                 [--width PX] [--height PX]
                 [--min-block-size BP] [--min-alignment-size BP]
                 [--strand +|-|both] [--bins N]
```

| Flag | Meaning |
|------|---------|
| `-p, --psl` | Input PSL (default stdin). |
| `-o, --output` | Output path (default stdout). |
| `--format` | `svg` (default), `png` (needs `--features png`), or `dot` (TSV). Inferred from `-o` if omitted (`.png` / `.tsv` / `.dot`). |
| `--query` / `--reference` | Keep only this query / reference sequence. |
| `--width` / `--height` | Picture size in pixels (default 1200). Ignored for `dot`. |
| `--min-block-size` | Drop individual blocks shorter than this. |
| `--min-alignment-size` | Drop records whose query span (`qEnd-qStart`) is shorter than this. |
| `--strand` | Keep `+`, `-`, or `both` (default) query strands. |
| `--bins N` | Rasterize into an `N×N` density grid (svg/png only). |

`--format dot` is a TSV of directed blocks in plot coordinates (`x` = reference,
`y` = query). Genome offsets are applied; minus-strand query blocks are already
flipped (`y0 > y1`). `#` comments list axis offsets; `pandas` / R skip them.

```bash
psltools dotplot -p in.psl --query chr1 --reference chr1 -o pair.svg
psltools dotplot -p in.psl -o genome.svg
psltools filter -p in.psl --min-score 4000 | psltools dotplot --bins 2000 -o dense.svg
psltools dotplot -p in.psl --format dot -o plot.tsv
```

```r
d <- read.table("plot.tsv", header=TRUE, comment.char="#")
ggplot(d, aes(x0, y0, xend=x1, yend=y1, colour=strand)) + geom_segment()
```

```python
import pandas as pd
d = pd.read_csv("plot.tsv", sep="\t", comment="#")
ax.plot(d[["x0", "x1"]].to_numpy().T, d[["y0", "y1"]].to_numpy().T)
```
