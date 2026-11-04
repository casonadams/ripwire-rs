use ripwire_rs::navigate::{
    VerifyContext, evaluate_verify_claim, execute_graph_query_expr_paged, execute_pack_task,
    execute_pack_task_partitioned, truncate_body_to_budget,
};
use ripwire_rs::query::commands::QueryContext;
use ripwire_rs::rank::PageRankConfig;
use ripwire_rs::serialize::{PageRequest, PathLayerExt};
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol,
};

#[test]
fn test_graph_query_execution() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/test.rs"),
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 500,
    };
    let sym_a = Symbol::new(
        0,
        interner.intern("foo"),
        interner.intern("c_foo"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let sym_b = Symbol::new(
        1,
        interner.intern("bar"),
        interner.intern("c_bar"),
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    );
    let ref_ab = Reference::new(0, interner.intern("bar"), 0, 2, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.push(f0);
    ingest.symbols.extend_from_slice(&[sym_a, sym_b]);
    ingest.references.extend_from_slice(&[ref_ab]);

    let graph = Graph::build(&ingest, &interner);
    let mut out = Vec::new();
    execute_graph_query_expr_paged(
        "callers(name(\"bar\"))",
        ".",
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    let out_str = String::from_utf8(out).unwrap();
    assert!(out_str.contains("<query schema=\"ripwire-rs.graph-query/v1\""));
    assert!(out_str.contains("count=\"1\""));
    assert!(out_str.contains("n=\"foo\""));

    let mut out = Vec::new();
    execute_graph_query_expr_paged(
        "callees(name(\"foo\"))",
        ".",
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    assert!(String::from_utf8(out).unwrap().contains("n=\"bar\""));

    let mut out = Vec::new();
    execute_graph_query_expr_paged(
        "and(kind(all, fn), file(all, test), fanin(all, 1), cx(all, 10))",
        ".",
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    assert!(String::from_utf8(out).unwrap().contains("n=\"bar\""));

    let mut out = Vec::new();
    execute_graph_query_expr_paged(
        "or(not(name(\"foo\")), callers(\"bar\", 1))",
        ".",
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    assert!(String::from_utf8(out).unwrap().contains("count=\"2\""));
    let mut paged_out = Vec::new();
    execute_graph_query_expr_paged(
        "or(not(name(\"foo\")), callers(\"bar\", 1))",
        ".",
        &ingest,
        &graph,
        &interner,
        ripwire_rs::serialize::PageRequest {
            limit: Some(1),
            offset: Some(0),
            budget: None,
        },
        &mut paged_out,
    )
    .unwrap();
    let paged_str = String::from_utf8(paged_out).unwrap();
    assert!(paged_str.contains("shown=\"1\" capped=\"1\" total=\"2\""));

    let long_body = "let x = 1;\n".repeat(400);
    let truncated = truncate_body_to_budget(&long_body, 512);
    assert!(truncated.len() < 600);
    assert!(truncated.contains("truncated to fit pack-task budget"));

    let sources = [("src/test.rs", &b"fn foo() { bar(); }\nfn bar() {}\n"[..])];
    let pr = graph.page_rank(&PageRankConfig::default());
    let query_ctx = QueryContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        sources: &sources,
        commits: None,
        at: None,
    };
    let v_ctx = VerifyContext::from(&query_ctx);
    for (claim, expected) in [
        ("calls(foo,bar)", "confirmed"),
        ("calls(bar,foo)", "not-established"),
        ("uses(bar)", "confirmed"),
        ("unused(bar)", "refuted"),
        ("unused(nonexistent_sym)", "not-established"),
        ("contains(test.rs, \"foo\")", "confirmed"),
        ("contains(test.rs, \"missing_lit_xyz\")", "refuted"),
        ("defines(test.rs, foo)", "confirmed"),
        ("defines(test.rs, missing_sym_xyz)", "refuted"),
        ("reaches(bar, \"test.rs\")", "confirmed"),
        ("reaches(foo, \"test.rs\")", "not-established"),
        ("invalid_claim", "not-established"),
    ] {
        let mut v_out = Vec::new();
        evaluate_verify_claim(
            claim,
            &v_ctx,
            ripwire_rs::serialize::PageRequest::default(),
            &mut v_out,
        )
        .unwrap();
        let v_str = String::from_utf8(v_out).unwrap();
        assert!(
            v_str.contains(&format!("verdict=\"{expected}\"")),
            "claim {claim} expected {expected}, got: {v_str}"
        );
    }
    let mut p_out = Vec::new();
    execute_pack_task("search", &query_ctx, 6000, &mut p_out).unwrap();
    assert!(String::from_utf8(p_out).unwrap().contains("<sigs>"));
    let mut named_out = Vec::new();
    execute_pack_task("foo", &query_ctx, 6000, &mut named_out).unwrap();
    let named = String::from_utf8(named_out).unwrap();
    assert!(named.contains("route=\"name-exact(foo)\""), "{named}");
    let mut pp_out = Vec::new();
    execute_pack_task_partitioned("foo bar", ".", 2, &ingest, &graph, &interner, &mut pp_out)
        .unwrap();
    let pp_str = String::from_utf8(pp_out).unwrap();
    assert!(pp_str.contains("<ctx-partitions"));
    assert!(pp_str.contains("role=\"core\""));
    for tag in [
        "function",
        "method",
        "class",
        "struct",
        "interface",
        "section",
        "unknown",
    ] {
        let mut k_out = Vec::new();
        execute_graph_query_expr_paged(
            &format!("kind(all, {tag})"),
            ".",
            &ingest,
            &graph,
            &interner,
            PageRequest::default(),
            &mut k_out,
        )
        .unwrap();
        assert!(!k_out.is_empty());
    }
    assert_eq!("src/infra/nodekind.h".builtin_layer(), Some("infra"));
    assert_eq!("src/ui/render/renderer.rs".builtin_layer(), Some("render"));
    assert_eq!("packages/ai/src/usage.ts".builtin_layer(), Some("ai"));
    assert_eq!("src/lib.rs".builtin_layer(), None);
    let mut l_out = Vec::new();
    execute_graph_query_expr_paged(
        "layer(all, \"test\")",
        ".",
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
        &mut l_out,
    )
    .unwrap();
    assert!(String::from_utf8(l_out).unwrap().contains("count=\"0\""));
}
