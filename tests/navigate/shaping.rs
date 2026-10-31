use ripwire_rs::navigate::execute_pack_task;
use ripwire_rs::query::commands::QueryContext;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files};

fn session_source() -> String {
    (0..24)
        .map(|i| {
            format!(
                "pub fn refresh_session_token_{i}(id: u32) -> u32 {{\n    let fresh = id + {i};\n    fresh * 2\n}}\n"
            )
        })
        .collect()
}

fn pack(budget_tokens: usize) -> String {
    let source = session_source();
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files = [("src/session.rs", Lang::Rust, source.as_bytes())];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let sources = [("src/session.rs", source.as_bytes())];
    let ctx = QueryContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        sources: &sources,
        commits: None,
        at: None,
    };
    let mut out = Vec::new();
    execute_pack_task("refresh session token", &ctx, budget_tokens, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn ctx_attr<'a>(doc: &'a roxmltree::Document<'_>, name: &str) -> Option<&'a str> {
    doc.root_element().attribute(name)
}

fn signature_rows(doc: &roxmltree::Document<'_>) -> usize {
    doc.descendants().filter(|n| n.has_tag_name("d")).count()
}

#[test]
fn test_pack_admission_responds_to_the_requested_budget() {
    let small = pack(500);
    let large = pack(8000);
    let small_doc = roxmltree::Document::parse(&small).unwrap();
    let large_doc = roxmltree::Document::parse(&large).unwrap();

    assert_eq!(ctx_attr(&small_doc, "budget_tokens"), Some("500"));
    assert_eq!(ctx_attr(&large_doc, "budget_tokens"), Some("8000"));
    assert!(signature_rows(&large_doc) > signature_rows(&small_doc));
    assert!(small.len() < large.len());
}

#[test]
fn test_pack_est_tokens_counts_the_whole_document_within_its_ceiling() {
    for budget in [500usize, 2000, 8000] {
        let xml = pack(budget);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let est: usize = ctx_attr(&doc, "est_tokens").unwrap().parse().unwrap();
        assert_eq!(
            est,
            ripwire_rs::serialize::est_tokens(xml.len()),
            "est_tokens counts the whole document"
        );
        assert!(est <= budget, "budget {budget} gave est_tokens {est}");
        assert_eq!(ctx_attr(&doc, "over_budget"), None);
    }
}

#[test]
fn test_pack_omission_is_disclosed_when_the_budget_cuts_candidates() {
    let small = pack(500);
    let doc = roxmltree::Document::parse(&small).unwrap();
    let bodies = doc
        .descendants()
        .find(|n| n.has_tag_name("bodies"))
        .unwrap();
    let shown: usize = bodies.attribute("shown").unwrap().parse().unwrap();
    let total: usize = bodies.attribute("total").unwrap().parse().unwrap();
    assert!(shown < total, "{small}");
    assert_eq!(bodies.attribute("capped"), Some("1"));
}

#[test]
fn test_pack_zero_budget_keeps_only_the_minimal_unit_and_says_so() {
    let xml = pack(0);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(ctx_attr(&doc, "over_budget"), Some("1"));
    assert_eq!(signature_rows(&doc), 1);
}
