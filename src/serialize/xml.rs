use crate::domain::{CorpusView, IngestResult, Interner, NodeId, Symbol};
use crate::graph::Graph;
use crate::rank::PageRankResult;
use crate::serialize::escape::{XmlEscapeExt, escape_xml_write};
use crate::serialize::{PageRequest, Paged, row_bytes};
use std::io::{self, BufWriter, Write};

pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 64 * 1024;
pub const DEFAULT_PACK_BUDGET_BYTES: usize = 8 * 1024;

/// Configuration options for streaming XML serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct SerializeOptions {
    pub root: String,
    pub top_k: Option<usize>,
    pub include_pr: bool,
    pub pack_budget_bytes: Option<usize>,
    pub rank_by: Option<String>,
    pub emit_legend: bool,
    pub legend_full: bool,
    pub map_diff_at: Option<String>,
    pub metrics: bool,
    pub no_inventory: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            root: ".".to_string(),
            top_k: None,
            include_pr: true,
            pack_budget_bytes: Some(DEFAULT_PACK_BUDGET_BYTES),
            rank_by: None,
            emit_legend: false,
            legend_full: false,
            map_diff_at: None,
            metrics: false,
            no_inventory: false,
        }
    }
}

const INVENTORY_NAME_CEILING: usize = 2000;

impl SerializeOptions {
    #[inline]
    #[must_use]
    pub fn has_full_pack_budget(&self) -> bool {
        self.pack_budget_bytes
            .is_none_or(|b| b >= DEFAULT_PACK_BUDGET_BYTES)
    }

    #[inline]
    #[must_use]
    pub fn allows_inventory_tier(&self) -> bool {
        self.emit_legend
            && !self.no_inventory
            && self.map_diff_at.is_none()
            && self.rank_by.is_none()
            && self.has_full_pack_budget()
    }
}

/// Metadata and callee context for serializing a single symbol row.
pub struct SymbolRowContext<'a> {
    pub sym: &'a Symbol,
    pub sym_name: &'a str,
    pub rank: f64,
    pub amb: u32,
    pub callees: &'a [crate::graph::CalleeEdge],
    pub include_pr: bool,
    pub metrics: bool,
    pub in_degree: usize,
    pub out_degree: usize,
    pub amp: usize,
}

/// Write the root `<r ...>` opening element.
/// Metadata for writing root XML element.
pub struct RootElementContext<'a> {
    pub root: &'a str,
    pub files: usize,
    pub symbols: usize,
    pub edges: usize,
    pub shown: usize,
    pub pr_iters: u32,
    pub converged: bool,
    pub rank_by: Option<&'a str>,
    pub ambiguous: usize,
    pub unresolved: usize,
    pub schema: Option<&'a str>,
}

/// Write the root `<r ...>` opening element.
pub fn write_root_open<W: Write>(w: &mut W, ctx: &RootElementContext<'_>) -> io::Result<()> {
    write_root_open_with_rank_by(w, ctx)
}

pub fn write_root_open_with_rank_by<W: Write>(
    w: &mut W,
    ctx: &RootElementContext<'_>,
) -> io::Result<()> {
    let schema = ctx.schema.unwrap_or("ripwire-rs.map/v1");
    write!(w, "<r schema=\"{schema}\" root=\"")?;
    escape_xml_write(w, ctx.root)?;
    write!(
        w,
        "\" files=\"{}\" symbols=\"{}\" edges=\"{}\" shown=\"{}\" ambiguous=\"{}\" unresolved=\"{}\"",
        ctx.files, ctx.symbols, ctx.edges, ctx.shown, ctx.ambiguous, ctx.unresolved
    )?;
    if let Some(rb) = ctx.rank_by {
        write!(w, " rank_by=\"{}\"", rb)?;
    } else {
        write!(w, " pr_iters=\"{}\"", ctx.pr_iters)?;
        if !ctx.converged {
            w.write_all(b" pr_converged=\"0\"")?;
        }
    }
    w.write_all(b">")
}

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

/// Write the file opening element `<f p="...">`.
pub fn write_file_header<W: Write>(w: &mut W, path: &str) -> io::Result<()> {
    use crate::serialize::PathLayerExt;
    path.write_file_open_tag(w)
}

fn write_symbol_metrics_tag<W: Write>(w: &mut W, ctx: &SymbolRowContext<'_>) -> io::Result<()> {
    write!(
        w,
        " in=\"{}\" out=\"{}\" cx=\"{}\" ccx=\"{}\" loc=\"{}\" params=\"{}\" nest=\"{}\" amp=\"{}\"",
        ctx.in_degree,
        ctx.out_degree,
        ctx.sym.cx,
        ctx.sym.ccx,
        ctx.sym.loc,
        ctx.sym.params,
        ctx.sym.max_nest,
        ctx.amp,
    )
}

/// Write a single symbol row and its outgoing call edges.
pub fn write_symbol_row<W: Write>(
    w: &mut W,
    ctx: &SymbolRowContext<'_>,
    interner: &Interner,
    symbols: &[Symbol],
) -> io::Result<()> {
    w.write_all(b"<s t=\"")?;
    w.write_all(ctx.sym.kind.tag().as_bytes())?;
    w.write_all(b"\" n=\"")?;
    escape_xml_write(w, ctx.sym_name)?;
    w.write_all(b"\"")?;

    if let Some(sc_spur) = ctx.sym.scope {
        let sc = interner.resolve(&sc_spur);
        if !sc.is_empty() {
            w.write_all(b" sc=\"")?;
            escape_xml_write(w, sc)?;
            w.write_all(b"\"")?;
        }
    }

    if ctx.include_pr {
        write!(w, " k=\"{:.4}\"", ctx.rank)?;
    }
    if ctx.amb > 0 {
        write!(w, " amb=\"{}\"", ctx.amb)?;
    }
    if ctx.metrics {
        write_symbol_metrics_tag(w, ctx)?;
    }
    w.write_all(b">")?;
    write_callees_tags(w, ctx.callees, interner, symbols)?;
    w.write_all(b"</s>")
}

fn write_callees_tags<W: Write>(
    w: &mut W,
    callees: &[crate::graph::CalleeEdge],
    interner: &Interner,
    symbols: &[Symbol],
) -> io::Result<()> {
    let mut by_name: Vec<(&str, crate::resolve::EdgeProv)> = Vec::with_capacity(callees.len());
    for edge in callees {
        let name = interner.resolve(&symbols[edge.to as usize].name);
        match by_name.iter_mut().find(|(n, _)| *n == name) {
            Some((_, prov)) => *prov = (*prov).max(edge.prov),
            None => by_name.push((name, edge.prov)),
        }
    }
    for (name, prov) in by_name {
        write_callee_tag(w, name, prov)?;
    }
    Ok(())
}

fn write_callee_tag<W: Write>(
    w: &mut W,
    name: &str,
    prov: crate::resolve::EdgeProv,
) -> io::Result<()> {
    write!(w, "<c n=\"{}\"", name.xml_escaped())?;
    if let Some(label) = prov.callee_label() {
        write!(w, " prov=\"{label}\"")?;
    }
    w.write_all(b"/>")
}

/// Compute symbol rank ordering and bucket symbols into files.
pub fn bucket_symbols_by_rank(
    symbols: &[Symbol],
    files_len: usize,
    ranks: &[f64],
    top_k: Option<usize>,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    let s = symbols.len();
    let keep = top_k.unwrap_or(s).min(s);

    let mut order: Vec<NodeId> = (0..s as NodeId)
        .filter(|&id| symbols[id as usize].is_code_symbol())
        .collect();
    order.sort_by(|&a, &b| {
        let ra = ranks.get(a as usize).copied().unwrap_or(0.0);
        let rb = ranks.get(b as usize).copied().unwrap_or(0.0);
        rb.total_cmp(&ra).then_with(|| a.cmp(&b))
    });
    let keep = keep.min(order.len());

    let mut buckets: Vec<Vec<NodeId>> = vec![Vec::new(); files_len];
    let mut file_order: Vec<u32> = Vec::new();
    let mut seen_files = vec![false; files_len];

    for &id in &order[..keep] {
        let f = symbols[id as usize].file_id;
        if (f as usize) < seen_files.len() && !seen_files[f as usize] {
            seen_files[f as usize] = true;
            file_order.push(f);
        }
        if (f as usize) < buckets.len() {
            buckets[f as usize].push(id);
        }
    }

    (file_order, buckets)
}

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

pub const SCHEMA_COMMENT: &[u8] = b"<!-- ripwire-rs map schema=ripwire-rs.map/v1: ranked symbol map: <f p= layer=> groups <s t= n= sc= k= amb=> rows (k= rank), <c n=> resolved callees; the header comment is data. est_tokens=: price as emitted (an upper bound under compact). root=: p= relative to it. pr_iters=N: PageRank iterations. declined=K: K calls left unbound (no evidence chose one def). prov=scip|binding|import|split|final-segment: how that <c> edge bound (absent: one unique name); split = one arm of an amb= pick; final-segment = a qualified type matched by last name only. files=/symbols=: files and symbols indexed; edges= distinct call edges; shown= symbols printed, a merged row counting each def; ambiguous= calls split over several defs, corpus-wide; unresolved= calls with in-tree evidence and no edge (defs all language-filtered, or import/pointer binding refused); order= rows by rank (important-first, important-last; (auto:fill) = flipped past a size threshold) or by path (stable). unindexed=ext:N: N text files of that extension no grammar reads (6 extensions at most). sc=: enclosing scope; the full id is p::sc::n (p= of the row or its <f>) and selectors take it. amb=K: K calls split over several defs.";
pub const SCHEMA_COMMENT_COMPACT: &[u8] = b"<!-- ripwire-rs map schema=ripwire-rs.map/v1: <f p= layer=> <s t= n= sc= k= amb=> <c n=> (dictv=80b7e589253a8e76; full dict: legend-dict)";

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

fn is_inv_code_file(lang: crate::domain::Lang, path: &str) -> bool {
    use crate::serialize::PathLayerExt;
    path.is_inventory_code_file(lang)
}

fn inv_entry_symbols<'a>(ctx: &StreamXmlContext<'a>) -> Vec<&'a Symbol> {
    use crate::domain::CorpusView;
    ctx.ingest
        .symbols
        .iter()
        .filter(|sym| {
            let name = ctx.sym_name(sym);
            let scope = ctx.sym_scope(sym);
            let path = ctx.sym_path(sym);
            sym.lang
                .semantics()
                .is_entrypoint_symbol(name, scope, sym.kind, path)
        })
        .collect()
}

fn write_inv_entry<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    sym: &Symbol,
) -> io::Result<()> {
    use crate::domain::CorpusView;
    let name = ctx.sym_name(sym);
    let path = ctx.sym_path(sym);
    write!(
        w,
        "<entry p=\"{}\" n=\"{}\">",
        path.xml_escaped(),
        name.xml_escaped()
    )?;
    let callees: Vec<crate::graph::CalleeEdge> = ctx.graph.callee_edges(sym.id).collect();
    write_callees_tags(w, &callees, ctx.interner, &ctx.ingest.symbols)?;
    w.write_all(b"</entry>")
}

fn write_inv_entries<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    page: PageRequest,
) -> io::Result<()> {
    let entries = inv_entry_symbols(ctx);
    let shown = Paged::new(&entries, page, |sym| {
        row_bytes(|mut w| write_inv_entry(&mut w, ctx, sym))
    });
    for sym in shown.rows() {
        write_inv_entry(w, ctx, sym)?;
    }
    Ok(())
}

fn collect_inv_dir_map<'a>(
    ctx: &StreamXmlContext<'a>,
    listed_files: &[bool],
) -> (
    std::collections::BTreeMap<&'a str, (usize, Vec<&'a str>)>,
    usize,
) {
    let mut dir_map: std::collections::BTreeMap<&'a str, (usize, Vec<&'a str>)> =
        std::collections::BTreeMap::new();
    let mut names_total = 0usize;
    for (idx, f) in ctx.ingest.files.iter().enumerate() {
        if listed_files.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let p = ctx.interner.resolve(&f.path);
        let (dir, fname) = p.rsplit_once('/').unwrap_or((".", p));
        let entry = dir_map.entry(dir).or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        if is_inv_code_file(f.lang, p) {
            names_total += 1;
            if entry.1.len() < 8 {
                entry.1.push(fname);
            }
        }
    }
    (dir_map, names_total)
}

fn write_inv_dir<W: Write>(
    w: &mut W,
    dir: &str,
    cnt: usize,
    code_files: &[&str],
) -> io::Result<()> {
    if code_files.is_empty() {
        write!(w, "<ls p=\"{}\" n=\"{cnt}\"/>", dir.xml_escaped())
    } else {
        let f_list = code_files.join(",");
        write!(
            w,
            "<ls p=\"{}\" n=\"{cnt}\" f=\"{}\"/>",
            dir.xml_escaped(),
            f_list.xml_escaped()
        )
    }
}

fn write_inv_section<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    listed_files: &[bool],
    page: PageRequest,
) -> io::Result<()> {
    let listed = listed_files.iter().filter(|&&b| b).count();
    let unlisted = ctx.ingest.files.len().saturating_sub(listed);
    if unlisted == 0 {
        return Ok(());
    }
    let (dir_map, names_total) = collect_inv_dir_map(ctx, listed_files);
    if names_total > INVENTORY_NAME_CEILING {
        write!(
            w,
            "<inv listed=\"{listed}\" unlisted=\"{unlisted}\" names_capped=\"1\" names_total=\"{names_total}\" names_next=\"--tree --limit={}\">",
            ctx.ingest.files.len()
        )?;
    } else {
        write!(w, "<inv listed=\"{listed}\" unlisted=\"{unlisted}\">")?;
    }
    write_inv_entries(w, ctx, page)?;
    let dirs: Vec<_> = dir_map.into_iter().collect();
    let shown = Paged::new(&dirs, page, |(dir, (cnt, files))| {
        row_bytes(|mut w| write_inv_dir(&mut w, dir, *cnt, files))
    });
    for (dir, (cnt, files)) in shown.rows() {
        write_inv_dir(w, dir, *cnt, files)?;
    }
    w.write_all(b"</inv>")
}

/// Stream minified XML with custom ranks and budget enforcement.
fn resolve_cli_map_schema(options: &SerializeOptions) -> &'static str {
    if options.map_diff_at.is_some() {
        "ripwire-rs.map-diff/v1"
    } else if options.metrics {
        "ripwire-rs.metrics/v1"
    } else {
        "ripwire-rs.map/v1"
    }
}

fn should_emit_data_sections_cut(options: &SerializeOptions, data_sections_cut: usize) -> bool {
    data_sections_cut > 0
        && options.map_diff_at.is_none()
        && options.top_k.is_none_or(|k| k >= 100)
        && options.has_full_pack_budget()
}

/// Per-report counts the map root element discloses.
struct CliRootCounts {
    pr_iters: u32,
    est_tokens: usize,
    data_sections_cut: usize,
    page_size: usize,
}

fn write_cli_root_open<W: Write>(
    w: &mut W,
    options: &SerializeOptions,
    counts: CliRootCounts,
) -> io::Result<()> {
    let CliRootCounts {
        pr_iters,
        est_tokens,
        data_sections_cut,
        page_size,
    } = counts;
    w.write_all(b"<r")?;
    if !options.legend_full {
        let schema = resolve_cli_map_schema(options);
        write!(w, " schema=\"{schema}\"")?;
    }
    if let Some(at) = &options.map_diff_at {
        write!(w, " at=\"{at}\"")?;
    }
    w.write_all(b" root=\"")?;
    escape_xml_write(w, &options.root)?;
    w.write_all(b"\"")?;
    if let Some(rb) = &options.rank_by {
        return write!(w, " rank_by=\"{rb}\" est_tokens=\"{est_tokens}\">");
    }
    write!(w, " est_tokens=\"{est_tokens}\" pr_iters=\"{pr_iters}\"")?;
    if should_emit_data_sections_cut(options, data_sections_cut) {
        let k_lim = options.top_k.unwrap_or(page_size);
        write!(
            w,
            " data_sections_cut=\"{data_sections_cut}\" next=\"--graph-query=&apos;kind(all,sec)&apos; --offset=0 --limit={k_lim}\""
        )?;
    }
    w.write_all(b">")
}

fn compute_effective_map_ranks(
    ctx: &StreamXmlContext<'_>,
    options: &SerializeOptions,
) -> Option<Vec<f64>> {
    use crate::domain::CorpusView;
    if options.map_diff_at.is_some()
        || !ctx
            .ingest
            .symbols
            .iter()
            .any(|s| ctx.is_production_symbol(s))
    {
        return None;
    }
    Some(
        ctx.ingest
            .symbols
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let r = ctx.ranks.get(i).copied().unwrap_or(0.0);
                if ctx.is_production_symbol(s) {
                    r
                } else {
                    r * 0.05
                }
            })
            .collect(),
    )
}

/// Stream minified XML with custom ranks and budget enforcement.
fn bucket_symbols_stable(
    ctx: &StreamXmlContext<'_>,
    top_k: Option<usize>,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    use crate::domain::CorpusView;
    let files_len = ctx.ingest.files.len();
    let mut file_order: Vec<u32> = (0..files_len as u32).collect();
    file_order.sort_by(|&a, &b| ctx.file_path(a).cmp(ctx.file_path(b)));
    let mut buckets: Vec<Vec<NodeId>> = vec![Vec::new(); files_len];
    let keep = top_k.unwrap_or(ctx.ingest.symbols.len());
    for sym in ctx
        .ingest
        .symbols
        .iter()
        .filter(|s| s.is_code_symbol())
        .take(keep)
    {
        if let Some(b) = buckets.get_mut(sym.file_id as usize) {
            b.push(sym.id);
        }
    }
    (file_order, buckets)
}

fn select_map_buckets(
    ctx: &StreamXmlContext<'_>,
    options: &SerializeOptions,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    if !options.include_pr {
        return bucket_symbols_stable(ctx, options.top_k);
    }
    let adjusted = compute_effective_map_ranks(ctx, options);
    let bucket_ranks = adjusted.as_deref().unwrap_or(ctx.ranks);
    bucket_symbols_by_rank(
        &ctx.ingest.symbols,
        ctx.ingest.files.len(),
        bucket_ranks,
        options.top_k,
    )
}

fn write_map_legend_header<W: Write>(
    counter: &mut W,
    ctx: &StreamXmlContext<'_>,
    options: &SerializeOptions,
    symbols_shown: usize,
) -> io::Result<()> {
    let header_prefix = if options.legend_full || options.metrics {
        SCHEMA_COMMENT
    } else {
        SCHEMA_COMMENT_COMPACT
    };
    counter.write_all(header_prefix)?;
    write!(
        counter,
        " files={} symbols={} edges={}",
        ctx.ingest.file_count(),
        ctx.ingest.symbol_count(),
        ctx.graph.edge_count(),
    )?;
    let precise = ctx.graph.scip_edge_count();
    if precise > 0 {
        write!(counter, " precise={precise}")?;
    }
    write!(counter, " shown=\"{symbols_shown}\"")?;
    crate::serialize::GraphGaugeProvider::write_summary_counts(ctx.graph, counter)?;
    if !options.include_pr {
        counter.write_all(b" order=stable")?;
    }
    counter.write_all(b" -->")
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

    let buf = BufWriter::with_capacity(DEFAULT_STREAM_BUFFER_CAPACITY, writer);
    let mut counter = CountingWriter::new(buf);
    let ambiguous = ctx.graph.amb_out.iter().map(|&a| a as usize).sum::<usize>();
    if options.emit_legend || options.metrics {
        write_map_legend_header(&mut counter, ctx, options, symbols_shown)?;
        let est_tokens = (body_buf.len() + 1200) * 2 / 5;
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
    if options.allows_inventory_tier() {
        let page = PageRequest::default().with_budget(options.pack_budget_bytes);
        write_inv_section(&mut counter, ctx, &listed_files, page)?;
    }
    counter.write_all(b"</r>\n")?;
    counter.flush()
}

/// Serialize to a minified XML string in memory (primarily for unit tests and inspection).
#[must_use]
pub fn serialize_xml_to_string(
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    options: &SerializeOptions,
) -> String {
    let mut out = Vec::new();
    serialize_xml(ingest, graph, pr, interner, &mut out, options)
        .expect("in-memory serialization should never fail");
    String::from_utf8(out).expect("valid utf-8 xml")
}
