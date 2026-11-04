use crate::support::index::index_files;
use ripwire_rs::commands::{
    ast_match, at, connect, exemplar, for_task, from_trace, graph_query, lego, pack_task,
    path_between, verify,
};
use ripwire_rs::query::commands::QueryContext;
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{Graph, IngestResult, Interner, Lang, PageRankConfig, PageRankResult};
use std::io;

const FLOW_RS: &[u8] =
    b"pub fn entry() { bridge(); }\nfn bridge() { leaf(); }\nfn leaf() {}\nfn isolated() {}\n";
const OTHER_RS: &[u8] = b"fn other() {}\n";

static FIXTURE_SOURCES: &[(&str, &[u8])] = &[("src/flow.rs", FLOW_RS), ("other/flow.rs", OTHER_RS)];

fn fixture() -> (IngestResult, Graph, Interner) {
    index_files(&[
        ("src/flow.rs", Lang::Rust, FLOW_RS),
        ("other/flow.rs", Lang::Rust, OTHER_RS),
    ])
}

fn context<'a>(
    ingest: &'a IngestResult,
    graph: &'a Graph,
    pr: &'a PageRankResult,
    interner: &'a Interner,
) -> QueryContext<'a> {
    QueryContext {
        root: ":memory:",
        ingest,
        graph,
        pr,
        interner,
        commits: None,
        at: None,
        sources: &FIXTURE_SOURCES,
    }
}

fn run_with<F>(f: F) -> io::Result<String>
where
    F: FnOnce(&QueryContext<'_>, &mut Vec<u8>) -> io::Result<()>,
{
    let (ingest, graph, interner) = fixture();
    let ranks = graph.page_rank(&PageRankConfig::default());
    let mut output = Vec::new();
    f(&context(&ingest, &graph, &ranks, &interner), &mut output)?;
    Ok(String::from_utf8(output).unwrap())
}

#[test]
fn ast_match_shrinks_output_under_page_limit() {
    let limited = PageRequest {
        limit: Some(1),
        offset: None,
        budget: None,
    };

    let expr = "(function_item name: (identifier) @name)";
    let one = run_with(|q, out| ast_match::execute(q, expr, limited, out)).unwrap();
    assert!(one.contains("shown=\"1\""), "{one}");
    assert!(one.contains("has_more=\"1\""), "{one}");
}

#[test]
fn directed_paths_distinguish_transitive_reverse_and_disconnected_calls() {
    for (pair, reachable, hops) in [
        ("entry -> leaf", "1", "2"),
        ("leaf,entry", "0", "0"),
        ("entry:isolated", "0", "0"),
    ] {
        let xml = run_with(|q, out| path_between::execute(q, pair, out)).unwrap();
        assert!(xml.contains(&format!("reachable=\"{reachable}\"")), "{xml}");
        assert!(xml.contains(&format!("hops=\"{hops}\"")), "{xml}");
        if reachable == "1" {
            assert!(xml.contains("bridge"), "{xml}");
            assert!(!xml.contains("isolated"), "{xml}");
        }
    }
}

#[test]
fn connect_includes_intermediate_callers_but_not_disconnected_definitions() {
    let xml = run_with(|q, out| connect::execute(q, " entry , leaf ", Some(2), out)).unwrap();
    assert!(xml.contains("<t n=\"entry\""), "{xml}");
    assert!(xml.contains("<t n=\"leaf\""), "{xml}");
    assert!(xml.contains("<s n=\"bridge\""), "{xml}");
    assert!(xml.contains("<e f=\"entry\" t=\"bridge\""), "{xml}");
    assert!(xml.contains("<e f=\"bridge\" t=\"leaf\""), "{xml}");
    assert!(!xml.contains("isolated"), "{xml}");
}

#[test]
fn missing_graph_endpoints_fail_before_emitting_partial_results() {
    let (ingest, graph, interner) = fixture();
    let ranks = graph.page_rank(&PageRankConfig::default());
    let q = context(&ingest, &graph, &ranks, &interner);
    type Case = fn(&QueryContext<'_>, &mut Vec<u8>) -> io::Result<()>;
    let cases: [Case; 4] = [
        |q, out| connect::execute(q, "entry,absent", None, out),
        |q, out| path_between::execute(q, "absent,leaf", out),
        |q, out| path_between::execute(q, "entry,absent", out),
        |q, out| lego::execute(q, "absent", out),
    ];
    for case in cases {
        let mut output = Vec::new();
        let error = case(&q, &mut output).unwrap_err();
        assert!(error.to_string().contains("absent"), "{error}");
        assert!(output.is_empty());
    }
}

#[test]
fn location_resolution_rejects_ambiguous_suffixes_and_out_of_definition_lines() {
    let xml = run_with(|q, out| at::execute(q, "src/flow.rs:2", out)).unwrap();
    assert!(xml.contains("sym=\"bridge\""), "{xml}");
    assert!(xml.contains("l=\"2\""), "{xml}");
    for spec in ["flow.rs:1", "src/flow.rs:99", "missing.rs:1"] {
        assert!(
            run_with(|q, out| at::execute(q, spec, out)).is_err(),
            "{spec}"
        );
    }
}

#[test]
fn call_claims_report_transitive_witnesses_without_claiming_absence_proves_non_calls() {
    for (claim, verdict) in [
        ("calls(entry,bridge)", "confirmed"),
        ("calls(entry,leaf)", "confirmed"),
        ("calls(leaf,entry)", "not-established"),
        ("calls(absent,leaf)", "not-established"),
    ] {
        let xml =
            run_with(|q, out| verify::execute(q, claim, PageRequest::default(), out)).unwrap();
        assert!(xml.contains(&format!("verdict=\"{verdict}\"")), "{xml}");
    }
}

#[test]
fn task_section_errors_are_rejected_before_any_output_even_with_signatures_only() {
    let check = for_task::FOR_TASK
        .check
        .expect("for_task validates its sections");
    for sections in ["lego,lego", "compose,", "unknown"] {
        let args = for_task::ForTaskArgs {
            task_for: Some("locate parser".into()),
            signatures_only: true,
            auto_bodies: true,
            sections: Some(sections.into()),
            ..Default::default()
        };
        let error = check(&args).unwrap_err();
        assert!(error.to_string().contains("--sections="), "{error}");
    }
}

#[test]
fn quoted_search_literals_preserve_payload_and_do_not_capture_possessives() {
    use ripwire_rs::query::task_route::extract_quoted_task_literal;
    for (task, expected) in [
        ("find parser's failure", None),
        ("find parser's 'bad_token'", Some("bad_token")),
        ("find 'two words'", None),
        ("find \"two words & <tag>\"", Some("two words & <tag>")),
        ("find 'first' then \"second\"", Some("second")),
        ("find \"unfinished", None),
    ] {
        assert_eq!(extract_quoted_task_literal(task), expected, "{task}");
    }
}

#[test]
fn advanced_retrieval_and_ast_search_execute_over_context_sources() {
    let ast_xml = run_with(|q, out| {
        ast_match::execute(
            q,
            "(function_item name: (identifier) @name)",
            PageRequest::default(),
            out,
        )
    })
    .unwrap();
    let report = roxmltree::Document::parse(&ast_xml).unwrap();
    assert_eq!(report.root_element().attribute("hits"), Some("5"));
    assert_eq!(report.root_element().attribute("files"), Some("2"));

    let trace_xml = run_with(|q, out| from_trace::execute(q, "src/flow.rs:2", out)).unwrap();
    let report = roxmltree::Document::parse(&trace_xml).unwrap();
    let suspect = report
        .descendants()
        .find(|node| node.has_tag_name("suspect"))
        .unwrap();
    assert_eq!(suspect.attribute("n"), Some("bridge"));
    assert_eq!(suspect.attribute("p"), Some("src/flow.rs"));
    assert_eq!(suspect.attribute("l"), Some("2"));

    let pack_xml = run_with(|q, out| pack_task::execute(q, "inspect bridge", 6000, out)).unwrap();
    assert!(pack_xml.contains("bridge"), "{pack_xml}");
    assert!(pack_xml.contains("<sigs"), "{pack_xml}");

    let gq_xml =
        run_with(|q, out| graph_query::execute(q, "callees(entry)", PageRequest::default(), out))
            .unwrap();
    let report = roxmltree::Document::parse(&gq_xml).unwrap();
    let callees: Vec<_> = report
        .descendants()
        .filter(|node| node.has_tag_name("s"))
        .map(|node| (node.attribute("n"), node.attribute("p")))
        .collect();
    assert_eq!(callees, [(Some("bridge"), Some("src/flow.rs:2"))]);

    let ex_xml = run_with(|q, out| exemplar::execute(q, "entry", out)).unwrap();
    let report = roxmltree::Document::parse(&ex_xml).unwrap();
    assert_eq!(report.root_element().attribute("n"), Some("entry"));
    let body = report
        .descendants()
        .find(|node| node.has_tag_name("b"))
        .unwrap();
    assert!(body.text().unwrap().contains("bridge();"));
    let callees: Vec<_> = body
        .descendants()
        .filter(|node| node.has_tag_name("c"))
        .map(|node| (node.attribute("n"), node.attribute("l")))
        .collect();
    assert_eq!(callees, [(Some("bridge"), Some("2"))]);
}

fn selects<A: clap::FromArgMatches>(plugin: &ripwire_rs::commands::Plugin<A>, flag: &str) -> bool {
    let cli = ripwire_rs::parse_from(["rw", flag]).unwrap();
    cli.plugins
        .decode::<A>()
        .is_some_and(|args| (plugin.selected)(&args))
}

#[test]
fn each_plugin_is_selected_only_by_its_primary_flag() {
    use ripwire_rs::commands::{
        ast_match::AST_MATCH, at::AT, connect::CONNECT, exemplar::EXEMPLAR, for_task::FOR_TASK,
        from_trace::FROM_TRACE, graph_query::GRAPH_QUERY, grep::GREP, help_task::HELP_TASK,
        lego::LEGO, pack_task::PACK_TASK, path_between::PATH_BETWEEN, recall::RECALL,
        verify::VERIFY,
    };
    let any = |flag: &str| {
        selects(&FOR_TASK, flag)
            || selects(&GREP, flag)
            || selects(&CONNECT, flag)
            || selects(&PATH_BETWEEN, flag)
            || selects(&EXEMPLAR, flag)
            || selects(&LEGO, flag)
            || selects(&RECALL, flag)
            || selects(&FROM_TRACE, flag)
            || selects(&AT, flag)
            || selects(&VERIFY, flag)
            || selects(&PACK_TASK, flag)
            || selects(&GRAPH_QUERY, flag)
            || selects(&AST_MATCH, flag)
            || selects(&HELP_TASK, flag)
    };
    for flag in [
        "--for=a",
        "--grep=a",
        "--connect=a,b",
        "--path-between=a,b",
        "--exemplar=a",
        "--lego=a",
        "--recall=a",
        "--from-trace=a",
        "--at=a",
        "--verify=a",
        "--pack-task=a",
        "--graph-query=a",
        "--match=a",
        "--help-task=a",
    ] {
        assert!(any(flag), "{flag}");
    }
    assert!(!any("--adaptive"), "--adaptive");
}
