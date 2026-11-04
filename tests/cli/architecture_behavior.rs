use crate::support::index::index_files;
use ripwire_rs::cli::TopModifierArgs;
use ripwire_rs::commands::dead_code::{DeadCodeContext, execute_dead_code_query_paged};
use ripwire_rs::commands::external_surface::execute_external_surface_query;
use ripwire_rs::commands::nav::{NavigateArgs, execute_navigate_with};
use ripwire_rs::metrics::architecture::{build_community_module, compute_deps_report};
use ripwire_rs::query::map::{MapRequest, execute_stream_xml_ranked};
use ripwire_rs::query::navigation::NavContext;
use ripwire_rs::{Lang, PageRankConfig, find_symbols_by_name};

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    let key = format!("{name}=\"");
    element
        .strip_prefix(&key)
        .or_else(|| {
            element
                .split_once(&format!(" {key}"))
                .map(|(_, value)| value)
        })
        .unwrap_or_else(|| panic!("missing {name} in {element}"))
        .split_once('"')
        .unwrap()
        .0
}

fn symbol_names(xml: &str) -> Vec<&str> {
    xml.split("<s ").skip(1).map(|row| attr(row, "n")).collect()
}

fn external_surface_xml(files: &[(&str, Lang, &[u8])]) -> String {
    let (ingest, _, interner) = index_files(files);
    let mut out = Vec::new();
    execute_external_surface_query(
        "memory",
        &ingest,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
        &mut out,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

fn external_surface_rows(xml: &str) -> Vec<&str> {
    xml.split_once("<external-surface ")
        .unwrap()
        .1
        .split("<x ")
        .skip(1)
        .map(|row| row.split_once('>').unwrap().0)
        .collect()
}

#[test]
fn external_surface_counts_calls_but_excludes_definitions_builtins_and_value_uses() {
    let xml = external_surface_xml(&[(
        "src/lib.rs",
        Lang::Rust,
        b"fn local() {}\nfn run(values: Vec<u8>) {\n    local();\n    vendor_send();\n    vendor_send();\n    vendor_flush();\n    values.len();\n    let callback = vendor_callback;\n}\n",
    )]);
    let rows = external_surface_rows(&xml);
    assert_eq!(
        rows.iter().map(|row| attr(row, "n")).collect::<Vec<_>>(),
        ["vendor_send", "vendor_flush"],
        "{xml}"
    );
    assert_eq!(attr(rows[0], "calls"), "2");
    assert_eq!(attr(rows[0], "refs"), "2");
    assert_eq!(attr(rows[1], "calls"), "1");
}

#[test]
fn external_surface_ignores_doc_links_shell_builtins_and_callable_fields() {
    let xml = external_surface_xml(&[
        (
            "docs/guide.md",
            Lang::Markdown,
            b"See [agents](AGENTS_GUIDE.md) and [setup](docs/INSTALL.md).\n",
        ),
        ("scripts/run.sh", Lang::Bash, b"cd /tmp\necho ok\ntrap done EXIT\n"),
        (
            "src/lib.rs",
            Lang::Rust,
            b"struct Hooks { measure: fn() }\nfn run(hooks: &Hooks) {\n    (hooks.measure)();\n    vendor_send();\n}\n",
        ),
    ]);
    let names: Vec<&str> = external_surface_rows(&xml)
        .iter()
        .map(|row| attr(row, "n"))
        .collect();
    assert_eq!(names, ["vendor_send"], "{xml}");
}

#[test]
fn community_members_exclude_module_scope_owners() {
    let (ingest, graph, interner) = index_files(&[(
        "pkg/app.py",
        Lang::Python,
        b"def entry():\n    helper()\n\ndef helper():\n    pass\n\nentry()\n",
    )]);
    let (_, _, modules) = ripwire_rs::metrics::architecture::build_ranked_communities(
        None, &ingest, &graph, &interner,
    );
    let members: Vec<&str> = modules
        .iter()
        .flat_map(|module| module.members.iter().map(|member| member.1))
        .collect();
    assert!(members.contains(&"helper"), "{members:?}");
    assert!(!members.contains(&"<file-scope>"), "{members:?}");
}

#[test]
fn rust_struct_literal_is_an_edge_to_the_constructed_type() {
    let (ingest, graph, interner) = index_files(&[(
        "src/lib.rs",
        Lang::Rust,
        b"struct Hook { run: u8 }\nstruct Unused;\nfn build() -> Hook {\n    Hook { run: 1 }\n}\nfn plain() {}\n",
    )]);
    let id_of = |name: &str| {
        ingest
            .symbols
            .iter()
            .find(|s| interner.resolve(&s.name) == name)
            .unwrap_or_else(|| panic!("no symbol {name}"))
            .id
    };
    assert!(
        graph.in_degree(id_of("Hook")) > 0,
        "literal Hook {{ .. }} must reach the type"
    );
    assert_eq!(
        graph.in_degree(id_of("Unused")),
        0,
        "a type never constructed has no edge"
    );
    assert_eq!(graph.in_degree(id_of("plain")), 0);
    let build = id_of("build");
    assert!(graph.callee_edges(build).any(|e| e.to == id_of("Hook")));
    assert!(
        graph.call_flow_edges(build).all(|e| e.to != id_of("Hook")),
        "type use is not call flow"
    );
}
#[test]
fn community_anchor_prefers_production_code_then_substantial_callable() {
    let (ingest, _, interner) = index_files(&[
        (
            "tests/check.rs",
            Lang::Rust,
            b"fn test_large() {\n if true {\n if true {\n if true {\n panic!();\n }\n }\n }\n}\n",
        ),
        ("docs/guide.md", Lang::Markdown, b"# Overview\n\nText.\n"),
        (
            "src/lib.rs",
            Lang::Rust,
            b"fn small() {}\nfn substantial(flag: bool) {\n if flag {\n small();\n } else {\n small();\n }\n}\n",
        ),
    ]);
    let ids: Vec<_> = ["Overview", "test_large", "small", "substantial"]
        .iter()
        .map(|name| find_symbols_by_name(name, &ingest, &interner)[0])
        .collect();
    let module = build_community_module(7, &ids, &ingest, &interner);
    assert_eq!(module.label, "substantial");
    assert_eq!(module.dir, "src/lib.rs");
    assert_eq!(module.size, 4);
    assert_eq!(
        module
            .members
            .iter()
            .map(|member| member.1)
            .collect::<Vec<_>>(),
        ["Overview", "test_large", "small", "substantial"]
    );

    let module = build_community_module(7, &ids[..3], &ingest, &interner);
    assert_eq!(
        module.label, "small",
        "test size must not displace production code"
    );
}

#[test]
fn community_summary_partitions_isolated_documents_declarations_and_bodies() {
    let (ingest, graph, interner) = index_files(&[
        (
            "src/lib.rs",
            Lang::Rust,
            b"fn entry() { linked(); }\nfn linked() {}\nfn unused() {}\n",
        ),
        ("include/api.h", Lang::C, b"void declared(void);\n"),
        (
            "docs/guide.md",
            Lang::Markdown,
            b"An isolated document without headings.\n",
        ),
    ]);
    let mut out = Vec::new();
    let ctx = ripwire_rs::query::safety::SafetyContext {
        root: "memory",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    ripwire_rs::commands::common::safety_queries::execute_partition_from_facts(
        &ctx,
        2,
        None,
        ripwire_rs::serialize::PageRequest::default(),
        &mut out,
    )
    .unwrap();
    let xml = String::from_utf8(out).unwrap();
    let summary = xml.split_once("<communities ").unwrap().1;
    assert_eq!(attr(summary, "isolated"), "3", "{xml}");
    assert_eq!(attr(summary, "isolated_source"), "1", "{xml}");
    assert_eq!(attr(summary, "isolated_decl"), "1", "{xml}");
    assert_eq!(attr(summary, "isolated_doc"), "1", "{xml}");
    assert!(xml.contains("n=\"entry\""), "{xml}");
    assert!(xml.contains("n=\"linked\""), "{xml}");
}

#[test]
fn dependency_report_deduplicates_file_edges_and_detects_transitive_cycles() {
    let (ingest, graph, interner) = index_files(&[
        (
            "src/a.rs",
            Lang::Rust,
            b"fn start() { middle(); middle(); }\n",
        ),
        ("src/b.rs", Lang::Rust, b"fn middle() { finish(); }\n"),
        ("src/c.rs", Lang::Rust, b"fn finish() { middle(); }\n"),
        ("src/alone.rs", Lang::Rust, b"fn alone() {}\n"),
    ]);
    let report = compute_deps_report(&ingest, &graph, &interner);
    let row = |path| report.rows.iter().find(|row| row.path == path).unwrap();
    assert_eq!((row("src/a.rs").efferent, row("src/a.rs").afferent), (1, 0));
    // Component dependency counts include the file itself.
    assert_eq!(row("src/a.rs").transitive, 3);
    assert_eq!((row("src/b.rs").efferent, row("src/b.rs").afferent), (1, 2));
    assert_eq!(row("src/b.rs").transitive, 2);
    assert_eq!(row("src/c.rs").transitive, 2);
    assert_eq!(
        (row("src/alone.rs").efferent, row("src/alone.rs").transitive),
        (0, 1)
    );
    assert_eq!(report.ccd, 8);
    assert_eq!(report.cycles.len(), 1);
    let mut cycle = report.cycles[0].clone();
    cycle.sort_unstable();
    assert_eq!(cycle, ["src/b.rs", "src/c.rs"]);
}

#[test]
fn ranked_stream_selects_authorities_hubs_and_preserves_complete_rows_at_budget() {
    let (ingest, graph, interner) = index_files(&[(
        "src/lib.rs",
        Lang::Rust,
        b"fn entry() { sink_a(); sink_b(); }\nfn sink_a() {}\nfn sink_b() {}\nfn isolated() {}\n",
    )]);
    let pr = graph.page_rank(&PageRankConfig::default());
    let mut args = MapRequest {
        root: "memory",
        rank_by: "authority",
        pack_budget: None,
        top_k: Some(2),
        emit_legend: false,
        legend_full: false,
        map_diff: None,
        no_inventory: true,
    };
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    let mut names = symbol_names(&xml);
    names.sort_unstable();
    assert_eq!(names, ["sink_a", "sink_b"], "{xml}");

    args.rank_by = "hub";
    args.top_k = Some(1);
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert_eq!(symbol_names(&xml), ["entry"], "{xml}");
    assert!(xml.contains("<c n=\"sink_a\""), "{xml}");
    assert!(xml.contains("<c n=\"sink_b\""), "{xml}");

    args.rank_by = "pagerank";
    args.top_k = None;
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    let full = String::from_utf8(out).unwrap();
    let mut top_names = symbol_names(&full)[..2].to_vec();
    top_names.sort_unstable();
    assert_eq!(top_names, ["sink_a", "sink_b"], "{full}");
    let first_row_end = full.find("</s>").unwrap() + "</s>".len();
    args.pack_budget = Some(first_row_end - full.find("<f ").unwrap());
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    let limited = String::from_utf8(out).unwrap();
    assert_eq!(
        symbol_names(&limited),
        &symbol_names(&full)[..1],
        "{limited}"
    );
    assert_eq!(limited.matches("</s>").count(), 1);
    assert!(limited.ends_with("</s></f></r>\n"), "{limited}");

    args.pack_budget = Some(0);
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    assert!(symbol_names(&String::from_utf8(out).unwrap()).is_empty());

    args.rank_by = "stable";
    args.pack_budget = None;
    args.top_k = Some(2);
    let mut out = Vec::new();
    execute_stream_xml_ranked(&args, &ingest, &graph, &pr, &interner, &mut out).unwrap();
    let stable = String::from_utf8(out).unwrap();
    assert_eq!(symbol_names(&stable), ["entry", "sink_a"], "{stable}");
    assert!(!stable.contains(" k=\""), "{stable}");
}

#[test]
fn navigation_reports_direct_neighbors_and_transitive_impact_without_reversing_edges() {
    let (ingest, graph, interner) = index_files(&[
        ("src/entry.rs", Lang::Rust, b"fn entry() { middle(); }\n"),
        ("src/middle.rs", Lang::Rust, b"fn middle() { leaf(); }\n"),
        (
            "src/leaf.rs",
            Lang::Rust,
            b"fn leaf() {}\nfn unrelated() {}\n",
        ),
    ]);
    let pr = graph.page_rank(&PageRankConfig::default());

    let ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &pr.ranks,
        pr_iters: Some(pr.iterations),
        interner: &interner,
        page: Default::default(),
        max_depth: None,
    };
    let bare = NavigateArgs {
        symbol: Some("middle".to_string()),
        ..Default::default()
    };
    let mut out = Vec::new();
    assert!(execute_navigate_with(".", &bare, &ctx, &mut out).is_err());
    for (callers, callees, impact, target, expected) in [
        (true, false, false, "middle", vec!["entry"]),
        (false, true, false, "middle", vec!["leaf"]),
        (false, false, true, "leaf", vec!["entry", "middle"]),
    ] {
        let args = NavigateArgs {
            symbol: Some(target.to_string()),
            callers,
            callees,
            impact,
            ..Default::default()
        };
        let mut out = Vec::new();
        execute_navigate_with(".", &args, &ctx, &mut out).unwrap();
        let xml = String::from_utf8(out).unwrap();
        let mut names = symbol_names(&xml);
        names.sort_unstable();
        assert_eq!(names, expected, "{xml}");
        assert!(!names.contains(&"unrelated"));
    }
}

#[test]
fn mermaid_aggregates_cross_directory_calls_without_internal_or_isolated_edges() {
    let (ingest, graph, interner) = index_files(&[
        (
            "src/ui/entry.rs",
            Lang::Rust,
            b"fn entry() { load_rows(); store_rows(); local(); }\nfn local() {}\n",
        ),
        (
            "src/core/logic.rs",
            Lang::Rust,
            b"fn load_rows() {}\nfn store_rows() {}\n",
        ),
        ("src/unused/alone.rs", Lang::Rust, b"fn alone() {}\n"),
    ]);
    let mut out = Vec::new();
    ripwire_rs::graph::generate_mermaid_diagram(&ingest, &graph, &interner, &mut out).unwrap();
    let diagram = String::from_utf8(out).unwrap();
    let edges: Vec<_> = diagram
        .lines()
        .filter(|line| line.contains("-->"))
        .collect();
    assert_eq!(
        edges,
        ["  src_ui[\"src/ui\"] -->|2| src_core[\"src/core\"]"]
    );
    assert!(!diagram.contains("unused"));
}

fn dead_code_names(files: &[(&str, Lang, &[u8])], page: &TopModifierArgs) -> Vec<String> {
    let (ingest, graph, interner) = index_files(files);
    let sources: Vec<(&str, &[u8])> = files.iter().map(|&(path, _, src)| (path, src)).collect();
    let ctx = DeadCodeContext {
        root: ":memory:",
        sources: &sources.as_slice(),
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
        page,
    };
    let mut out = Vec::new();
    execute_dead_code_query_paged(&ctx, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    let report = roxmltree::Document::parse(&xml).unwrap();
    report
        .descendants()
        .filter(|node| node.has_tag_name("d"))
        .map(|node| node.attribute("n").unwrap().to_string())
        .collect()
}

#[test]
fn dead_code_reports_only_unreferenced_functions_under_each_linkage_rule() {
    let default_page = TopModifierArgs::default();
    type Sources<'a> = &'a [(&'a str, Lang, &'a [u8])];
    let cases: [(Sources, &[&str]); 6] = [
        (
            &[
                ("src/main.c", Lang::C, b"static int unused_helper(void) {\nreturn 1;\n}\nint public_entry(void) {\nreturn 2;\n}\nstatic int used_helper(void) {\nreturn 3;\n}\nint caller(void) {\nreturn used_helper();\n}\n"),
                ("src/header.h", Lang::C, b"static int header_helper(void) {\nreturn 4;\n}\n"),
            ],
            &["unused_helper"],
        ),
        (
            &[(
                "src/lib.c",
                Lang::C,
                b"/* static function */\nint comment_fake_static(void) {\nreturn 10;\n}\n// static int line_fake_static(void)\nint line_fake_static(void) {\nreturn 20;\n}\nstatic\nint\nmultiline_unused(void) {\nreturn 30;\n}\n",
            )],
            &["multiline_unused"],
        ),
        (
            &[(
                "src/inherited.c",
                Lang::C,
                b"static int inherited_helper(void);\n\nint inherited_helper(void) {\nreturn 42;\n}\n",
            )],
            &["inherited_helper"],
        ),
        (
            &[
                ("src/linkage.c", Lang::C, b"static int unrelated(int public_worker);\nint public_worker(void) {\nreturn 1;\n}\nstatic int c_dead(void) {\nreturn 2;\n}\n"),
                ("src/linkage.cpp", Lang::Cpp, b"static int select_value(int value);\nint select_value(double value) {\nreturn 0;\n}\nstatic int cpp_dead(void) {\nreturn 2;\n}\n"),
            ],
            &["c_dead", "cpp_dead"],
        ),
        (
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"fn private_unused() -> u8 {\n    1\n}\npub fn public_api() -> u8 {\n    2\n}\npub(crate) fn crate_api() -> u8 {\n    3\n}\nfn helper_used() -> u8 {\n    4\n}\ntrait Runner {\n    fn run(&self) -> u8;\n}\nstruct Engine;\nimpl Runner for Engine {\n    fn run(&self) -> u8 {\n        5\n    }\n}\n#[test]\nfn checks_engine() {\n    assert_eq!(1, 1);\n}\n#[no_mangle]\nfn plain_hook() -> u8 {\n    6\n}\n#[no_mangle]\nextern \"C\" fn c_hook() -> u8 {\n    7\n}\n#[cfg(test)]\nmod tests {\n    fn fixture_unused() -> u8 {\n        8\n    }\n}\nfn main() {\n    let _ = helper_used();\n}\n",
            )],
            &["private_unused"],
        ),
        (
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"struct Plugin {\n    run: fn() -> u8,\n}\nimpl Plugin {\n    fn method_unused(&self) -> u8 {\n        5\n    }\n}\nfn run() -> u8 {\n    1\n}\nfn helper() -> u8 {\n    2\n}\nfn unused_free() -> u8 {\n    3\n}\nfn plugin() -> Plugin {\n    Plugin { run }\n}\nfn pointer_user() -> u8 {\n    let f = helper;\n    f()\n}\nfn main() {\n    let _ = plugin();\n    let _ = pointer_user();\n}\n",
            )],
            &["unused_free"],
        ),
    ];
    for (files, expected) in cases {
        assert_eq!(dead_code_names(files, &default_page), expected);
    }
}

#[test]
fn dead_code_pagination_selects_expected_slice_of_static_functions() {
    let files = [(
        "src/paged.c",
        Lang::C,
        b"static int first_unused(void) {\nreturn 1;\n}\nstatic int second_unused(void) {\nreturn 2;\n}\nstatic int third_unused(void) {\nreturn 3;\n}\n".as_slice(),
    )];
    let page_at = |offset: usize| TopModifierArgs {
        limit: Some(1),
        offset: Some(offset),
        ..Default::default()
    };
    assert_eq!(dead_code_names(&files, &page_at(0)), ["first_unused"]);
    assert_eq!(dead_code_names(&files, &page_at(1)), ["second_unused"]);
    assert_eq!(dead_code_names(&files, &page_at(2)), ["third_unused"]);
    assert!(dead_code_names(&files, &page_at(3)).is_empty());
}
