use ripwire_rs::query::commands::{BatchQuery, QueryContext, execute_batch, parse_batch_content};
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files};
use serde_json::json;

fn dispatch(
    context: &QueryContext<'_>,
    name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    ripwire_rs::query::commands::dispatch(context, ripwire_rs::commands::tools(), name, args)
}

fn with_context(run: impl FnOnce(&QueryContext<'_>)) {
    let sources = [(
        "virtual.rs",
        b"pub fn target() {}\nfn caller() { target(); }\n".as_slice(),
    )];
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let ingest = ingest_in_memory_files(
        &[("virtual.rs", Lang::Rust, sources[0].1)],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    run(&QueryContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        commits: None,
        sources: &sources,
        at: None,
    });
}

#[test]
fn map_top_k_selects_the_most_connected_definition() {
    let source = b"fn hub() { left(); right(); last(); }\nfn left() {}\nfn right() {}\nfn last() {}\nfn caller() { left(); }\n";
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[("virtual.rs", Lang::Rust, source.as_slice())],
        &registry,
        &interner,
    );
    let workspace = ripwire_rs::Workspace::from_ingest(":memory:", ingest, interner);
    let sources = [("virtual.rs", source.as_slice())];
    let context = workspace.query_context(&sources);
    let result = dispatch(
        &context,
        "explore",
        &json!({"rank_by": "degree", "top_k": 1}),
    )
    .unwrap();
    let report = roxmltree::Document::parse(result.as_str().unwrap()).unwrap();
    let names: Vec<_> = report
        .descendants()
        .filter(|node| node.has_tag_name("s"))
        .map(|node| node.attribute("n").unwrap())
        .collect();
    assert_eq!(names, ["hub"]);
}

#[test]
fn dispatch_rejects_invalid_arguments_and_returns_indexed_symbol() {
    with_context(|context| {
        for arguments in [
            json!({}),
            json!({"symbol": 7}),
            json!({"symbol": "target", "limit": -1}),
        ] {
            assert!(dispatch(context, "callers", &arguments).is_err());
        }
        assert!(dispatch(context, "not_a_command", &json!({})).is_err());
        let result = dispatch(context, "callers", &json!({"symbol": "target"})).unwrap();
        let xml = result.as_str().unwrap();
        assert!(xml.contains("target"));
        assert!(xml.contains("virtual.rs"));
    });
}

#[test]
fn insert_symbol_needs_a_before_or_after_position() {
    with_context(|context| {
        for arguments in [
            json!({"symbol": "target", "text": "x"}),
            json!({"symbol": "target", "text": "x", "position": "sideways"}),
        ] {
            assert!(dispatch(context, "insert_symbol", &arguments).is_err());
        }
    });
}

#[test]
fn batch_retains_successes_and_isolates_rejected_queries() {
    with_context(|context| {
        let tools = ripwire_rs::commands::tools();
        let queries = parse_batch_content(
            tools,
            "# indexed reads\ncallers:target\ncallers:{}\nreplace_symbol_body:{\"symbol\":\"target\",\"new_body\":\"fn target() {}\"}\nbatch:{}\nquality_delta:{\"ack_only\":true}\ncallers:caller\n",
        );
        let results = execute_batch(context, tools, &queries).unwrap();
        assert_eq!(results.len(), 6);
        assert!(results[0]["result"].as_str().unwrap().contains("target"));
        for result in &results[1..5] {
            assert!(result["error"].is_string());
            assert!(result.get("result").is_none());
        }
        assert!(results[5]["result"].as_str().unwrap().contains("caller"));
    });
}

#[test]
fn batch_enforces_sixteen_query_boundary() {
    with_context(|context| {
        let tools = ripwire_rs::commands::tools();
        let mut queries: Vec<_> = (0..16)
            .map(|_| BatchQuery {
                name: "callers".into(),
                arguments: json!({"symbol": "target"}),
            })
            .collect();
        assert_eq!(execute_batch(context, tools, &queries).unwrap().len(), 16);
        queries.push(BatchQuery {
            name: "callers".into(),
            arguments: json!({"symbol": "target"}),
        });
        assert!(execute_batch(context, tools, &queries).is_err());
    });
}

fn batch_items(queries: serde_json::Value) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    with_context(|context| {
        let result = dispatch(context, "batch", &json!({ "queries": queries })).unwrap();
        items = result.as_array().unwrap().clone();
    });
    items
}

#[test]
fn batch_malformed_items_preserve_order_and_structured_errors() {
    let items = batch_items(json!([
        {"name": "callers", "arguments": {"symbol": "target"}},
        {"name": 7}, null,
        {"name": "callers", "arguments": []},
        {"name": "unknown"},
        {"name": "callers", "arguments": {"symbol": "caller"}}
    ]));
    assert_eq!(items.len(), 6);
    assert!(items[0]["result"].as_str().unwrap().contains("target"));
    for item in &items[1..5] {
        assert!(item["error"].is_string());
    }
    assert!(items[5]["result"].as_str().unwrap().contains("caller"));
    assert_eq!(items[0]["kind"], "xml");
}

#[test]
fn batch_accepts_arguments_alongside_the_tool_name() {
    let items = batch_items(json!([
        {"tool": "callers", "symbol": "target"},
        {"name": "callers", "symbol": "caller"},
        {"name": "callers", "arguments": {"symbol": "target"}, "limit": 1},
        {"symbol": "target"}
    ]));
    assert!(items[0]["result"].as_str().unwrap().contains("target"));
    assert!(items[1]["result"].as_str().unwrap().contains("caller"));
    for item in &items[2..] {
        assert!(item["error"].as_str().unwrap().contains(r#"{"name": TOOL"#));
    }
}

#[test]
fn batch_xml_escapes_text_and_json_without_escaping_xml_reports() {
    let results = [
        json!({"tool": "memory_recall", "kind": "text", "result": "A & B <example/>"}),
        json!({"tool": "json", "kind": "json", "result": {"note": "<&"}}),
        json!({"tool": "callers", "kind": "xml", "result": "<symbols/>"}),
        json!({"tool": "unknown", "error": "unknown & rejected"}),
    ];
    let mut output = Vec::new();
    ripwire_rs::commands::batch::write_batch_results(":memory:", &results, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("A &amp; B &lt;example/&gt;"));
    assert!(xml.contains("&lt;&amp;"));
    assert!(xml.contains("<symbols/>"));
    assert!(xml.contains("unknown &amp; rejected"));

    let bad_xml = [json!({"tool": "bad", "kind": "xml", "result": 123})];
    let mut bad_out = Vec::new();
    assert!(
        ripwire_rs::commands::batch::write_batch_results(":memory:", &bad_xml, &mut bad_out)
            .is_err()
    );

    let mut esc_out = Vec::new();
    let sample = [json!({"tool": "callers", "kind": "xml", "result": "<symbols/>"})];
    ripwire_rs::commands::batch::write_batch_results("root&path", &sample, &mut esc_out).unwrap();
    assert!(
        String::from_utf8(esc_out)
            .unwrap()
            .contains("root=\"root&amp;path\"")
    );
}

#[test]
fn owners_dispatch_retains_root_revision_and_detail_author() {
    with_context(|context| {
        let commits = [ripwire_rs::git::history::CommitInfo {
            id: "revision".into(),
            timestamp: 1_700_000_000,
            author: "Ada".into(),
            changed_files: vec!["virtual.rs".into()],
        }];
        let owned = QueryContext {
            commits: Some(&commits),
            at: Some("revision:date"),
            ..*context
        };
        let folded = dispatch(&owned, "owners", &json!({})).unwrap();
        assert!(folded.as_str().unwrap().contains("<uniform"));
        let detail = dispatch(&owned, "owners", &json!({"detail": 1})).unwrap();
        let xml = detail.as_str().unwrap();
        assert!(xml.contains("root=\":memory:\""));
        assert!(xml.contains("revision:date"));
        assert!(xml.contains("virtual.rs"));
        assert!(xml.contains("Ada"));
    });
}

#[test]
fn catalogue_document_consumers_use_supplied_source_bytes() {
    let sources = [
        ("virtual.rs", b"pub fn target() {}\n".as_slice()),
        ("guide.md", b"# Guide\nUse `target` and replace `removed_api()`.\n".as_slice()),
        ("config.py", b"import os\nFLAG = os.getenv(\"IN_MEMORY_FLAG\")\n".as_slice()),
        ("test_runner.py", b"import unittest\nclass Tests(unittest.TestCase):\n    def test_target(self):\n        pass\nif __name__ == '__main__':\n    unittest.main()\n".as_slice()),
    ];
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            ("virtual.rs", Lang::Rust, sources[0].1),
            ("guide.md", Lang::Markdown, sources[1].1),
            ("config.py", Lang::Python, sources[2].1),
            ("test_runner.py", Lang::Python, sources[3].1),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let context = QueryContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        commits: Some(&[]),
        sources: &sources,
        at: Some("memory"),
    };
    let mentions = dispatch(&context, "mentions", &json!({"symbol": "target"})).unwrap();
    assert!(mentions.as_str().unwrap().contains("guide.md"));
    let flags = dispatch(&context, "flags", &json!({})).unwrap();
    assert!(flags.as_str().unwrap().contains("IN_MEMORY_FLAG"));
    let drift = dispatch(&context, "doc_drift", &json!({})).unwrap();
    assert!(drift.as_str().unwrap().contains("removed_api"));
    let affected = dispatch(&context, "affected", &json!({"files": "test_runner.py"})).unwrap();
    let document = roxmltree::Document::parse(affected.as_str().unwrap()).unwrap();
    assert!(
        document
            .descendants()
            .any(|node| node.attribute("run") == Some("python3 test_runner.py"))
    );
}

#[test]
fn whereis_catalogue_advertises_selectors_and_renderer_exposes_refs() {
    let descriptor = ripwire_rs::commands::tools()
        .iter()
        .find(|command| command.name == "whereis")
        .unwrap();
    for selector in ["listing", "full_text"] {
        assert!(
            descriptor
                .params
                .iter()
                .any(|&(name, _, _)| name == selector)
        );
    }
    with_context(|context| {
        assert!(
            dispatch(
                context,
                "whereis",
                &json!({"symbol": "target", "listing": "invalid"})
            )
            .unwrap_err()
            .contains("expected defs, refs, or all")
        );
        assert!(
            dispatch(
                context,
                "whereis",
                &json!({"symbol": "target", "full_text": 1})
            )
            .is_err()
        );
        let mut report = ripwire_rs::query::doc_tools::whereis_from_sources_with_facts(
            context.sources,
            context.ingest,
            context.interner,
            "target",
            &ripwire_rs::query::doc_tools::WhereisFacts::default(),
        );
        report["listing"] = "refs".into();
        report["full_text"] = true.into();
        let mut output = Vec::new();
        ripwire_rs::serialize::serialize_whereis_xml(&report, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();
        assert!(xml.contains("kind=\"ref\""));
        assert!(xml.contains("caller()"));
    });
}

#[test]
fn edit_check_pages_preserve_flagged_callers_and_offset_unflagged_rows() {
    let source = b"pub fn target(a: i32, b: i32) {}\nfn broken() { target(1); }\nfn first() { target(1, 2); }\nfn second() { target(3, 4); }\n";
    crate::support::safety_context::with_safety_context(
        ":memory:",
        &[("virtual.rs", std::str::from_utf8(source).unwrap())],
        |context| {
            let xml = crate::support::safety_context::target_edit_check_xml(
                context,
                Some((1, 1)),
                "memory",
            );
            assert!(xml.contains("n=\"broken\""));
            assert!(xml.contains("shown_unflagged=\"1\""));
            assert!(xml.contains("offset=\"1\" limit=\"1\""));
        },
    );
}

#[test]
fn dispatch_accepts_envelope_arguments_and_honors_legend() {
    with_context(|context| {
        let compact = dispatch(context, "callers", &json!({"symbol": "target"})).unwrap();
        assert!(compact.as_str().unwrap().starts_with("<!--"));
        let with_envelope = json!({
            "symbol": "target",
            "path": ".",
            "paths": ["src"],
            "legend": "none",
        });
        let none = dispatch(context, "callers", &with_envelope).unwrap();
        let xml = none.as_str().unwrap();
        assert!(!xml.contains("<!--"));
        assert!(xml.contains("target"));
        for bad in [
            json!({"symbol": "target", "legend": "verbose"}),
            json!({"symbol": "target", "paths": "src"}),
            json!({"symbol": "target", "bogus": 1}),
        ] {
            assert!(dispatch(context, "callers", &bad).is_err());
        }
    });
}

#[test]
fn envelope_path_scopes_tools_that_take_in_scope() {
    let files: [(&str, Lang, &[u8]); 2] = [
        (
            "alpha/one.rs",
            Lang::Rust,
            b"pub fn parse_alpha_input() {}\n",
        ),
        ("beta/two.rs", Lang::Rust, b"pub fn parse_beta_input() {}\n"),
    ];
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let workspace = ripwire_rs::Workspace::from_ingest(":memory:", ingest, interner);
    let sources = [("alpha/one.rs", files[0].2), ("beta/two.rs", files[1].2)];
    let context = workspace.query_context(&sources);
    let run = |args: serde_json::Value| {
        dispatch(&context, "for", &args)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    let whole = run(json!({"task": "parse input"}));
    assert!(whole.contains("parse_alpha_input") && whole.contains("parse_beta_input"));
    for path in ["alpha", "./alpha/"] {
        let scoped = run(json!({"task": "parse input", "path": path}));
        assert!(scoped.contains("parse_alpha_input"), "{path}");
        assert!(!scoped.contains("parse_beta_input"), "{path}");
    }
    let root = run(json!({"task": "parse input", "path": "."}));
    assert!(root.contains("parse_beta_input"));
}
