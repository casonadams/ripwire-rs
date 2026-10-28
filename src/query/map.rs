use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::rank::PageRankResult;
use crate::serialize::SerializeOptions;
use std::borrow::Cow;
use std::io::{self, Write};

pub struct MapDiff<'a> {
    pub files: &'a [String],
    pub at: &'a str,
}

pub struct MapRequest<'a> {
    pub root: &'a str,
    pub rank_by: &'a str,
    pub pack_budget: Option<usize>,
    pub top_k: Option<usize>,
    pub emit_legend: bool,
    pub legend_full: bool,
    pub map_diff: Option<MapDiff<'a>>,
    pub no_inventory: bool,
}

impl<'a> MapRequest<'a> {
    #[must_use]
    pub const fn new(root: &'a str) -> Self {
        Self {
            root,
            rank_by: "pagerank",
            pack_budget: Some(crate::serialize::DEFAULT_PACK_BUDGET_BYTES),
            top_k: Some(200),
            emit_legend: true,
            legend_full: false,
            map_diff: None,
            no_inventory: false,
        }
    }
}

#[must_use]
pub fn token_budget_bytes(tokens: usize) -> usize {
    if tokens <= 200 {
        tokens.saturating_mul(4)
    } else {
        tokens.saturating_mul(2).saturating_sub(1100).max(512)
    }
}

pub fn execute_stream_xml_with<W: Write>(
    root: &str,
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let args = MapRequest::new(root);
    execute_stream_xml_ranked(&args, ingest, graph, pr, interner, out)
}
pub fn execute_stream_xml_ranked<W: Write>(
    args: &MapRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let is_stable = args.rank_by.eq_ignore_ascii_case("stable");
    let (ranks, rank_lbl) = map_ranks(args, ingest, graph, pr, interner)?;
    let stream_ctx = crate::serialize::StreamXmlContext {
        ingest,
        graph,
        ranks: &ranks,
        pr_iters: pr.iterations,
        pr_converged: pr.converged,
        interner,
    };

    crate::serialize::serialize_xml_with_ranks(
        &stream_ctx,
        out,
        &SerializeOptions {
            root: args.root.to_string(),
            top_k: args.top_k,
            include_pr: !is_stable,
            pack_budget_bytes: args.pack_budget,
            rank_by: rank_lbl.map(str::to_string),
            emit_legend: args.emit_legend,
            legend_full: args.legend_full,
            map_diff_at: args.map_diff.as_ref().map(|diff| diff.at.to_string()),
            metrics: false,
            no_inventory: args.no_inventory,
        },
    )
}

fn map_ranks<'a>(
    args: &MapRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    pr: &'a PageRankResult,
    interner: &Interner,
) -> io::Result<(Cow<'a, [f64]>, Option<&'static str>)> {
    if let Some(diff) = &args.map_diff {
        return Ok((
            Cow::Owned(diff_ranks(diff.files, ingest, graph, interner)),
            None,
        ));
    }
    if args.rank_by.eq_ignore_ascii_case("authority") || args.rank_by.eq_ignore_ascii_case("hub") {
        let node_files: Vec<u32> = ingest.symbols.iter().map(|s| s.file_id).collect();
        let hits = graph.hits(&node_files);
        return Ok(if args.rank_by.eq_ignore_ascii_case("hub") {
            (Cow::Owned(hits.hub), Some("hub"))
        } else {
            (Cow::Owned(hits.authority), Some("authority"))
        });
    }
    if args.rank_by.eq_ignore_ascii_case("degree") {
        let ranks = ingest
            .symbols
            .iter()
            .map(|s| (graph.in_degree(s.id) + graph.out_degree(s.id)) as f64)
            .collect();
        return Ok((Cow::Owned(ranks), Some("degree")));
    }
    if args.rank_by.eq_ignore_ascii_case("ccx") {
        let ranks = ingest.symbols.iter().map(|s| f64::from(s.ccx)).collect();
        return Ok((Cow::Owned(ranks), Some("ccx")));
    }
    if args.rank_by.eq_ignore_ascii_case("pagerank") || args.rank_by.eq_ignore_ascii_case("stable")
    {
        return Ok((Cow::Borrowed(&pr.ranks), None));
    }
    Err(io::Error::other(format!(
        "unknown ranking signal: {}",
        args.rank_by
    )))
}

fn diff_ranks(
    changed_files: &[String],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Vec<f64> {
    let n = ingest.symbols.len();
    let mut weights = vec![0.3 / (n.max(1) as f64); n];
    for (i, sym) in ingest.symbols.iter().enumerate() {
        let path = interner.resolve(&ingest.files[sym.file_id as usize].path);
        if changed_files
            .iter()
            .any(|file| file == path || file.ends_with(path))
        {
            weights[i] = 0.7;
        }
    }
    let teleport = crate::rank::biased_teleport(&weights);
    crate::rank::page_rank(
        &graph.in_edges,
        &graph.w_out_deg,
        &teleport,
        &crate::rank::PageRankConfig::default(),
    )
    .ranks
}
