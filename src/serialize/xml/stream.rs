use super::buckets::select_map_buckets;
use super::inventory::write_inv_section;
use super::options::{DEFAULT_STREAM_BUFFER_CAPACITY, SerializeOptions};
use super::root::{
    CliRootCounts, RootElementContext, write_cli_root_open, write_map_legend_header,
    write_root_open,
};
use super::rows::{SymbolRowContext, write_file_header, write_symbol_row};
use crate::domain::{CorpusView, IngestResult, Interner, NodeId, Symbol};
use crate::graph::Graph;
use crate::rank::PageRankResult;
use crate::serialize::PageRequest;
use std::io::{self, BufWriter, Write};

/// Counting writer to enforce streaming pack budgets.
pub struct CountingWriter<W> {
    inner: W,
    pub count: usize,
}

impl<W: Write> CountingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, count: 0 }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The root element's own open and close tags, which `est_tokens=` counts but the header is written before.
const ROOT_FRAME_BYTES: usize = 160;

/// Least budget left after the body for the inventory tier to be worth its element; below it the tier is dropped
/// so an explicit budget bounds the whole answer, not the body alone.
const MIN_INVENTORY_BYTES: usize = 512;

/// Stream minified, token-dense XML directly through a buffered writer to stdout or output.
/// Context for streaming XML serialization with custom ranks.
pub struct StreamXmlContext<'a> {
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub ranks: &'a [f64],
    pub pr_iters: u32,
    pub pr_converged: bool,
    pub interner: &'a Interner,
}

impl<'a> crate::domain::CorpusView<'a> for StreamXmlContext<'a> {
    #[inline]
    fn ingest(&self) -> &'a IngestResult {
        self.ingest
    }

    #[inline]
    fn interner(&self) -> &'a Interner {
        self.interner
    }
}

fn build_symbol_row_context<'a>(
    sym: &'a Symbol,
    sym_id: NodeId,
    ctx: &'a StreamXmlContext<'_>,
    callees: &'a [crate::graph::CalleeEdge],
    include_pr: bool,
    metrics: bool,
) -> SymbolRowContext<'a> {
    let sym_name = ctx.interner.resolve(&sym.name);
    let rank = ctx.ranks.get(sym_id as usize).copied().unwrap_or(0.0);
    let amb = ctx.graph.ambiguity(sym_id);
    let (in_degree, out_degree) = if metrics {
        (ctx.graph.in_degree(sym_id), ctx.graph.out_degree(sym_id))
    } else {
        (0, 0)
    };
    SymbolRowContext {
        sym,
        sym_name,
        rank,
        amb,
        callees,
        include_pr,
        metrics,
        in_degree,
        out_degree,
        amp: in_degree,
    }
}

fn serialize_single_file_bucket(
    ctx: &StreamXmlContext<'_>,
    sym_ids: &[NodeId],
    remaining_budget: usize,
    include_pr: bool,
    metrics: bool,
    file_buf: &mut Vec<u8>,
) -> io::Result<usize> {
    let mut shown = 0usize;
    for &sym_id in sym_ids {
        if file_buf.len() >= remaining_budget {
            break;
        }
        let sym = &ctx.ingest.symbols[sym_id as usize];
        let callees: Vec<crate::graph::CalleeEdge> = ctx.graph.call_flow_edges(sym_id).collect();
        let row_ctx = build_symbol_row_context(sym, sym_id, ctx, &callees, include_pr, metrics);
        write_symbol_row(file_buf, &row_ctx, ctx.interner, &ctx.ingest.symbols)?;
        shown += 1;
    }
    Ok(shown)
}

fn serialize_symbol_buckets(
    ctx: &StreamXmlContext<'_>,
    file_order: &[u32],
    buckets: &[Vec<NodeId>],
    budget: usize,
    include_pr: bool,
    metrics: bool,
) -> io::Result<(Vec<u8>, usize, Vec<bool>)> {
    let mut body_buf = Vec::with_capacity(budget.min(DEFAULT_STREAM_BUFFER_CAPACITY));
    let mut file_buf = Vec::new();
    let mut symbols_shown = 0usize;
    let mut listed_files = vec![false; ctx.ingest.files.len()];

    for &f in file_order {
        if body_buf.len() >= budget {
            break;
        }
        let file_path = ctx.file_path(f);
        file_buf.clear();
        write_file_header(&mut file_buf, file_path)?;
        let shown = serialize_single_file_bucket(
            ctx,
            &buckets[f as usize],
            budget.saturating_sub(body_buf.len()),
            include_pr,
            metrics,
            &mut file_buf,
        )?;
        if shown > 0 {
            symbols_shown += shown;
            listed_files[f as usize] = true;
            file_buf.extend_from_slice(b"</f>");
            body_buf.extend_from_slice(&file_buf);
        }
    }

    Ok((body_buf, symbols_shown, listed_files))
}

pub fn serialize_xml<W: Write>(
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    writer: &mut W,
    options: &SerializeOptions,
) -> io::Result<()> {
    serialize_xml_with_ranks(
        &StreamXmlContext {
            ingest,
            graph,
            ranks: &pr.ranks,
            pr_iters: pr.iterations,
            pr_converged: pr.converged,
            interner,
        },
        writer,
        options,
    )
}

pub fn serialize_xml_with_ranks<W: Write>(
    ctx: &StreamXmlContext<'_>,
    writer: &mut W,
    options: &SerializeOptions,
) -> io::Result<()> {
    let (file_order, buckets) = select_map_buckets(ctx, options);

    let budget = options.pack_budget_bytes.unwrap_or(usize::MAX);
    let (body_buf, symbols_shown, listed_files) = serialize_symbol_buckets(
        ctx,
        &file_order,
        &buckets,
        budget,
        options.include_pr,
        options.metrics,
    )?;

    let mut inv_buf = Vec::new();
    if options.allows_inventory_tier() {
        let left = options
            .pack_budget_bytes
            .map(|b| b.saturating_sub(body_buf.len()));
        if left.is_none_or(|bytes| bytes >= MIN_INVENTORY_BYTES) {
            write_inv_section(
                &mut inv_buf,
                ctx,
                &listed_files,
                PageRequest::default().with_budget(left),
            )?;
        }
    }
    let buf = BufWriter::with_capacity(DEFAULT_STREAM_BUFFER_CAPACITY, writer);
    let mut counter = CountingWriter::new(buf);
    let ambiguous = ctx.graph.amb_out.iter().map(|&a| a as usize).sum::<usize>();
    if options.emit_legend || options.metrics {
        write_map_legend_header(&mut counter, ctx, options, symbols_shown)?;
        let est_tokens = crate::serialize::est_tokens(
            counter.count + ROOT_FRAME_BYTES + body_buf.len() + inv_buf.len(),
        );
        let data_sections_cut = ctx
            .ingest
            .symbols
            .iter()
            .filter(|s| s.kind == crate::domain::SymKind::Section)
            .count();
        write_cli_root_open(
            &mut counter,
            options,
            CliRootCounts {
                pr_iters: ctx.pr_iters,
                est_tokens,
                data_sections_cut,
                page_size: symbols_shown,
            },
        )?;
    } else {
        let root_ctx = RootElementContext {
            root: &options.root,
            files: ctx.ingest.file_count(),
            symbols: ctx.ingest.symbol_count(),
            edges: ctx.graph.edge_count(),
            shown: symbols_shown,
            pr_iters: ctx.pr_iters,
            converged: ctx.pr_converged,
            rank_by: options.rank_by.as_deref(),
            ambiguous,
            unresolved: ctx.graph.total_unresolved(),
            schema: None,
        };
        write_root_open(&mut counter, &root_ctx)?;
    }
    counter.write_all(&body_buf)?;
    counter.write_all(&inv_buf)?;
    counter.write_all(b"</r>\n")?;
    counter.flush()
}
