use ripwire_rs::{
    CommunityModule, FileInfo, Graph, IngestResult, Interner, Lang, PageRankResult,
    QualitySnapshot, RefRole, Reference, SerializeOptions, SymKind, Symbol, ToXmlString,
    TreeFileSymbols, escape_xml_str, serialize_around_xml, serialize_communities_xml,
    serialize_connect_xml, serialize_dead_code_xml, serialize_deps_xml, serialize_doc_drift_xml,
    serialize_exemplar_xml, serialize_exercises_xml, serialize_expand_xml,
    serialize_external_surface_xml, serialize_flags_xml, serialize_for_xml,
    serialize_from_trace_xml, serialize_grep_xml, serialize_lego_xml, serialize_mentions_xml,
    serialize_owners_xml, serialize_pack_top_n_xml, serialize_path_between_xml,
    serialize_recall_text, serialize_safe_delete_xml, serialize_stray_content_xml,
    serialize_test_gate_xml, serialize_whereis_xml, serialize_xml_to_string,
};
use serde_json::json;

fn validate_xml_well_formed(raw_xml: &str) {
    let cleaned = ripwire_rs::serialize::strip_xml_comments(raw_xml).unwrap();
    let xml = cleaned.as_str();
    let mut tag_stack: Vec<&str> = Vec::new();
    let mut i = 0;
    let bytes = xml.as_bytes();
    let mut saw_root = false;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            let end = xml[i..].find('>').expect("unclosed tag") + i;
            let tag_content = &xml[i + 1..end];
            if let Some(closing_tag) = tag_content.strip_prefix('/') {
                let expected = tag_stack.pop().expect("unexpected closing tag");
                assert_eq!(closing_tag, expected, "closing tag mismatch");
            } else if tag_content.ends_with('/') {
                assert!(tag_content.starts_with("c "));
            } else {
                let tag_name = tag_content
                    .split_whitespace()
                    .next()
                    .expect("empty tag name");
                if tag_name == "r" {
                    saw_root = true;
                }
                tag_stack.push(tag_name);
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    assert!(tag_stack.is_empty(), "unclosed tags: {:?}", tag_stack);
    assert!(saw_root, "Must emit <r> root tag");
}

#[test]
fn test_streaming_xml_serializer() {
    let interner = Interner::new();

    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/app & test.rs"),
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1200,
    };
    let f1 = FileInfo {
        id: 1,
        path: interner.intern("src/util.rs"),
        lang: Lang::Rust,
        line_count: 30,
        byte_size: 600,
    };

    let name_main = interner.intern("main");
    let name_helper = interner.intern("compute<T> & log");
    let name_util = interner.intern("init \"core\"");
    let canon = interner.intern("canon");

    let sym_main = Symbol::new(0, name_main, canon, SymKind::Function, Lang::Rust, 0, 10);
    let sym_helper = Symbol::new(1, name_helper, canon, SymKind::Function, Lang::Rust, 0, 20);
    let sym_util = Symbol::new(2, name_util, canon, SymKind::Function, Lang::Rust, 1, 5);

    let ref01 = Reference::new(0, name_helper, 0, 11, Lang::Rust, RefRole::Call);
    let ref12 = Reference::new(1, name_util, 0, 21, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.extend_from_slice(&[f0, f1]);
    ingest
        .symbols
        .extend_from_slice(&[sym_main, sym_helper, sym_util]);
    ingest.references.extend_from_slice(&[ref01, ref12]);

    let graph = Graph::build(&ingest, &interner);
    let pr = PageRankResult {
        iterations: 15,
        converged: true,
        ranks: vec![0.20, 0.35, 0.45],
    };

    let options = SerializeOptions {
        root: ".".into(),
        top_k: None,
        include_pr: true,
        ..Default::default()
    };

    let xml = serialize_xml_to_string(&ingest, &graph, &pr, &interner, &options);
    validate_xml_well_formed(&xml);

    assert!(xml.contains("p=\"src/app &amp; test.rs\""));
    assert!(xml.contains("n=\"compute&lt;T&gt; &amp; log\""));
    assert!(xml.contains("n=\"init &quot;core&quot;\""));
    assert!(!xml.contains("&amp;amp;"));
    assert!(!xml.contains("&amp;lt;"));
    assert!(!xml.contains("&amp;gt;"));

    let pos_f1 = xml.find("p=\"src/util.rs\"").unwrap();
    let pos_f0 = xml.find("p=\"src/app &amp; test.rs\"").unwrap();
    assert!(
        pos_f1 < pos_f0,
        "File with highest rank symbol must appear first"
    );

    let pos_helper = xml.find("n=\"compute&lt;T&gt; &amp; log\"").unwrap();
    let pos_main = xml.find("n=\"main\"").unwrap();
    assert!(
        pos_helper < pos_main,
        "Higher-rank symbol must appear before lower-rank symbol in file"
    );

    assert!(
        xml.contains(
            "<s t=\"fn\" n=\"main\" k=\"0.2000\"><c n=\"compute&lt;T&gt; &amp; log\"/></s>"
        )
    );

    let pr_unconverged = PageRankResult {
        iterations: 100,
        converged: false,
        ranks: vec![0.20, 0.35, 0.45],
    };
    let xml_unconverged =
        serialize_xml_to_string(&ingest, &graph, &pr_unconverged, &interner, &options);
    assert!(xml_unconverged.contains("pr_converged=\"0\""));

    let top1_opts = SerializeOptions {
        root: ".".into(),
        top_k: Some(1),
        include_pr: true,
        ..Default::default()
    };
    let xml_top1 = serialize_xml_to_string(&ingest, &graph, &pr, &interner, &top1_opts);
    assert!(xml_top1.contains("p=\"src/util.rs\""));
    assert!(!xml_top1.contains("src/app &amp; test.rs"));

    let clean = "clean_func_name";
    let cow = escape_xml_str(clean);
    assert!(matches!(cow, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn test_streaming_xml_layer_test_prov_split_and_shown() {
    let interner = Interner::new();
    let paths = ["lib/core.rs", "tests/t1.rs", "tests/t2.rs"];
    let mut files = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        files.push(FileInfo {
            id: i as u32,
            path: interner.intern(p),
            lang: Lang::Rust,
            line_count: 10 * (i as u32 + 1),
            byte_size: 100 * (i as u64 + 1),
        });
    }

    let mut syms = Vec::new();
    for (i, (name, fid)) in [("root", 0), ("target", 2), ("target", 2), ("caller", 1)]
        .iter()
        .enumerate()
    {
        syms.push(Symbol::new(
            i as u32,
            interner.intern(name),
            interner.intern(name),
            SymKind::Function,
            Lang::Rust,
            *fid,
            i as u32 * 10,
        ));
    }

    let ref_call = Reference::new(
        3,
        interner.intern("target"),
        1,
        35,
        Lang::Rust,
        RefRole::Call,
    );
    let mut ingest = IngestResult::new();
    ingest.files = files;
    ingest.symbols = syms;
    ingest.references.push(ref_call);

    let graph = Graph::build(&ingest, &interner);
    let pr = PageRankResult {
        iterations: 5,
        converged: true,
        ranks: vec![0.25; 4],
    };

    let options = SerializeOptions {
        root: ".".into(),
        emit_legend: true,
        ..Default::default()
    };

    let xml = serialize_xml_to_string(&ingest, &graph, &pr, &interner, &options);
    validate_xml_well_formed(&xml);

    assert!(xml.starts_with("<!-- ripwire-rs map schema=ripwire-rs.map/v1"));
    assert!(xml.contains("<f p=\"tests/t1.rs\" layer=\"test\">"));
    assert!(xml.contains("<f p=\"lib/core.rs\">"));
    assert!(xml.contains("shown=\"4\""));
    assert!(xml.contains("<c n=\"target\" prov=\"split\"/>"));
}

#[test]
fn test_query_xml_retrieval_and_git() {
    let mut buf = Vec::new();

    let val_for = json!({
        "task": "authenticate user",
        "symbols": [{"name": "auth", "path": "src/auth.rs", "line": 10, "score": 2.0, "kind": "fn"}]
    });
    serialize_for_xml(&val_for, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.for/v1\""));
    buf.clear();

    let val_grep = json!({
        "pattern": "login",
        "count": 1,
        "hits": [{"file": "src/main.rs", "line": 42, "enclosing": "main", "text": "login()"}]
    });
    serialize_grep_xml(&val_grep, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.grep/v1\""));
    buf.clear();

    let val_whereis = json!({
        "symbol": "auth",
        "hits": [
            {"ref": "HEAD", "path": "src/auth.rs", "line": 1, "kind": "def", "text": "fn auth()"},
            {"ref": "feat-1", "path": "src/auth.rs", "line": 1, "kind": "def", "text": "fn auth()"},
            {"ref": "feat-2", "path": "src/auth.rs", "line": 1, "kind": "def", "text": "fn auth()"}
        ]
    });
    serialize_whereis_xml(&val_whereis, &mut buf).unwrap();
    let whereis_xml = String::from_utf8_lossy(&buf);
    assert!(whereis_xml.contains("schema=\"ripwire-rs.whereis/v1\""));
    assert!(whereis_xml.contains("folded=\"1\""));
    buf.clear();

    let val_owners = json!({
        "files": [{"path": "src/auth.rs", "authors": 1, "top_author": "Alice", "top_share": 1.0}]
    });
    serialize_owners_xml(&val_owners, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.owners/v1\""));
    buf.clear();

    let val_flags = json!({
        "gates": [{"name": "FEATURE_X", "kind": "env", "file": "src/lib.rs", "line": 5}]
    });
    serialize_flags_xml(&val_flags, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.flags/v1\""));
    buf.clear();

    let val_doc = json!({
        "drift": [{"doc": "docs/arch.md", "line": 12, "referenced": "old_api"}]
    });
    serialize_doc_drift_xml(&val_doc, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.doc-drift/v1\""));
    buf.clear();

    let val_stray = json!({
        "branches": [{"branch": "feature", "unmerged_commits": 1}]
    });
    serialize_stray_content_xml(&val_stray, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.stray-content/v1\""));
    buf.clear();

    let val_mentions = json!({
        "symbol": "auth",
        "docs": [{"path": "README.md", "mentions": 3}]
    });
    serialize_mentions_xml(&val_mentions, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.mentions/v1\""));
}

#[test]
fn test_stray_content_xml_lists_capped_file_rows_with_more_count() {
    let files: Vec<serde_json::Value> = (0..14)
        .map(|i| {
            json!({"p": format!("src/f{i:02}.rs"), "verdict": "unmerged", "stray": 14 - i,
                   "authored": 20, "del": 0, "redone": 0, "sim": 0.3, "head_touched": true})
        })
        .collect();
    let val = json!({
        "branches": [{"name": "feature", "tip": "abc123", "date": "2026-10-22",
                      "base": "base1", "verdict": "unmerged", "stray": 105,
                      "superseded": 0, "files": files}]
    });
    let mut buf = Vec::new();
    serialize_stray_content_xml(&val, &mut buf).unwrap();
    let xml = String::from_utf8_lossy(&buf);
    assert!(xml.contains("<ref name=\"feature\" tip=\"abc123\" date=\"2026-10-22\" base=\"base1\" ok=\"1\" v=\"unmerged\" stray=\"105\" files=\"14\" superseded=\"0\">"));
    assert_eq!(xml.matches("<file ").count(), 12);
    assert!(xml.contains("<more files=\"2\"/>"));
    assert!(xml.contains("sim=\"0.30\" head-touched=\"1\""));
}

#[test]
fn test_query_xml_detail_ladder() {
    let mut buf = Vec::new();
    let interner = Interner::new();
    let sym = Symbol::new(
        0,
        interner.intern("run"),
        interner.intern("run"),
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    serialize_expand_xml(
        ".",
        &[(&sym, "run", "src/main.rs", "fn run() {}")],
        &mut buf,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.expand/v1\""));
    buf.clear();

    let tree_files: Vec<TreeFileSymbols> = ["src/a.rs", "src/b.rs", "src/c.rs"]
        .into_iter()
        .map(|path| TreeFileSymbols {
            path,
            total_symbols: 1,
            top_symbols: vec![("fn", "run")],
        })
        .collect();
    use ripwire_rs::serialize::XmlReport;
    ripwire_rs::serialize::query_xml::TreeReport {
        root: ".",
        file_symbols: &tree_files,
        files_unlisted: 0,
        pr_iters: 16,
        page: ripwire_rs::serialize::PageRequest {
            limit: Some(1),
            offset: Some(1),
            budget: None,
        },
    }
    .render_envelope(&mut buf)
    .unwrap();
    let xml = String::from_utf8_lossy(&buf).into_owned();
    assert!(xml.contains("<file p=\"src/b.rs\""), "{xml}");
    assert!(
        !xml.contains("src/a.rs") && !xml.contains("src/c.rs"),
        "{xml}"
    );
    assert!(xml.contains(" next=\"--tree --offset=2\""), "{xml}");
    buf.clear();

    serialize_pack_top_n_xml(".", &[("src/main.rs", "fn run() {}")], &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.pack-top-n/v1\""));
}

#[test]
fn test_query_xml_safety_family() {
    let mut buf = Vec::new();

    let tests = [ripwire_rs::gate::affected::AffectedTest::new(
        "tests/foo.rs".into(),
        1,
        false,
        false,
    )];
    serialize_test_gate_xml(".", &tests, 2, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.test-gate/v1\""));
    buf.clear();

    let report = ripwire_rs::serialize::arch_xml::SafeDeleteReport {
        root: ".",
        sym_name: "sym",
        kind_tag: "fn",
        def_loc: "src/lib.rs:1",
        callers: &[("src/lib.rs", "caller")],
        defs: 1,
        ambiguous_callers: 0,
        uses: 1,
        tested_self: 0,
        impact_reaches: 3,
        radius_tested: 0,
        radius_untested: 3,
        page: ripwire_rs::serialize::PageRequest::default(),
        gauge_attrs: ripwire_rs::serialize::GraphGauges::ZERO_XML_ATTRS,
    };
    serialize_safe_delete_xml(&report, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.safe-delete/v1\""));
    buf.clear();

    serialize_exercises_xml(".", "tests/foo.rs", &[("src/lib.rs", "target")], &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.exercises/v1\""));
}

#[test]
fn test_query_xml_architecture_family() {
    let mut buf = Vec::new();

    serialize_dead_code_xml(".", &[("unused", "fn", "src/lib.rs", 10)], &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.dead-code/v1\""));
    buf.clear();

    let mods = [CommunityModule {
        id: 0,
        size: 1,
        dir: "src",
        label: "core",
        members: vec![("fn", "run", "src/lib.rs", 1)],
    }];
    serialize_communities_xml(".", &mods, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.communities/v1\""));
    buf.clear();

    serialize_around_xml(
        ".",
        "run",
        1,
        32,
        &[("run", "fn", "src/lib.rs", 1)],
        &mut buf,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.around/v1\""));
    buf.clear();

    serialize_external_surface_xml(
        ".",
        &[("extern_fn", "rs", 5)],
        ripwire_rs::serialize::PageRequest::default(),
        &mut buf,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.external-surface/v1\""));
    buf.clear();

    serialize_deps_xml(".", &[("src/lib.rs", 2, 1)], &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.deps/v1\""));
    buf.clear();

    let deps_rep = ripwire_rs::serialize::DepsReport {
        corpus_files: 2,
        dep_files: 2,
        ccd: 4,
        acd: 2.0,
        nccd: 1.1,
        rows: vec![
            ripwire_rs::serialize::DepsFileRow {
                path: "src/a.rs",
                includes: 1,
                efferent: 1,
                afferent: 1,
                transitive: 2,
                inc_targets: vec!["./b.rs"],
            },
            ripwire_rs::serialize::DepsFileRow {
                path: "src/b.rs",
                includes: 1,
                efferent: 1,
                afferent: 1,
                transitive: 2,
                inc_targets: Vec::new(),
            },
        ],
        cycles: vec![vec!["src/a.rs", "src/b.rs"]],
        violations: vec![("src/a.rs", "src/b.rs", 0.25)],
        page: ripwire_rs::serialize::PageRequest::default(),
    };
    ripwire_rs::serialize::serialize_deps_report_xml(".", &deps_rep, &mut buf).unwrap();
    let deps_xml = String::from_utf8_lossy(&buf);
    assert!(deps_xml.contains("<cycles total=\"1\" shown=\"1\" capped=\"0\">"));
    assert!(deps_xml.contains("<cycle size=\"2\" cost=\"4\""));
    assert!(deps_xml.contains("<inc t=\"./b.rs\"/>"));
}

#[test]
fn dead_code_list_follows_output_budget() {
    use ripwire_rs::serialize::{
        GraphGauges, PageRequest, arch_xml::serialize_dead_code_paged_xml,
    };
    let items: Vec<(&str, &'static str, &str, u32)> =
        (0..5).map(|i| ("unused", "fn", "src/lib.rs", i)).collect();
    let render = |page: PageRequest| {
        let mut buf = Vec::new();
        serialize_dead_code_paged_xml(".", &items, GraphGauges::default(), page, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    };

    let tiny = render(PageRequest {
        budget: Some(1),
        ..PageRequest::default()
    });
    assert_eq!(tiny.matches("<d n=\"").count(), 1, "{tiny}");
    assert!(
        tiny.contains("shown=\"1\" capped=\"1\" total=\"5\" has_more=\"1\""),
        "{tiny}"
    );

    let all = render(PageRequest {
        budget: Some(1 << 20),
        ..PageRequest::default()
    });
    assert_eq!(all.matches("<d n=\"").count(), 5, "{all}");
    assert!(all.contains("capped=\"0\""), "{all}");

    let limited = render(PageRequest {
        limit: Some(3),
        budget: Some(1),
        ..PageRequest::default()
    });
    assert_eq!(limited.matches("<d n=\"").count(), 3, "{limited}");
}

#[test]
fn test_query_xml_task_retrieval_family() {
    let mut buf = Vec::new();

    let val_ex = json!({
        "kind": "fn",
        "exemplar": { "name": "run", "path": "src/main.rs", "line": 1, "body": "fn run() {}", "ccx": 0, "callers": 1 }
    });
    serialize_exemplar_xml(".", &val_ex, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.exemplar/v1\""));
    buf.clear();

    let val_lego = json!({
        "interface": "ToXmlString",
        "implementors": [{ "name": "AffectedReport", "path": "src/gate/affected.rs" }]
    });
    serialize_lego_xml(".", &val_lego, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.lego/v1\""));
    buf.clear();

    let val_recall = json!({
        "relevant": 1,
        "documents": 1,
        "docs": [{ "path": "README.md", "score": 0.5, "content": "context router\n" }]
    });
    serialize_recall_text("router", &val_recall, &mut buf).unwrap();
    assert!(
        String::from_utf8_lossy(&buf)
            .contains("━━ README.md  (relevance 0.500) ━━\ncontext router\n")
    );
    buf.clear();

    let val_trace = json!({
        "suspects": [{ "name": "run", "path": "src/main.rs", "line": 10 }]
    });
    serialize_from_trace_xml(&val_trace, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.from-trace/v1\""));
}

#[test]
fn test_query_xml_graph_paths() {
    let mut buf = Vec::new();

    let val_connect = json!({
        "symbols": ["a", "b"],
        "edges": [{"from": "a", "to": "b"}]
    });
    serialize_connect_xml(&val_connect, &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("schema=\"ripwire-rs.connect/v1\""));
    buf.clear();

    let val_path_reach = json!({
        "from": "a",
        "to": "b",
        "reachable": true,
        "hops": 1,
        "path": ["a", "b"]
    });
    serialize_path_between_xml(&val_path_reach, &mut buf).unwrap();
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains(
        "<path schema=\"ripwire-rs.path/v1\" from=\"a\" to=\"b\" from_defs=\"1\" to_defs=\"1\" reachable=\"1\" hops=\"1\">"
    ));
    buf.clear();

    let val_path_unreach = json!({
        "from": "x",
        "to": "y",
        "reachable": false,
        "hops": 0
    });
    serialize_path_between_xml(&val_path_unreach, &mut buf).unwrap();
    let s_unreach = String::from_utf8_lossy(&buf);
    assert!(s_unreach.contains(
        "<path schema=\"ripwire-rs.path/v1\" from=\"x\" to=\"y\" from_defs=\"1\" to_defs=\"1\" reachable=\"0\" hops=\"0\""
    ));
    assert!(s_unreach.contains("hint="));
}

#[test]
fn test_page_window_and_xml_report_trait() {
    use ripwire_rs::serialize::{PageWindow, XmlReport};

    struct SampleReport {
        items: Vec<&'static str>,
        pw: PageWindow,
    }

    impl XmlReport for SampleReport {
        fn schema_tag(&self) -> &'static str {
            "sample"
        }
        fn schema_version(&self) -> &'static str {
            "ripwire-rs.sample/v1"
        }
        fn root_path(&self) -> &str {
            "pkg/sub"
        }
        fn legend_comment(&self) -> Option<&'static str> {
            Some("ripwire-rs sample schema=ripwire-rs.sample/v1")
        }
        fn page_window(&self) -> Option<PageWindow> {
            Some(self.pw)
        }
        fn write_body<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
            for &item in &self.items[self.pw.begin..self.pw.end] {
                writeln!(out, "  <item n=\"{item}\"/>")?;
            }
            Ok(())
        }
    }

    let items = vec!["a", "b", "c", "d", "e"];
    let budget = ripwire_rs::serialize::PageRequest {
        budget: Some(4),
        ..Default::default()
    };
    let pw = budget.window_fit(items.len(), |i| {
        ripwire_rs::serialize::text_bytes(&[items[i]])
    });
    assert_eq!(pw.shown, 2);
    assert!(pw.capped);
    assert!(pw.has_more);
    assert_eq!(pw.next_offset, 2);

    let rep = SampleReport { items, pw };
    let mut buf = Vec::new();
    rep.render_envelope(&mut buf).unwrap();
    let xml = String::from_utf8(buf).unwrap();
    assert!(xml.contains("<!-- ripwire-rs sample schema=ripwire-rs.sample/v1 -->"));
    assert!(xml.contains("<sample schema=\"ripwire-rs.sample/v1\" root=\"pkg/sub\" shown=\"2\" capped=\"1\" total=\"5\" has_more=\"1\" next_offset=\"2\" offset=\"0\" limit=\"0\">"));
    assert!(xml.contains("<item n=\"a\"/>"));
    assert!(xml.contains("<item n=\"b\"/>"));
    assert!(!xml.contains("<item n=\"c\"/>"));

    let ingest = IngestResult::new();
    let interner = Interner::new();
    let graph = Graph::build(&ingest, &interner);
    let qp = ripwire_rs::metrics::compute_quality_panel_report(
        &ingest,
        &graph,
        &interner,
        ripwire_rs::metrics::PanelInputs {
            preset: None,
            history: Err("in-memory ingest: no git history".to_string()),
        },
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(qp.to_xml_string().contains("ripwire-rs.quality-panel/v1"));
    let cr = ripwire_rs::metrics::compute_context_ratio(
        &ingest,
        &graph,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(cr.to_xml_string().contains("ripwire-rs.context-ratio/v1"));
    let ens = ripwire_rs::metrics::compute_ensemble(
        &ingest,
        &graph,
        &interner,
        Err("in-memory ingest: no git history".to_string()),
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(ens.to_xml_string().contains("ripwire-rs.ensemble/v1"));
    let cc = ripwire_rs::metrics::compute_comment_coherence(
        &ingest,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(
        cc.to_xml_string()
            .contains("ripwire-rs.comment-coherence/v1")
    );
    let seams = ripwire_rs::gate::seams::find_integration_seams(
        &ingest,
        &graph,
        &interner,
        Default::default(),
    );
    assert!(seams.to_xml_string().contains("ripwire-rs.seams/v1"));
    let scout = ripwire_rs::gate::merge_scout::evaluate_merge_scout("abc123def", "abc123def", &[]);
    assert!(scout.to_xml_string().contains("ripwire-rs.merge-scout/v1"));
    let pr_req = ripwire_rs::gate::pr_context::PrContextRequest {
        changed_lines: None,
        changed_files: &[],
        base_ref: None,
        at_stamp: "",
        commits: &[],
        page: ripwire_rs::serialize::PageRequest::default(),
    };
    let pr_ctx = ripwire_rs::gate::pr_context::compute_pr_context_from_facts(
        &pr_req, &ingest, &graph, &interner,
    );
    assert!(pr_ctx.to_xml_string().contains("ripwire-rs.pr-context/v1"));
    let snap = QualitySnapshot::default();
    let dmm = ripwire_rs::metrics::compute_dmm_report(&snap, &snap, "worktree", "HEAD");
    assert!(dmm.to_xml_string().contains("ripwire-rs.dmm/v1"));
}
