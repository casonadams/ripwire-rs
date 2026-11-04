use crate::support::index::index_files;
use ripwire_rs::commands::batch::execute_batch_content;
use ripwire_rs::commands::common::safety_queries::{
    execute_exercises_query, execute_safe_delete_query,
};
use ripwire_rs::query::graph::execute_path_query;
use ripwire_rs::query::safety::SafetyContext;
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{Graph, IngestResult, Interner, Lang, PageRankConfig};

fn fixture() -> (IngestResult, Graph, Interner) {
    index_files(&[(
        "lib.rs",
        Lang::Rust,
        b"pub fn run_cli() { helper(); } fn helper() {}",
    )])
}

fn safe_delete(
    (ingest, graph, interner): &(IngestResult, Graph, Interner),
    symbol: &str,
    out: &mut Vec<u8>,
) -> std::io::Result<()> {
    let facts = SafetyContext {
        root: ":memory:",
        ingest,
        graph,
        interner,
    };
    execute_safe_delete_query(&facts, symbol, PageRequest::default(), out)
}

#[test]
fn test_dispatched_queries_answer_the_fixture_graph() {
    let (ingest, graph, interner) = fixture();
    let pr = graph.page_rank(&PageRankConfig::default());

    let mut out = Vec::new();
    execute_path_query("run_cli,helper", &ingest, &graph, &interner, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("reachable=\"1\""), "{xml}");
    assert!(xml.contains("hops=\"1\""), "{xml}");
    let mut out = Vec::new();
    execute_path_query("run_cli,missing", &ingest, &graph, &interner, &mut out).unwrap_err();
    assert!(out.is_empty());

    let fixture = (ingest, graph, interner);
    let mut out = Vec::new();
    safe_delete(&fixture, "helper", &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(
        xml.contains("<safe-delete schema=\"ripwire-rs.safe-delete/v1\""),
        "{xml}"
    );
    assert!(xml.contains("run_cli"), "{xml}");
    let mut out = Vec::new();
    safe_delete(&fixture, "missing", &mut out).unwrap_err();
    assert!(out.is_empty());
    let (ingest, graph, interner) = fixture;

    let mut out = Vec::new();
    execute_batch_content(
        ".",
        "# fixture queries\n\ncallers:helper\ncallees:run_cli\n",
        &ingest,
        &graph,
        &pr,
        &interner,
        &mut out,
    )
    .unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("count=\"2\""), "{xml}");
    assert!(
        xml.contains("<callers schema=\"ripwire-rs.callers/v1\" of=\"helper\""),
        "{xml}"
    );
    assert!(
        xml.contains("<callees schema=\"ripwire-rs.callees/v1\" of=\"run_cli\""),
        "{xml}"
    );
    assert!(xml.ends_with("</batch>\n"), "{xml}");

    let (ingest, graph, interner) = index_files(&[
        ("lib.rs", Lang::Rust, b"pub fn run_cli() {}"),
        (
            "tests/check.rs",
            Lang::Rust,
            b"fn test_run() { run_cli(); }",
        ),
    ]);
    let mut out = Vec::new();
    execute_exercises_query(
        ":memory:",
        "tests/check.rs",
        &ingest,
        &graph,
        &interner,
        &mut out,
    )
    .unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(
        xml.contains("<exercises schema=\"ripwire-rs.exercises/v1\""),
        "{xml}"
    );
    assert!(xml.contains("run_cli"), "{xml}");
    assert!(xml.contains("lib.rs"), "{xml}");
}
