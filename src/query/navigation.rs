use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    Callers,
    Callees,
    Impact,
    Uses,
}

impl NavMode {
    #[must_use]
    pub const fn cli_flag(self) -> &'static str {
        match self {
            Self::Callers => "--callers",
            Self::Callees => "--callees",
            Self::Impact => "--impact",
            Self::Uses => "--uses",
        }
    }
}

impl std::fmt::Display for NavMode {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.cli_flag())
    }
}

pub struct NavContext<'a> {
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub ranks: &'a [f64],
    pub interner: &'a Interner,
    pub page: crate::serialize::PageRequest,
    pub max_depth: Option<u32>,
    pub pr_iters: Option<u32>,
}

impl<'a> crate::domain::CorpusView<'a> for NavContext<'a> {
    #[inline]
    fn ingest(&self) -> &'a IngestResult {
        self.ingest
    }

    #[inline]
    fn interner(&self) -> &'a Interner {
        self.interner
    }
}

/// One navigation request: the symbol asked about, how, under which root, and which page.
pub struct NavQuery<'a> {
    pub root: &'a str,
    pub sym: &'a str,
    pub mode: NavMode,
    pub declined: &'a [crate::navigate::DeclinedCall<'a>],
    pub def_rows: &'a [crate::navigate::ImpactDef<'a>],
    pub page: crate::serialize::PageRequest,
}

pub fn serialize_1hop_mode<W: Write>(
    query: &NavQuery<'_>,
    defs: usize,
    nav: &crate::navigate::NavNeighborhood,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    let (dir, items) = match query.mode {
        NavMode::Callers => (crate::navigate::NavDir::Callers, &nav.callers),
        NavMode::Callees => (crate::navigate::NavDir::Callees, &nav.callees),
        _ => return Ok(()),
    };
    crate::navigate::NavItemsReport {
        dir,
        root: query.root,
        of: query.sym,
        defs,
        def_rows: query.def_rows,
        items,
        declined: query.declined,
        gauge_attrs: &nav.gauge_attrs,
        page: query.page,
    }
    .render_envelope(out)
}

pub fn execute_nav_symbol_query_in_root<W: Write>(
    root: &str,
    sym: &str,
    mode: NavMode,
    ctx: &NavContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let ids = crate::navigate::find_symbols_by_name(sym, ctx.ingest, ctx.interner);
    if ids.is_empty() && !matches!(mode, NavMode::Uses) {
        return Err(symbol_not_found(mode, sym, ctx));
    }
    match mode {
        NavMode::Uses => execute_uses_query(
            root,
            &crate::navigate::scan_name(sym, ctx.ingest, ctx.interner),
            ctx,
            out,
        ),
        NavMode::Impact => serialize_impact(root, sym, &ids, ctx, out),
        NavMode::Callers | NavMode::Callees => serialize_one_hop(root, sym, mode, &ids, ctx, out),
    }
}

fn serialize_impact<W: Write>(
    root: &str,
    sym: &str,
    ids: &[NodeId],
    ctx: &NavContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    use crate::domain::CorpusView;
    use crate::serialize::XmlReport;
    let gauge_attrs = ctx.graph.xml_gauge_attrs();
    let impact = crate::navigate::transitive_impact_multi(
        ids,
        ctx.ingest,
        ctx.graph,
        ctx.ranks,
        ctx.interner,
        ctx.max_depth,
    );
    crate::navigate::ImpactReport {
        root,
        sym_name: sym,
        defs: crate::navigate::impact_defs(ids, ctx.ingest, ctx.interner),
        nodes: &impact,
        importers: (ctx.ingest, ctx.interner).resolve_symbol_importers(ids),
        declined: crate::navigate::declined_calls(ids, &impact, ctx.ingest, ctx.interner),
        gauge_attrs: &gauge_attrs,
        pr_iters: ctx.pr_iters,
        page: ctx.page,
    }
    .render_envelope(out)
}

fn serialize_one_hop<W: Write>(
    root: &str,
    sym: &str,
    mode: NavMode,
    ids: &[NodeId],
    ctx: &NavContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let Some(nav) =
        crate::navigate::navigate_1hop_multi(ids, ctx.ingest, ctx.graph, ctx.ranks, ctx.interner)
    else {
        return Ok(());
    };
    let declined = match mode {
        NavMode::Callers => crate::navigate::declined_calls(ids, &[], ctx.ingest, ctx.interner),
        NavMode::Callees => crate::navigate::declined_calls_from(ids, ctx.ingest, ctx.interner),
        _ => Vec::new(),
    };
    let def_rows = crate::navigate::impact_defs(ids, ctx.ingest, ctx.interner);
    let query = NavQuery {
        root,
        sym,
        mode,
        declined: &declined,
        def_rows: &def_rows,
        page: ctx.page,
    };
    serialize_1hop_mode(&query, ids.len(), &nav, out)
}

fn symbol_not_found(mode: NavMode, sym: &str, ctx: &NavContext<'_>) -> io::Error {
    let flag = mode.cli_flag();
    if crate::navigate::is_seed(sym)
        && let Err(cause) = crate::navigate::resolve_seed(sym, ctx.ingest, ctx.interner)
    {
        return io::Error::other(format!("ripwire-rs: {flag} {cause}"));
    }
    match crate::navigate::find_nearest_symbol_name(sym, ctx.ingest, ctx.interner) {
        Some(near) => io::Error::other(format!(
            "ripwire-rs: {flag} symbol not found: {sym} (did you mean '{near}'?)"
        )),
        None => io::Error::other(format!("ripwire-rs: {flag} symbol not found: {sym}")),
    }
}

fn execute_uses_query<W: Write>(
    root: &str,
    sym: &str,
    ctx: &NavContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    let report = crate::navigate::UsesReport::collect(sym, ctx.ingest, ctx.interner);
    if report.defs == 0 && report.sites.is_empty() {
        return Err(symbol_not_found(NavMode::Uses, sym, ctx));
    }
    crate::navigate::UsesReport {
        root,
        gauge_attrs: &ctx.graph.xml_gauge_attrs(),
        page: ctx.page,
        ..report
    }
    .render_envelope(out)
}
