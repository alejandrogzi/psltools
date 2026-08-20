// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use psltools::{
    Coord, OwnedPsl, PslRecord, Strand, StreamingReader, query_block_forward,
    reference_block_interval,
};

use super::{CliError, ensure_inputs_exist, write_output};

const DEFAULT_SIZE: u32 = 1200;
const MARGIN_LEFT: f64 = 56.0;
const MARGIN_RIGHT: f64 = 16.0;
const MARGIN_TOP: f64 = 16.0;
const MARGIN_BOTTOM: f64 = 48.0;

/// Arguments for the `dotplot` subcommand.
#[derive(Debug, Args)]
pub struct DotplotArgs {
    #[arg(
        short = 'p',
        long = "psl",
        value_name = "PATH",
        help = "Input .psl file(s). If omitted, read from standard input.",
        value_delimiter = ' ',
        num_args = 1..,
    )]
    inputs: Vec<PathBuf>,

    #[arg(
        short = 'o',
        long = "output",
        value_name = "FILE",
        help = "Output path (default stdout)."
    )]
    out: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        value_name = "svg|png|dot",
        help = "Output format. Default: infer from -o, else svg."
    )]
    format: Option<OutputFormat>,

    #[arg(long, value_name = "NAME", help = "Keep only this query sequence.")]
    query: Option<String>,

    #[arg(long, value_name = "NAME", help = "Keep only this reference sequence.")]
    reference: Option<String>,

    #[arg(long, default_value_t = DEFAULT_SIZE, help = "SVG width in pixels.")]
    width: u32,

    #[arg(long, default_value_t = DEFAULT_SIZE, help = "SVG height in pixels.")]
    height: u32,

    #[arg(long, value_name = "BP", help = "Drop blocks shorter than this.")]
    min_block_size: Option<Coord>,

    #[arg(
        long,
        value_name = "BP",
        help = "Drop records whose query span (qEnd-qStart) is shorter than this."
    )]
    min_alignment_size: Option<u64>,

    #[arg(
        long,
        value_enum,
        default_value_t = StrandFilter::Both,
        help = "Keep records with this query strand."
    )]
    strand: StrandFilter,

    #[arg(
        long,
        value_name = "N",
        help = "Rasterize into an N×N density grid instead of exact segments."
    )]
    bins: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Svg,
    Png,
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum StrandFilter {
    #[value(name = "both")]
    Both,
    #[value(name = "+")]
    Plus,
    #[value(name = "-")]
    Minus,
}

struct Filters {
    query: Option<Vec<u8>>,
    reference: Option<Vec<u8>>,
    strand: Option<Strand>,
    min_alignment_size: Option<u64>,
    min_block_size: Option<Coord>,
}

struct PlotSegment {
    x0: u64,
    y0: u64,
    x1: u64,
    y1: u64,
    strand: Strand,
}

struct Axis {
    names: Vec<Vec<u8>>,
    offsets: HashMap<Vec<u8>, u64>,
    total: u64,
}

struct Layout {
    width: f64,
    height: f64,
    plot_w: f64,
    plot_h: f64,
    t_total: u64,
    q_total: u64,
}

struct Plot<'a> {
    filters: &'a Filters,
    query: &'a Axis,
    reference: &'a Axis,
    layout: Layout,
    records: u64,
    blocks: u64,
}

/// Runs the `dotplot` subcommand. Emits SVG, PNG, or a TSV segment table.
pub fn run<R, W, E>(
    args: DotplotArgs,
    stdin: &mut R,
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<(), CliError>
where
    R: BufRead,
    W: Write,
    E: Write,
{
    let format = resolve_format(&args);
    validate(&args, format)?;
    let input_refs: Vec<&std::path::Path> = args.inputs.iter().map(PathBuf::as_path).collect();
    ensure_inputs_exist(&input_refs)?;

    let filters = Filters {
        query: args.query.as_ref().map(|s| s.as_bytes().to_vec()),
        reference: args.reference.as_ref().map(|s| s.as_bytes().to_vec()),
        strand: match args.strand {
            StrandFilter::Both => None,
            StrandFilter::Plus => Some(Strand::Forward),
            StrandFilter::Minus => Some(Strand::Reverse),
        },
        min_alignment_size: args.min_alignment_size,
        min_block_size: args.min_block_size,
    };

    let stats = if args.inputs.is_empty() {
        let mut kept = Vec::new();
        let mut reader = StreamingReader::new(stdin);
        while let Some(record) = reader.next_record()? {
            if keep_record(&record, &filters) {
                kept.push(record);
            }
        }
        let (query, reference) = axes_from_records(&kept, &filters)?;
        write_plot(
            &args,
            format,
            stdout,
            &filters,
            &query,
            &reference,
            |visit| {
                for record in &kept {
                    visit(record)?;
                }
                Ok(())
            },
        )?
    } else {
        let (query, reference) = axes_from_files(&args.inputs, &filters)?;
        write_plot(
            &args,
            format,
            stdout,
            &filters,
            &query,
            &reference,
            |visit| {
                for input in &args.inputs {
                    let mut reader = StreamingReader::from_path(input)?;
                    while let Some(record) = reader.next_record()? {
                        if keep_record(&record, &filters) {
                            visit(&record)?;
                        }
                    }
                }
                Ok(())
            },
        )?
    };

    super::log_summary("dotplot", &[("records", stats.0), ("blocks", stats.1)]);
    Ok(())
}

fn resolve_format(args: &DotplotArgs) -> OutputFormat {
    args.format
        .unwrap_or_else(|| infer_format(args.out.as_deref()))
}

fn infer_format(path: Option<&Path>) -> OutputFormat {
    let Some(ext) = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    else {
        return OutputFormat::Svg;
    };
    match ext.as_str() {
        "png" => OutputFormat::Png,
        "tsv" | "dot" => OutputFormat::Dot,
        _ => OutputFormat::Svg,
    }
}

fn validate(args: &DotplotArgs, format: OutputFormat) -> Result<(), CliError> {
    if args.bins == Some(0) {
        return Err(CliError::Message("--bins must be > 0".to_owned()));
    }
    if format == OutputFormat::Png {
        ensure_png_feature()?;
    }
    if format == OutputFormat::Dot {
        return Ok(());
    }
    if args.width == 0 || args.height == 0 {
        return Err(CliError::Message(
            "--width and --height must be positive".to_owned(),
        ));
    }
    if f64::from(args.width) <= MARGIN_LEFT + MARGIN_RIGHT
        || f64::from(args.height) <= MARGIN_TOP + MARGIN_BOTTOM
    {
        return Err(CliError::Message(
            "--width/--height too small for plot margins".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_png_feature() -> Result<(), CliError> {
    #[cfg(not(feature = "png"))]
    {
        Err(CliError::Message(
            "--format png requires psltools to be built with the `png` feature".to_owned(),
        ))
    }
    #[cfg(feature = "png")]
    {
        Ok(())
    }
}

fn keep_record<P: PslRecord>(record: &P, filters: &Filters) -> bool {
    if filters
        .query
        .as_deref()
        .is_some_and(|name| record.query_name() != name)
    {
        return false;
    }
    if filters
        .reference
        .as_deref()
        .is_some_and(|name| record.reference_name() != name)
    {
        return false;
    }
    if filters
        .strand
        .is_some_and(|strand| record.strands().query != strand)
    {
        return false;
    }
    if filters.min_alignment_size.is_some_and(|min| {
        (record.query_end() as u64).saturating_sub(record.query_start() as u64) < min
    }) {
        return false;
    }
    true
}

fn plot_segment<P: PslRecord>(record: &P, i: usize) -> PlotSegment {
    let query = query_block_forward(record, i);
    let reference = reference_block_interval(record, i);
    let mut x0 = reference.start as u64;
    let mut x1 = reference.end as u64;
    let mut y0 = query.start as u64;
    let mut y1 = query.end as u64;
    if record.strands().reference_or_forward() == Strand::Reverse {
        std::mem::swap(&mut x0, &mut x1);
    }
    if record.strands().query == Strand::Reverse {
        std::mem::swap(&mut y0, &mut y1);
    }
    PlotSegment {
        x0,
        y0,
        x1,
        y1,
        strand: record.strands().query,
    }
}

fn note_size(map: &mut HashMap<Vec<u8>, Coord>, name: &[u8], size: Coord) {
    map.entry(name.to_vec())
        .and_modify(|old| *old = (*old).max(size))
        .or_insert(size);
}

fn collect_sizes<P: PslRecord>(
    record: &P,
    query: &mut HashMap<Vec<u8>, Coord>,
    reference: &mut HashMap<Vec<u8>, Coord>,
) {
    note_size(query, record.query_name(), record.query_size());
    note_size(reference, record.reference_name(), record.reference_size());
}

fn build_axis(sizes: &HashMap<Vec<u8>, Coord>) -> Axis {
    let mut names: Vec<Vec<u8>> = sizes.keys().cloned().collect();
    names.sort_by(|a, b| cmp_natural(a, b));
    let mut offsets = HashMap::with_capacity(names.len());
    let mut total = 0u64;
    for name in &names {
        offsets.insert(name.clone(), total);
        total += u64::from(*sizes.get(name).expect("name from sizes"));
    }
    Axis {
        names,
        offsets,
        total,
    }
}

fn require_named(
    filters: &Filters,
    query: &HashMap<Vec<u8>, Coord>,
    reference: &HashMap<Vec<u8>, Coord>,
) -> Result<(), CliError> {
    if let Some(name) = filters.query.as_deref()
        && !query.contains_key(name)
    {
        return Err(CliError::Message(format!(
            "query not found: {}",
            String::from_utf8_lossy(name)
        )));
    }
    if let Some(name) = filters.reference.as_deref()
        && !reference.contains_key(name)
    {
        return Err(CliError::Message(format!(
            "reference not found: {}",
            String::from_utf8_lossy(name)
        )));
    }
    Ok(())
}

fn axes_from_records(records: &[OwnedPsl], filters: &Filters) -> Result<(Axis, Axis), CliError> {
    let mut query = HashMap::new();
    let mut reference = HashMap::new();
    for record in records {
        collect_sizes(record, &mut query, &mut reference);
    }
    require_named(filters, &query, &reference)?;
    Ok((build_axis(&query), build_axis(&reference)))
}

fn axes_from_files(paths: &[PathBuf], filters: &Filters) -> Result<(Axis, Axis), CliError> {
    let mut query = HashMap::new();
    let mut reference = HashMap::new();
    for path in paths {
        let mut reader = StreamingReader::from_path(path)?;
        while let Some(record) = reader.next_record()? {
            if keep_record(&record, filters) {
                collect_sizes(&record, &mut query, &mut reference);
            }
        }
    }
    require_named(filters, &query, &reference)?;
    Ok((build_axis(&query), build_axis(&reference)))
}

fn cmp_natural(a: &[u8], b: &[u8]) -> Ordering {
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            while i < a.len() && a[i] == b'0' {
                i += 1;
            }
            while j < b.len() && b[j] == b'0' {
                j += 1;
            }
            let is = i;
            let js = j;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            match (i - is).cmp(&(j - js)) {
                Ordering::Equal => {}
                other => return other,
            }
            match a[is..i].cmp(&b[js..j]) {
                Ordering::Equal => {}
                other => return other,
            }
        } else if a[i] != b[j] {
            return a[i].cmp(&b[j]);
        } else {
            i += 1;
            j += 1;
        }
    }
    a.len().cmp(&b.len())
}

fn write_plot<W: Write>(
    args: &DotplotArgs,
    format: OutputFormat,
    stdout: &mut W,
    filters: &Filters,
    query: &Axis,
    reference: &Axis,
    each: impl FnMut(&mut dyn FnMut(&OwnedPsl) -> Result<(), CliError>) -> Result<(), CliError>,
) -> Result<(u64, u64), CliError> {
    let mut plot = Plot {
        filters,
        query,
        reference,
        layout: Layout {
            width: f64::from(args.width),
            height: f64::from(args.height),
            plot_w: f64::from(args.width) - MARGIN_LEFT - MARGIN_RIGHT,
            plot_h: f64::from(args.height) - MARGIN_TOP - MARGIN_BOTTOM,
            t_total: reference.total,
            q_total: query.total,
        },
        records: 0,
        blocks: 0,
    };
    write_output(args.out.as_deref(), false, stdout, |w| {
        render(w, format, args.bins, &mut plot, each)
    })?;
    Ok((plot.records, plot.blocks))
}

fn render(
    w: &mut dyn Write,
    format: OutputFormat,
    bins: Option<u32>,
    plot: &mut Plot<'_>,
    each: impl FnMut(&mut dyn FnMut(&OwnedPsl) -> Result<(), CliError>) -> Result<(), CliError>,
) -> Result<(), CliError> {
    match format {
        OutputFormat::Dot => write_dot(w, plot, each),
        OutputFormat::Svg => write_picture(w, bins, plot, each),
        OutputFormat::Png => {
            let mut svg = Vec::new();
            write_picture(&mut svg, bins, plot, each)?;
            svg_to_png(&svg, w)
        }
    }
}

fn write_picture(
    w: &mut dyn Write,
    bins: Option<u32>,
    plot: &mut Plot<'_>,
    mut each: impl FnMut(&mut dyn FnMut(&OwnedPsl) -> Result<(), CliError>) -> Result<(), CliError>,
) -> Result<(), CliError> {
    if let Some(n) = bins {
        let n = n as usize;
        let cells = n
            .checked_mul(n)
            .ok_or_else(|| CliError::Message("--bins grid is too large".to_owned()))?;
        let mut grid = vec![0u32; cells];
        each(&mut |record| {
            plot.records += 1;
            for seg in segments(record, plot) {
                splat_segment(&mut grid, n, &seg, plot.reference.total, plot.query.total);
            }
            Ok(())
        })?;
        write_svg(w, plot, None, Some((n, &grid)))
    } else {
        let mut plus = String::new();
        let mut minus = String::new();
        each(&mut |record| {
            plot.records += 1;
            for seg in segments(record, plot) {
                let path = if seg.strand == Strand::Forward {
                    &mut plus
                } else {
                    &mut minus
                };
                let _ = write!(
                    path,
                    "M{:.1} {:.1}L{:.1} {:.1}",
                    plot.layout.scale_x(seg.x0),
                    plot.layout.scale_y(seg.y0),
                    plot.layout.scale_x(seg.x1),
                    plot.layout.scale_y(seg.y1)
                );
            }
            Ok(())
        })?;
        write_svg(w, plot, Some((&plus, &minus)), None)
    }
}

fn write_dot(
    w: &mut dyn Write,
    plot: &mut Plot<'_>,
    mut each: impl FnMut(&mut dyn FnMut(&OwnedPsl) -> Result<(), CliError>) -> Result<(), CliError>,
) -> Result<(), CliError> {
    writeln!(
        w,
        "# psltools-dotplot\tqTotal={}\trTotal={}",
        plot.query.total, plot.reference.total
    )?;
    write_axis_comments(w, "query", plot.query)?;
    write_axis_comments(w, "reference", plot.reference)?;
    writeln!(w, "x0\ty0\tx1\ty1\tstrand\tquery\treference")?;
    each(&mut |record| {
        plot.records += 1;
        let q = String::from_utf8_lossy(record.query_name());
        let r = String::from_utf8_lossy(record.reference_name());
        for seg in segments(record, plot) {
            let strand = if seg.strand == Strand::Forward {
                '+'
            } else {
                '-'
            };
            writeln!(
                w,
                "{}\t{}\t{}\t{}\t{strand}\t{q}\t{r}",
                seg.x0, seg.y0, seg.x1, seg.y1
            )?;
        }
        Ok(())
    })
}

fn write_axis_comments(w: &mut dyn Write, kind: &str, axis: &Axis) -> Result<(), CliError> {
    writeln!(w, "# {kind}\toffset\tsize")?;
    for (i, name) in axis.names.iter().enumerate() {
        let start = *axis.offsets.get(name).expect("offset");
        let end = axis
            .names
            .get(i + 1)
            .and_then(|n| axis.offsets.get(n).copied())
            .unwrap_or(axis.total);
        writeln!(
            w,
            "# {}\t{start}\t{}",
            String::from_utf8_lossy(name),
            end - start
        )?;
    }
    Ok(())
}

fn svg_to_png(svg: &[u8], w: &mut dyn Write) -> Result<(), CliError> {
    #[cfg(not(feature = "png"))]
    {
        let _ = (svg, w);
        unreachable!("png guarded in validate");
    }
    #[cfg(feature = "png")]
    {
        let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
            .map_err(|err| CliError::Message(format!("svg parse for png: {err}")))?;
        let size = tree.size().to_int_size();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or_else(|| CliError::Message("failed to allocate png pixmap".to_owned()))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
        );
        let png = pixmap
            .encode_png()
            .map_err(|err| CliError::Message(format!("png encode: {err}")))?;
        w.write_all(&png)?;
        Ok(())
    }
}

fn segments(record: &OwnedPsl, plot: &mut Plot<'_>) -> Vec<PlotSegment> {
    let Some(&x_off) = plot.reference.offsets.get(record.reference_name()) else {
        return Vec::new();
    };
    let Some(&y_off) = plot.query.offsets.get(record.query_name()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..record.block_count() {
        if plot
            .filters
            .min_block_size
            .is_some_and(|min| record.block_sizes()[i] < min)
        {
            continue;
        }
        plot.blocks += 1;
        let mut seg = plot_segment(record, i);
        seg.x0 += x_off;
        seg.x1 += x_off;
        seg.y0 += y_off;
        seg.y1 += y_off;
        out.push(seg);
    }
    out
}

fn splat_segment(grid: &mut [u32], n: usize, seg: &PlotSegment, x_total: u64, y_total: u64) {
    if n == 0 || x_total == 0 || y_total == 0 {
        return;
    }
    let n64 = n as u64;
    let bin = |v: u64, total: u64| -> i64 {
        ((v.min(total.saturating_sub(1)) * n64) / total).min(n64 - 1) as i64
    };
    walk_line(
        bin(seg.x0, x_total),
        bin(seg.y0, y_total),
        bin(seg.x1, x_total),
        bin(seg.y1, y_total),
        |x, y| {
            if (0..n as i64).contains(&x) && (0..n as i64).contains(&y) {
                let idx = y as usize * n + x as usize;
                grid[idx] = grid[idx].saturating_add(1);
            }
        },
    );
}

fn walk_line(mut x0: i64, mut y0: i64, x1: i64, y1: i64, mut plot: impl FnMut(i64, i64)) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    loop {
        plot(x0, y0);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 > -dy {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
}

impl Layout {
    fn scale_x(&self, x: u64) -> f64 {
        if self.t_total == 0 {
            MARGIN_LEFT
        } else {
            MARGIN_LEFT + (x as f64 / self.t_total as f64) * self.plot_w
        }
    }

    fn scale_y(&self, y: u64) -> f64 {
        if self.q_total == 0 {
            MARGIN_TOP + self.plot_h
        } else {
            MARGIN_TOP + self.plot_h - (y as f64 / self.q_total as f64) * self.plot_h
        }
    }
}

fn write_svg(
    w: &mut dyn Write,
    plot: &Plot<'_>,
    paths: Option<(&str, &str)>,
    bins: Option<(usize, &[u32])>,
) -> Result<(), CliError> {
    writeln!(
        w,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>\n\
         <style>.plus{{stroke:#2166ac;stroke-width:1;fill:none}}.minus{{stroke:#b2182b;stroke-width:1;fill:none}}\
         .grid{{stroke:#ccc;stroke-width:0.5}}.axis{{stroke:#333;stroke-width:1;fill:none}}\
         .lbl{{font:10px sans-serif;fill:#333}}</style>",
        w = plot.layout.width,
        h = plot.layout.height
    )?;
    render_axes(w, &plot.layout)?;
    render_boundaries(w, &plot.layout, plot.query, plot.reference)?;
    if let Some((plus, minus)) = paths {
        render_alignments(w, plus, minus)?;
    }
    if let Some((n, grid)) = bins {
        render_bins(w, &plot.layout, n, grid)?;
    }
    render_labels(w, &plot.layout, plot.query, plot.reference)?;
    writeln!(w, "</svg>")?;
    Ok(())
}

fn render_axes(w: &mut dyn Write, layout: &Layout) -> Result<(), CliError> {
    writeln!(
        w,
        "<rect class=\"axis\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"/>",
        MARGIN_LEFT, MARGIN_TOP, layout.plot_w, layout.plot_h
    )?;
    Ok(())
}

fn render_boundaries(
    w: &mut dyn Write,
    layout: &Layout,
    query: &Axis,
    reference: &Axis,
) -> Result<(), CliError> {
    let y0 = MARGIN_TOP;
    let y1 = MARGIN_TOP + layout.plot_h;
    let x0 = MARGIN_LEFT;
    let x1 = MARGIN_LEFT + layout.plot_w;
    for name in reference.names.iter().skip(1) {
        let x = layout.scale_x(*reference.offsets.get(name).expect("offset"));
        writeln!(
            w,
            "<line class=\"grid\" x1=\"{x:.1}\" y1=\"{y0:.1}\" x2=\"{x:.1}\" y2=\"{y1:.1}\"/>"
        )?;
    }
    for name in query.names.iter().skip(1) {
        let y = layout.scale_y(*query.offsets.get(name).expect("offset"));
        writeln!(
            w,
            "<line class=\"grid\" x1=\"{x0:.1}\" y1=\"{y:.1}\" x2=\"{x1:.1}\" y2=\"{y:.1}\"/>"
        )?;
    }
    Ok(())
}

fn render_alignments(w: &mut dyn Write, plus: &str, minus: &str) -> Result<(), CliError> {
    if !plus.is_empty() {
        writeln!(w, "<path class=\"plus\" d=\"{plus}\"/>")?;
    }
    if !minus.is_empty() {
        writeln!(w, "<path class=\"minus\" d=\"{minus}\"/>")?;
    }
    Ok(())
}

fn render_bins(w: &mut dyn Write, layout: &Layout, n: usize, grid: &[u32]) -> Result<(), CliError> {
    let max = grid.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return Ok(());
    }
    let cw = layout.plot_w / n as f64;
    let ch = layout.plot_h / n as f64;
    for y in 0..n {
        for x in 0..n {
            let count = grid[y * n + x];
            if count == 0 {
                continue;
            }
            let opacity = (f64::from(count) / f64::from(max)).clamp(0.15, 1.0);
            // y=0 is the first query bin (low coord) → SVG bottom.
            let sx = MARGIN_LEFT + x as f64 * cw;
            let sy = MARGIN_TOP + layout.plot_h - (y as f64 + 1.0) * ch;
            writeln!(
                w,
                "<rect x=\"{sx:.2}\" y=\"{sy:.2}\" width=\"{cw:.2}\" height=\"{ch:.2}\" fill=\"#2166ac\" fill-opacity=\"{opacity:.3}\"/>"
            )?;
        }
    }
    Ok(())
}

fn render_labels(
    w: &mut dyn Write,
    layout: &Layout,
    query: &Axis,
    reference: &Axis,
) -> Result<(), CliError> {
    for (i, name) in reference.names.iter().enumerate() {
        let start = *reference.offsets.get(name).expect("offset");
        let end = reference
            .names
            .get(i + 1)
            .and_then(|n| reference.offsets.get(n).copied())
            .unwrap_or(reference.total);
        let span = layout.scale_x(end) - layout.scale_x(start);
        let label = String::from_utf8_lossy(name);
        if !label_ok(span, &label) {
            continue;
        }
        let x = (layout.scale_x(start) + layout.scale_x(end)) / 2.0;
        let y = MARGIN_TOP + layout.plot_h + 14.0;
        writeln!(
            w,
            "<text class=\"lbl\" text-anchor=\"middle\" x=\"{x:.1}\" y=\"{y:.1}\">{}</text>",
            xml_escape(&label)
        )?;
    }
    for (i, name) in query.names.iter().enumerate() {
        let start = *query.offsets.get(name).expect("offset");
        let end = query
            .names
            .get(i + 1)
            .and_then(|n| query.offsets.get(n).copied())
            .unwrap_or(query.total);
        let span = layout.scale_y(start) - layout.scale_y(end); // start is lower on SVG
        let label = String::from_utf8_lossy(name);
        if !label_ok(span, &label) {
            continue;
        }
        let y = (layout.scale_y(start) + layout.scale_y(end)) / 2.0;
        writeln!(
            w,
            "<text class=\"lbl\" text-anchor=\"end\" dominant-baseline=\"middle\" x=\"{:.1}\" y=\"{y:.1}\">{}</text>",
            MARGIN_LEFT - 6.0,
            xml_escape(&label)
        )?;
    }
    Ok(())
}

fn label_ok(span_px: f64, name: &str) -> bool {
    span_px >= 12.0 && span_px >= name.len() as f64 * 5.0
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse_line(line: &str) -> OwnedPsl {
        let mut reader = StreamingReader::new(line.as_bytes());
        reader.next_record().unwrap().expect("record")
    }

    fn fixture() -> Vec<OwnedPsl> {
        let mut reader = StreamingReader::from_path("tests/data/dotplot.psl").unwrap();
        let mut out = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            out.push(r);
        }
        out
    }

    fn none_filters() -> Filters {
        Filters {
            query: None,
            reference: None,
            strand: None,
            min_alignment_size: None,
            min_block_size: None,
        }
    }

    fn args_for(path: &str) -> DotplotArgs {
        DotplotArgs {
            inputs: vec![PathBuf::from(path)],
            out: None,
            format: None,
            query: None,
            reference: None,
            width: DEFAULT_SIZE,
            height: DEFAULT_SIZE,
            min_block_size: None,
            min_alignment_size: None,
            strand: StrandFilter::Both,
            bins: None,
        }
    }

    fn output_from(args: DotplotArgs) -> Vec<u8> {
        let mut stdin = Cursor::new([]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run(args, &mut stdin, &mut stdout, &mut stderr).unwrap();
        stdout
    }

    fn svg_from(args: DotplotArgs) -> String {
        String::from_utf8(output_from(args)).unwrap()
    }

    fn body_rows(dot: &str) -> Vec<Vec<&str>> {
        dot.lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("x0"))
            .map(|l| l.split('\t').collect())
            .collect()
    }

    #[test]
    fn plus_strand_is_slash() {
        let p = parse_line(
            "10\t0\t0\t0\t0\t0\t0\t0\t+\tqPlus\t100\t10\t20\tchr1\t1000\t20\t30\t1\t10,\t10,\t20,\n",
        );
        let s = plot_segment(&p, 0);
        assert_eq!((s.x0, s.y0, s.x1, s.y1), (20, 10, 30, 20));
        assert!(s.x1 > s.x0 && s.y1 > s.y0);
        assert_eq!(s.strand, Strand::Forward);
    }

    #[test]
    fn minus_strand_is_backslash() {
        let p = parse_line(
            "20\t0\t0\t0\t0\t0\t0\t0\t-\tqMinus\t80\t50\t70\tchr1\t1000\t100\t120\t1\t20,\t10,\t100,\n",
        );
        let s = plot_segment(&p, 0);
        assert_eq!((s.x0, s.y0, s.x1, s.y1), (100, 70, 120, 50));
        assert!(s.x1 > s.x0 && s.y1 < s.y0);
        assert_eq!(s.strand, Strand::Reverse);
    }

    #[test]
    fn protein_plus_uses_size_mul() {
        let p = parse_line(
            "10\t0\t0\t0\t0\t0\t0\t0\t++\tprot1\t20\t0\t10\tchr3\t300\t0\t30\t1\t10,\t0,\t0,\n",
        );
        let s = plot_segment(&p, 0);
        assert_eq!((s.x0, s.y0, s.x1, s.y1), (0, 0, 30, 10));
    }

    #[test]
    fn protein_minus_reference_flips_x() {
        let p = parse_line(
            "10\t0\t0\t0\t0\t0\t0\t0\t+-\tprot2\t20\t0\t10\tchr3\t300\t270\t300\t1\t10,\t0,\t0,\n",
        );
        let s = plot_segment(&p, 0);
        assert_eq!((s.x0, s.y0, s.x1, s.y1), (300, 0, 270, 10));
        assert!(s.x1 < s.x0 && s.y1 > s.y0);
    }

    #[test]
    fn multi_block_keeps_gaps() {
        let p = parse_line(
            "10\t0\t0\t0\t1\t5\t1\t15\t+\tqMulti\t50\t0\t15\tchr2\t500\t0\t25\t2\t5,5,\t0,10,\t0,20,\n",
        );
        let a = plot_segment(&p, 0);
        let b = plot_segment(&p, 1);
        assert_eq!((a.x0, a.y0, a.x1, a.y1), (0, 0, 5, 5));
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (20, 10, 25, 15));
        assert_ne!((a.x1, a.y1), (b.x0, b.y0));
    }

    #[test]
    fn chromosome_offsets_use_natural_order() {
        let recs = fixture();
        let (query, reference) = axes_from_records(&recs, &none_filters()).unwrap();
        assert_eq!(
            reference
                .names
                .iter()
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .collect::<Vec<_>>(),
            ["chr1", "chr2", "chr3", "chr10"]
        );
        assert_eq!(*reference.offsets.get(b"chr1".as_slice()).unwrap(), 0);
        assert_eq!(*reference.offsets.get(b"chr2".as_slice()).unwrap(), 1000);
        assert_eq!(*reference.offsets.get(b"chr3".as_slice()).unwrap(), 1500);
        assert_eq!(*reference.offsets.get(b"chr10".as_slice()).unwrap(), 1800);
        assert_eq!(reference.total, 2000);

        assert_eq!(*query.offsets.get(b"prot1".as_slice()).unwrap(), 0);
        assert_eq!(*query.offsets.get(b"prot2".as_slice()).unwrap(), 20);
        assert_eq!(*query.offsets.get(b"qMinus".as_slice()).unwrap(), 40);
        assert_eq!(*query.offsets.get(b"qMulti".as_slice()).unwrap(), 120);
        assert_eq!(*query.offsets.get(b"qPlus".as_slice()).unwrap(), 170);
        assert_eq!(query.total, 270);
    }

    #[test]
    fn filters_query_reference_strand_and_sizes() {
        let recs = fixture();
        let qplus = recs
            .iter()
            .filter(|r| {
                keep_record(
                    *r,
                    &Filters {
                        query: Some(b"qPlus".to_vec()),
                        reference: Some(b"chr1".to_vec()),
                        strand: Some(Strand::Forward),
                        min_alignment_size: None,
                        min_block_size: None,
                    },
                )
            })
            .count();
        assert_eq!(qplus, 1);

        let minus_only = recs
            .iter()
            .filter(|r| {
                keep_record(
                    *r,
                    &Filters {
                        query: None,
                        reference: None,
                        strand: Some(Strand::Reverse),
                        min_alignment_size: None,
                        min_block_size: None,
                    },
                )
            })
            .count();
        assert_eq!(minus_only, 1);

        let long = recs
            .iter()
            .filter(|r| {
                keep_record(
                    *r,
                    &Filters {
                        query: None,
                        reference: None,
                        strand: None,
                        min_alignment_size: Some(15),
                        min_block_size: None,
                    },
                )
            })
            .count();
        // qMinus span 20, qMulti span 15; others are 10, 5, 2, 10, 10
        assert_eq!(long, 2);

        let tiny = parse_line(
            "2\t0\t0\t0\t0\t0\t0\t0\t+\tqPlus\t100\t30\t32\tchr2\t500\t10\t12\t1\t2,\t30,\t10,\n",
        );
        assert!(keep_record(&tiny, &none_filters()));
        assert_eq!(tiny.block_sizes()[0], 2);
    }

    #[test]
    fn svg_smoke_file_and_stdin() {
        let svg = svg_from(args_for("tests/data/dotplot.psl"));
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("class=\"plus\""));
        assert!(svg.contains("class=\"minus\""));
        assert!(svg.contains(">chr1<"));
        assert!(svg.contains(">chr10<"));
        assert!(svg.contains("</svg>"));
        assert!(svg.len() > 200);

        let mut args = args_for("tests/data/dotplot.psl");
        args.inputs.clear();
        let data = std::fs::read("tests/data/dotplot.psl").unwrap();
        let mut stdin = Cursor::new(data);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run(args, &mut stdin, &mut stdout, &mut stderr).unwrap();
        let stdin_svg = String::from_utf8(stdout).unwrap();
        assert!(stdin_svg.contains("<svg"));
        assert!(stdin_svg.contains("class=\"plus\""));
    }

    #[test]
    fn empty_input_is_valid_svg() {
        let mut args = args_for("tests/data/dotplot.psl");
        args.inputs.clear();
        let mut stdin = Cursor::new([]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run(args, &mut stdin, &mut stdout, &mut stderr).unwrap();
        let svg = String::from_utf8(stdout).unwrap();
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("class=\"plus\""));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn rejects_missing_query_and_zero_bins() {
        let mut args = args_for("tests/data/dotplot.psl");
        args.query = Some("nope".into());
        let mut stdin = Cursor::new([]);
        let err = run(args, &mut stdin, &mut Vec::new(), &mut Vec::new()).unwrap_err();
        assert!(err.to_string().contains("query not found"));

        let mut args = args_for("tests/data/dotplot.psl");
        args.bins = Some(0);
        let err = run(args, &mut stdin, &mut Vec::new(), &mut Vec::new()).unwrap_err();
        assert!(err.to_string().contains("--bins"));

        let mut args = args_for("tests/data/dotplot.psl");
        args.width = 0;
        let err = run(args, &mut stdin, &mut Vec::new(), &mut Vec::new()).unwrap_err();
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn min_block_size_drops_short_blocks() {
        let all = svg_from(args_for("tests/data/dotplot.psl"));
        let mut args = args_for("tests/data/dotplot.psl");
        args.min_block_size = Some(5);
        let filtered = svg_from(args);
        assert!(
            filtered.matches("M").count() < all.matches("M").count(),
            "min-block-size should drop the 2bp block"
        );
    }

    #[test]
    fn binned_svg_emits_rects() {
        let mut args = args_for("tests/data/dotplot.psl");
        args.bins = Some(20);
        let svg = svg_from(args);
        assert!(svg.contains("<rect x="));
        assert!(!svg.contains("class=\"plus\""));
    }

    #[test]
    fn infers_format_from_extension() {
        assert_eq!(infer_format(Some(Path::new("a.png"))), OutputFormat::Png);
        assert_eq!(infer_format(Some(Path::new("a.TSV"))), OutputFormat::Dot);
        assert_eq!(infer_format(Some(Path::new("a.dot"))), OutputFormat::Dot);
        assert_eq!(infer_format(Some(Path::new("a.svg"))), OutputFormat::Svg);
        assert_eq!(infer_format(None), OutputFormat::Svg);
        let mut args = args_for("tests/data/dotplot.psl");
        args.out = Some(PathBuf::from("x.png"));
        args.format = Some(OutputFormat::Dot);
        assert_eq!(resolve_format(&args), OutputFormat::Dot);
    }

    #[test]
    fn dot_table_flips_strands_keeps_gaps_and_offsets() {
        let mut args = args_for("tests/data/dotplot.psl");
        args.format = Some(OutputFormat::Dot);
        args.query = Some("qPlus".into());
        args.reference = Some("chr1".into());
        let pair = String::from_utf8(output_from(args)).unwrap();
        let rows = body_rows(&pair);
        assert_eq!(rows, [["20", "10", "30", "20", "+", "qPlus", "chr1"]]);

        let mut args = args_for("tests/data/dotplot.psl");
        args.format = Some(OutputFormat::Dot);
        args.query = Some("qMinus".into());
        args.reference = Some("chr1".into());
        let minus = String::from_utf8(output_from(args)).unwrap();
        assert_eq!(
            body_rows(&minus),
            [["100", "70", "120", "50", "-", "qMinus", "chr1"]]
        );

        let mut args = args_for("tests/data/dotplot.psl");
        args.format = Some(OutputFormat::Dot);
        args.query = Some("qMulti".into());
        args.reference = Some("chr2".into());
        let multi_text = String::from_utf8(output_from(args)).unwrap();
        let multi = body_rows(&multi_text);
        assert_eq!(multi.len(), 2);
        assert_eq!(multi[0], ["0", "0", "5", "5", "+", "qMulti", "chr2"]);
        assert_eq!(multi[1], ["20", "10", "25", "15", "+", "qMulti", "chr2"]);
        assert_ne!((multi[0][2], multi[0][3]), (multi[1][0], multi[1][1]));

        let mut args = args_for("tests/data/dotplot.psl");
        args.format = Some(OutputFormat::Dot);
        let wg = String::from_utf8(output_from(args)).unwrap();
        assert!(wg.contains("# chr1\t0\t1000"));
        assert!(wg.contains("# chr2\t1000\t500"));
        assert!(wg.contains("# chr3\t1500\t300"));
        assert!(wg.contains("# chr10\t1800\t200"));
        // qPlus offset 170 + local 10,20
        let plus = body_rows(&wg)
            .into_iter()
            .find(|r| r[5] == "qPlus" && r[6] == "chr1")
            .unwrap();
        assert_eq!(plus, ["20", "180", "30", "190", "+", "qPlus", "chr1"]);
    }

    #[test]
    fn png_without_feature_errors() {
        let mut args = args_for("tests/data/dotplot.psl");
        args.format = Some(OutputFormat::Png);
        let err = run(args, &mut Cursor::new([]), &mut Vec::new(), &mut Vec::new());
        #[cfg(not(feature = "png"))]
        {
            assert!(
                err.unwrap_err().to_string().contains("`png` feature"),
                "expected png feature error"
            );
        }
        #[cfg(feature = "png")]
        {
            let png = output_from({
                let mut args = args_for("tests/data/dotplot.psl");
                args.format = Some(OutputFormat::Png);
                args
            });
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            let _ = err;
        }
    }
}
