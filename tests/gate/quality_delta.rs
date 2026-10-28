use ripwire_rs::gate::baseline::{
    QualitySnapshot, compute_snapshot, parse_snapshot_from_str, serialize_snapshot_to_string,
};
use ripwire_rs::gate::quality_delta::{
    DeltaCliOptions, QualityDeltaFacts, QualityRegression, StaleAck, ack_matching_regressions,
    apply_cli_delta_options_with_writer, assemble_quality_delta_report, body_text_kinds,
    collect_changed_clone_regressions, compute_quality_delta, compute_quality_delta_with_acks,
    content_id, is_changed_callable, parse_acks_content, resolve_delta_head_sha,
    split_ack_patterns, stale_replay_for, symbol_matches_and_changed,
};
use ripwire_rs::metrics::source_mask::mask_source;
use ripwire_rs::query::commands::QueryContext;
use ripwire_rs::query::safety::{SafetyContext, baseline_from_facts};
use ripwire_rs::{
    CloneGroup, CloneIdiom, CloneType, FileInfo, Graph, IngestResult, Interner, Lang, SymKind,
    Symbol, ToXmlString,
};

fn setup_baseline_repository(interner: &Interner) -> (IngestResult, Graph) {
    let mut ingest = IngestResult::new();

    let p = interner.intern("src/calc.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: p,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });

    let s_name = interner.intern("evaluate");
    let mut sym = Symbol::new(0, s_name, s_name, SymKind::Function, Lang::Rust, 0, 10);
    sym.ccx = 10;
    sym.loc = 25;
    sym.max_nest = 2;
    sym.params = 2;

    ingest.symbols.push(sym);
    let graph = Graph::build(&ingest, interner);
    (ingest, graph)
}

#[test]
fn test_snapshot_computation_and_serialization() {
    let interner = Interner::new();
    let (ingest, graph) = setup_baseline_repository(&interner);

    let snap = compute_snapshot(&ingest, &graph, &interner, "test_sha_123");
    assert_eq!(snap.head_sha, "test_sha_123");
    assert_eq!(snap.ccx_by_sym["src/calc.rs::evaluate"], 10);
    assert_eq!(snap.loc_by_sym["src/calc.rs::evaluate"], 25);
    assert_eq!(snap.nest_by_sym["src/calc.rs::evaluate"], 2);
    assert_eq!(snap.params_by_sym["src/calc.rs::evaluate"], 2);

    let serialized = serialize_snapshot_to_string(&snap);
    assert!(serialized.contains("head test_sha_123"));
    assert!(serialized.contains("ccx src/calc.rs::evaluate 10"));

    let deserialized = parse_snapshot_from_str(&serialized);
    assert_eq!(deserialized.head_sha, snap.head_sha);
    assert_eq!(deserialized.ccx_by_sym, snap.ccx_by_sym);
}

#[test]
fn test_quality_delta_clean_vs_regression() {
    let mut baseline = QualitySnapshot {
        head_sha: "sha_base".to_string(),
        ..Default::default()
    };
    baseline
        .ccx_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 10);
    baseline
        .loc_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 25);

    let mut clean_working = baseline.clone();
    let clean_report = compute_quality_delta(&clean_working, &baseline, "test-base");
    assert_eq!(clean_report.regressions.len(), 0);
    assert_eq!(clean_report.gating_count, 0);
    assert!(!clean_report.has_gating_failures());

    clean_working
        .ccx_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 25);
    clean_working
        .line_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 7);
    let reg_report = compute_quality_delta(&clean_working, &baseline, "test-base");
    assert_eq!(reg_report.regressions.len(), 1);
    assert_eq!(reg_report.gating_count, 1);
    assert!(reg_report.has_gating_failures());

    let reg = &reg_report.regressions[0];
    assert_eq!(reg.kind, "complexity");
    assert_eq!(reg.sym_name, "evaluate");
    assert_eq!(reg.was_val, 10);
    assert_eq!(reg.now_val, 25);
    assert!(reg.is_gating);
    assert!(!reg.is_new_symbol);

    let xml = reg_report.to_xml_string();
    assert!(xml.contains("gating=\"1\""));
    assert!(xml.contains("<r kind=\"complexity\" sym=\"evaluate\" p=\"src/calc.rs:7\" was=\"10\" now=\"25\" gating=\"1\" bar=\"15\"/>"));
}

#[test]
fn test_quality_delta_new_symbol_and_dead_code() {
    let baseline = QualitySnapshot::default();
    let mut working = QualitySnapshot::default();

    working
        .ccx_by_sym
        .insert("src/new.rs::monster".to_string(), 35);
    working
        .dead_symbols
        .insert("src/new.rs::monster".to_string());

    let report = compute_quality_delta(&working, &baseline, "head");
    assert_eq!(report.new_symbol_count, 2);
    assert_eq!(report.gating_count, 0);
    assert!(!report.has_gating_failures());

    let xml = report.to_xml_string();
    assert!(xml.contains("origin=\"new-symbol\""));
}

#[test]
fn test_quality_baseline_rejects_absorbing_findings_unless_allowed() {
    let interner = Interner::new();
    let (ingest, graph) = setup_baseline_repository(&interner);
    let state = SafetyContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    let absorbed = Some((2, "complexity evaluate src/calc.rs:10 was=5 now=10"));
    let error = baseline_from_facts(&state, "fixture-head", absorbed, false)
        .expect_err("refuse to absorb supplied gating findings");
    assert!(error.to_string().contains("2 gating finding(s)"));
    assert!(error.to_string().contains("--allow-dirty"));

    let snapshot = baseline_from_facts(&state, "fixture-head", absorbed, true).unwrap();
    assert_eq!(snapshot.head_sha, "fixture-head");
    assert!(snapshot.dirty);
    assert_eq!(snapshot.absorbed, 2);
    assert_eq!(snapshot.ccx_by_sym["src/calc.rs::evaluate"], 10);

    let baseline = baseline_from_facts(&state, "fixture-head", None, false).unwrap();
    assert!(!baseline.dirty);
    assert_eq!(baseline.absorbed, 0);
    let mut working = baseline.clone();
    working
        .ccx_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 25);
    let report = compute_quality_delta(&working, &baseline, "fixture-head");
    assert!(report.has_gating_failures());
    assert_eq!(report.gating_count, 1);
    assert!(report.to_xml_string().contains(
        "<r kind=\"complexity\" sym=\"evaluate\" p=\"src/calc.rs:10\" was=\"10\" now=\"25\" gating=\"1\" bar=\"15\"/>"
    ));
}

#[test]
fn test_changed_callable_and_clone_regressions_in_memory() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/a.rs"),
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 200,
    });
    let sym = Symbol::new(
        0,
        interner.intern("dup_fn"),
        interner.intern("src/a.rs::dup_fn"),
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    );
    ingest.symbols.push(sym);
    let changed = vec!["src/a.rs".to_string()];
    assert!(is_changed_callable(&sym, "src/a.rs", &changed));
    assert!(!is_changed_callable(&sym, "src/b.rs", &changed));
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let sources = [("src/a.rs", b"fn dup_fn() {}\n".as_slice())];
    let state = QueryContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        sources: &sources,
        commits: None,
        at: None,
    };
    let cg = CloneGroup {
        members: vec![0],
        clone_type: CloneType::Type1,
        token_count: 80,
        similarity: 1.0,
        idiom: CloneIdiom::None,
        tokens_saved: 80,
    };
    let mut regs = Vec::new();
    collect_changed_clone_regressions(vec![cg], &state, &changed, &mut regs);
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0].kind, "duplication");
    assert_eq!(regs[0].line, 5);
}

#[test]
fn test_body_text_kinds_ignore_literals_and_read_comments_for_markers() {
    let kinds = |src: &str, lang: Lang| body_text_kinds(&mask_source(src, lang));

    let quoted = "fn f() { let s = \"TODO: catch {} todo!( std::unique(\"; }";
    assert!(kinds(quoted, Lang::Rust).is_empty());
    assert_eq!(
        kinds("fn f() { // TODO: wire it\n}", Lang::Rust),
        ["placeholder"]
    );
    assert_eq!(kinds("fn f() { todo!() }", Lang::Rust), ["placeholder"]);
    assert_eq!(
        kinds(
            "def f():\n    try:\n        g()\n    except:\n        pass\n",
            Lang::Python
        ),
        ["error-masking"]
    );
    assert_eq!(
        kinds("void f() { try { g(); } catch (...) { } }", Lang::Cpp),
        ["error-masking"]
    );
    assert_eq!(
        kinds("void f() { std::unique(v.begin(), v.end()); }", Lang::Cpp),
        ["defect-shape"]
    );
    assert!(
        kinds(
            "void f() { std::sort(a, b); std::unique(a, b); }",
            Lang::Cpp
        )
        .is_empty()
    );
}

#[test]
fn test_stale_rows_collapse_unless_show_stale() {
    let snap = QualitySnapshot::default();
    let mut report = compute_quality_delta_with_acks(&snap, &snap, "git-HEAD", None);
    let stale = |kind: &str, key: &str| StaleAck {
        kind: kind.to_string(),
        key: key.to_string(),
        why: "finding-gone",
        sym: key.to_string(),
        path: "src/a.rs:1".to_string(),
    };
    report.stale_acks = vec![
        stale("complexity", "k1"),
        stale("dead-code:new-symbol", "k2"),
        stale("complexity", "k3"),
    ];

    report.stale_collapse_replay = Some("--quality-delta --scope=src".to_string());
    let collapsed = report.to_xml_string();
    assert!(collapsed.contains("stale=\"3\""));
    assert!(collapsed.contains("stale_by_kind=\"complexity:2,dead-code:1\""));
    assert!(collapsed.contains("stale_next=\"--quality-delta --scope=src --show-stale\""));
    assert!(!collapsed.contains("<sa "));

    report.stale_collapse_replay = None;
    let full = report.to_xml_string();
    assert_eq!(full.matches("<sa ").count(), 3);
    assert!(!full.contains(" stale_by_kind=\""));
}

#[test]
fn test_ack_only_filter_marks_and_records_matching_rows() {
    let snap = QualitySnapshot::default();
    let mut report = compute_quality_delta_with_acks(&snap, &snap, "git-HEAD", None);
    let row = |kind: &str, sym: &str| QualityRegression {
        kind: kind.to_string(),
        sym_name: sym.to_string(),
        file_path: "src/a.rs".to_string(),
        line: 3,
        was_val: 10,
        now_val: 20,
        is_gating: true,
        is_new_symbol: false,
        renamed_from: None,
    };
    report.regressions = vec![
        row("complexity", "calc"),
        row("nesting", "walk"),
        row("duplication", "calc"),
    ];
    let mut ledger = Vec::new();
    let cid_for = |r: &QualityRegression| (r.kind != "duplication").then_some(0xabcu64);
    ack_matching_regressions(
        &mut report,
        &split_ack_patterns(" complexity ,duplication"),
        "why",
        cid_for,
        &mut ledger,
    )
    .unwrap();
    let text = String::from_utf8(ledger).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("ack complexity calc 20 cid=0000000000000abc now=20"));
    assert!(lines[0].ends_with("p=src/a.rs:3 why"));
    assert!(lines[1].starts_with("ack duplication calc 20 now=20"));
    assert!(!text.contains("cid=0000000000000000"));
    assert_eq!(report.acked_count, 2);
    assert!(!report.regressions[0].is_gating);
    assert!(report.regressions[1].is_gating);
    assert_eq!(stale_replay_for(Some("src")), "--quality-delta --scope=src");
}

#[test]
fn test_content_id_survives_reindent_and_self_rename_only() {
    let base = content_id("fn calc(x: i32) -> i32 {\n    x + 1\n}", "calc");
    let reindented = content_id("fn calc(x: i32) -> i32 {\n\t\tx + 1\n}", "calc");
    let renamed = content_id("fn total(x: i32) -> i32 {\n    x + 1\n}", "total");
    let changed = content_id("fn calc(x: i32) -> i32 {\n    x + 2\n}", "calc");
    assert_eq!(base, reindented);
    assert_eq!(base, renamed);
    assert_ne!(base, changed);
    assert_ne!(base, 0);
}

#[test]
fn test_quality_delta_nesting_and_params_regressions() {
    let mut baseline = QualitySnapshot::default();
    baseline
        .ccx_by_sym
        .insert("src/math.rs::deep".to_string(), 5);
    baseline
        .nest_by_sym
        .insert("src/math.rs::deep".to_string(), 2);
    baseline
        .params_by_sym
        .insert("src/math.rs::deep".to_string(), 2);

    let mut working = QualitySnapshot::default();
    working
        .ccx_by_sym
        .insert("src/math.rs::deep".to_string(), 5);
    working
        .nest_by_sym
        .insert("src/math.rs::deep".to_string(), 7);
    working
        .params_by_sym
        .insert("src/math.rs::deep".to_string(), 6);

    let report = compute_quality_delta(&working, &baseline, "test-baseline");
    assert_eq!(report.regressions.len(), 2);
    assert_eq!(report.gating_count, 2);
    assert!(report.has_gating_failures());

    let kinds: Vec<&str> = report.regressions.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"nesting"));
    assert!(kinds.contains(&"params"));
}

#[test]
fn test_quality_delta_acks_suppression() {
    let mut baseline = QualitySnapshot::default();
    baseline
        .ccx_by_sym
        .insert("src/math.rs::deep".to_string(), 5);
    baseline
        .nest_by_sym
        .insert("src/math.rs::deep".to_string(), 2);

    let mut working = QualitySnapshot::default();
    working
        .ccx_by_sym
        .insert("src/math.rs::deep".to_string(), 5);
    working
        .nest_by_sym
        .insert("src/math.rs::deep".to_string(), 7);

    let acks_content = "ack nesting 70a00a1f5e885d55 10 p=src/math.rs:1 deliberate deep nesting\n";
    let acks = parse_acks_content(acks_content);

    let report = compute_quality_delta_with_acks(&working, &baseline, "test-baseline", Some(&acks));
    assert_eq!(report.regressions.len(), 1);
    assert_eq!(report.gating_count, 0);
    assert!(!report.has_gating_failures());
}

#[test]
fn test_symbol_matches_and_changed() {
    let mut baseline = QualitySnapshot::default();
    baseline
        .ccx_by_sym
        .insert("src/math.rs::calc".to_string(), 10);
    baseline
        .loc_by_sym
        .insert("src/math.rs::calc".to_string(), 20);

    let interner = Interner::new();
    let sym_name = interner.intern("calc");
    let path = interner.intern("src/math.rs");
    let mut sym = Symbol::new(
        0,
        sym_name,
        interner.intern("src/math.rs::calc"),
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    sym.loc = 30;
    sym.ccx = 10;
    let ingest = IngestResult {
        files: vec![FileInfo {
            id: 0,
            path,
            lang: Lang::Rust,
            byte_size: 100,
            line_count: 100,
        }],
        symbols: vec![sym],
        references: Vec::new(),
        ..IngestResult::default()
    };
    let changed = symbol_matches_and_changed(&sym, "src/math.rs", &ingest, &interner, &baseline);
    assert!(changed.is_some());
}

#[test]
fn test_resolve_delta_head_sha_variants() {
    assert_eq!(resolve_delta_head_sha("1234567+dirty"), "1234567");
    assert_eq!(resolve_delta_head_sha(""), "HEAD");
    assert_eq!(resolve_delta_head_sha("abcdef1"), "abcdef1");
}

#[test]
fn test_apply_cli_delta_options_with_in_memory_writer() {
    let interner = Interner::new();
    let (ingest, graph) = setup_baseline_repository(&interner);
    let sources: [(&str, &[u8]); 0] = [];
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let qc = QueryContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        commits: None,
        sources: &sources,
        at: None,
    };

    let snap = compute_snapshot(&ingest, &graph, &interner, "HEAD");
    let mut report = compute_quality_delta_with_acks(&snap, &snap, "git-HEAD", None);
    report.regressions.push(QualityRegression {
        kind: "complexity".to_string(),
        sym_name: "evaluate".to_string(),
        file_path: "src/calc.rs".to_string(),
        line: 10,
        was_val: 1,
        now_val: 10,
        is_gating: true,
        is_new_symbol: false,
        renamed_from: None,
    });
    report.gating_count = 1;

    let mut written_bytes = Vec::new();
    let opts = DeltaCliOptions {
        scope: Some("src/calc.rs"),
        quality_ack: Some("testing ack writer"),
        ack_only: Some("src/calc.rs"),
        show_stale: false,
        base: None,
    };

    let res =
        apply_cli_delta_options_with_writer(&qc, &mut report, opts, || Ok(&mut written_bytes));
    assert!(res.is_ok());
    assert!(report.stale_collapse_replay.is_some());
    assert_eq!(report.gating_count, 0, "regression should be acknowledged");
    assert!(
        !written_bytes.is_empty(),
        "ack ledger entry should be written"
    );
}

#[test]
fn test_assemble_quality_delta_report_from_facts() {
    let interner = Interner::new();
    let (ingest, graph) = setup_baseline_repository(&interner);
    let calc_code = b"pub fn evaluate(a: i32, b: i32) -> i32 { a + b }\n".as_slice();
    let sources = [("src/calc.rs", calc_code)];
    let commit = ripwire_rs::git::history::CommitInfo {
        id: "c1".to_string(),
        author: "test".to_string(),
        timestamp: 1000,
        changed_files: vec!["src/calc.rs".to_string()],
    };
    let commits = vec![commit.clone(), commit];
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let qc = QueryContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        commits: Some(&commits),
        sources: &sources,
        at: None,
    };

    let snap = compute_snapshot(&ingest, &graph, &interner, "HEAD");
    let mut baseline = snap.clone();
    baseline
        .ccx_by_sym
        .insert("src/calc.rs::evaluate".to_string(), 5);
    let changed_files = vec!["src/calc.rs".to_string()];
    let facts = QualityDeltaFacts {
        working: &snap,
        baseline: &baseline,
        source_name: "git-HEAD",
        head_basis: Some("identity"),
        at_stamp: "abc1234",
        changed_files: &changed_files,
        acks: None,
        clones: &[],
    };
    let report = assemble_quality_delta_report(&qc, &facts);
    assert_eq!(report.head_basis, Some("identity"));
    assert_eq!(report.baseline_source, "git-HEAD");
    assert_eq!(report.at_stamp, "abc1234");
    assert_eq!(report.gating_count, 0);
    assert!(
        report
            .regressions
            .iter()
            .any(|r| r.kind == "short-horizon-churn")
    );
    assert!(report.regressions.iter().any(|r| r.kind == "complexity"));
}

#[test]
fn test_churn_regression_reports_every_preexisting_callable_in_file() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/calc.rs"),
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });
    for (id, name, line) in [(0, "evaluate", 10), (1, "combine", 22)] {
        let name = interner.intern(name);
        let mut sym = Symbol::new(id, name, name, SymKind::Function, Lang::Rust, 0, line);
        sym.ccx = 10;
        sym.loc = 25;
        ingest.symbols.push(sym);
    }
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let calc_code = b"pub fn evaluate() {}\npub fn combine() {}\n".as_slice();
    let sources = [("src/calc.rs", calc_code)];
    let commit = ripwire_rs::git::history::CommitInfo {
        id: "c1".to_string(),
        author: "test".to_string(),
        timestamp: 1000,
        changed_files: vec!["src/calc.rs".to_string()],
    };
    let commits = vec![commit.clone(), commit];
    let qc = QueryContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
        commits: Some(&commits),
        sources: &sources,
        at: None,
    };
    let snap = compute_snapshot(&ingest, &graph, &interner, "HEAD");
    let mut baseline = snap.clone();
    for ccx in baseline.ccx_by_sym.values_mut() {
        *ccx += 1;
    }
    let changed_files = vec!["src/calc.rs".to_string()];
    let facts = QualityDeltaFacts {
        working: &snap,
        baseline: &baseline,
        source_name: "git-HEAD",
        head_basis: Some("identity"),
        at_stamp: "abc1234",
        changed_files: &changed_files,
        acks: None,
        clones: &[],
    };
    let report = assemble_quality_delta_report(&qc, &facts);
    let mut churned: Vec<&str> = report
        .regressions
        .iter()
        .filter(|r| r.kind == "short-horizon-churn")
        .map(|r| r.sym_name.as_str())
        .collect();
    churned.sort_unstable();
    assert_eq!(churned, ["combine", "evaluate"]);
}

#[test]
fn quality_report_preserves_borrowed_type_names_and_quoted_paths() {
    let key = "src/a&\"b.rs::&View<T>::item";
    let mut baseline = QualitySnapshot::default();
    baseline.ccx_by_sym.insert(key.into(), 10);
    let mut working = baseline.clone();
    working.dead_symbols.insert(key.into());
    working.line_by_sym.insert(key.into(), 7);
    let mut report = compute_quality_delta(&working, &baseline, "base<&");
    report.at_stamp = "revision<&".into();
    report.stale_acks.push(StaleAck {
        kind: "complexity".into(),
        key: key.into(),
        why: "missing",
        sym: "&View<T>::item".into(),
        path: "src/a&\"b.rs".into(),
    });
    let xml = report.to_xml_string();
    let document = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(
        document.root_element().attribute("baseline"),
        Some("base<&")
    );
    assert_eq!(document.root_element().attribute("at"), Some("revision<&"));
    let regression = document
        .descendants()
        .find(|node| node.has_tag_name("r"))
        .unwrap();
    assert_eq!(regression.attribute("kind"), Some("dead-code"));
    assert_eq!(regression.attribute("sym"), Some(key));
    assert_eq!(regression.attribute("p"), Some("src/a&\"b.rs:7"));
    let stale = document
        .descendants()
        .find(|node| node.has_tag_name("sa"))
        .unwrap();
    assert_eq!(stale.attribute("key"), Some(key));
    assert_eq!(stale.attribute("sym"), Some("&View<T>::item"));
    assert_eq!(stale.attribute("p"), Some("src/a&\"b.rs"));
}

#[test]
fn test_quality_delta_report_names_its_merge_base_only_when_measured_from_a_ref() {
    let baseline = QualitySnapshot {
        head_sha: "sha_base".to_string(),
        ..Default::default()
    };
    let header = |base_ref: Option<&str>| {
        let mut report = compute_quality_delta(&baseline, &baseline, "git-ref");
        report.base_ref = base_ref.map(str::to_string);
        let xml = report.to_xml_string();
        let doc = roxmltree::Document::parse(&xml).expect("well-formed xml");
        doc.root_element().attribute("base_ref").map(str::to_string)
    };
    assert_eq!(
        (header(Some("a1b2c3d4e")), header(None)),
        (Some("a1b2c3d4e".to_string()), None)
    );
}
