use super::options::SerializeOptions;
use super::stream::StreamXmlContext;
use crate::serialize::escape::escape_xml_write;
use std::io::{self, Write};

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

pub const SCHEMA_COMMENT: &[u8] = b"<!-- ripwire-rs map schema=ripwire-rs.map/v1: ranked symbol map: <f p= layer=> groups <s t= n= sc= k= amb=> rows (k= rank), <c n=> resolved callees; the header comment is data. est_tokens=: price as emitted (an upper bound under compact). root=: p= relative to it. pr_iters=N: PageRank iterations. declined=K: K calls left unbound (no evidence chose one def). prov=scip|binding|import|split|final-segment: how that <c> edge bound (absent: one unique name); split = one arm of an amb= pick; final-segment = a qualified type matched by last name only. files=/symbols=: files and symbols indexed; edges= distinct call edges; shown= symbols printed, a merged row counting each def; ambiguous= calls split over several defs, corpus-wide; unresolved= calls with in-tree evidence and no edge (defs all language-filtered, or import/pointer binding refused); order= rows by rank (important-first, important-last; (auto:fill) = flipped past a size threshold) or by path (stable). unindexed=ext:N: N text files of that extension no grammar reads (6 extensions at most). sc=: enclosing scope; the full id is p::sc::n (p= of the row or its <f>) and selectors take it. amb=K: K calls split over several defs.";
pub const SCHEMA_COMMENT_COMPACT: &[u8] = b"<!-- ripwire-rs map schema=ripwire-rs.map/v1: <f p= layer=> <s t= n= sc= k= amb=> <c n=> (dictv=80b7e589253a8e76; full dict: legend-dict)";

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
pub(super) struct CliRootCounts {
    pub(super) pr_iters: u32,
    pub(super) est_tokens: usize,
    pub(super) data_sections_cut: usize,
    pub(super) page_size: usize,
    pub(super) unreadable: usize,
}

pub(super) fn write_cli_root_open<W: Write>(
    w: &mut W,
    options: &SerializeOptions,
    counts: CliRootCounts,
) -> io::Result<()> {
    let CliRootCounts {
        pr_iters,
        est_tokens,
        data_sections_cut,
        page_size,
        unreadable,
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
    if unreadable > 0 {
        write!(w, " unreadable=\"{unreadable}\" counts_floor=\"1\"")?;
    }
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

pub(super) fn write_map_legend_header<W: Write>(
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
