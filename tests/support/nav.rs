use ripwire_rs::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use ripwire_rs::resolve::scip_sites;
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files,
};

pub fn nav_report(
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
    sym: &str,
    mode: NavMode,
) -> std::io::Result<String> {
    let ctx = NavContext {
        ingest,
        graph,
        ranks,
        interner,
        page: PageRequest::default(),
        max_depth: None,
        pr_iters: None,
    };
    let mut out = Vec::new();
    execute_nav_symbol_query_in_root(".", sym, mode, &ctx, &mut out)?;
    Ok(String::from_utf8(out).unwrap())
}

pub fn nav_xml(
    fixtures: &[(&str, Lang, &[u8])],
    scip: Option<&[u8]>,
    sym: &str,
    mode: NavMode,
) -> std::io::Result<String> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let mut ingest = ingest_in_memory_files(fixtures, &registry, &interner);
    if let Some(bytes) = scip {
        ingest.scip_sites = scip_sites(bytes, &ingest, &interner).unwrap();
    }
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    nav_report(&ingest, &graph, &ranks, &interner, sym, mode)
}
