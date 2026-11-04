use ripwire_rs::graph::GraphGauges;
use ripwire_rs::navigate::{
    ImpactReport, NavDir, find_symbols_by_name, navigate_1hop, transitive_impact,
};
use ripwire_rs::query::navigation::NavMode;
use ripwire_rs::serialize::{PageRequest, XmlReport};
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol,
};

#[test]
fn test_navigate_1hop_and_impact() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/lib.rs"),
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 500,
    };

    let sym_a = Symbol::new(
        0,
        interner.intern("a"),
        interner.intern("c_a"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let sym_b = Symbol::new(
        1,
        interner.intern("b"),
        interner.intern("c_b"),
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    );
    let sym_c = Symbol::new(
        2,
        interner.intern("c"),
        interner.intern("c_c"),
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );

    let ref_ab = Reference::new(0, interner.intern("b"), 0, 2, Lang::Rust, RefRole::Call);
    let ref_bc = Reference::new(1, interner.intern("c"), 0, 6, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.push(f0);
    ingest.symbols.extend_from_slice(&[sym_a, sym_b, sym_c]);
    ingest.references.extend_from_slice(&[ref_ab, ref_bc]);

    let graph = Graph::build(&ingest, &interner);
    let ranks = vec![0.2, 0.3, 0.5];

    let matched = find_symbols_by_name("b", &ingest, &interner);
    assert_eq!(matched, vec![1]);

    let nav = navigate_1hop(1, &ingest, &graph, &ranks, &interner).unwrap();
    assert_eq!(nav.target.name, "b");
    assert_eq!(nav.callers.len(), 1);
    assert_eq!(nav.callers[0].name, "a");
    assert_eq!(nav.callers[0].sites, vec!["src/lib.rs:2".to_string()]);
    assert_eq!(nav.callees.len(), 1);
    assert_eq!(nav.callees[0].name, "c");

    let callers_xml =
        crate::support::nav::nav_report(&ingest, &graph, &ranks, &interner, "b", NavMode::Callers)
            .unwrap();
    assert!(callers_xml.contains("sites_at=\"src/lib.rs:2\""));

    let impact = transitive_impact(2, &ingest, &graph, &ranks, &interner, None);
    assert_eq!(impact.len(), 2);
    assert_eq!(impact[0].name, "b");
    assert_eq!(impact[0].hops, 1);
    assert_eq!(impact[1].name, "a");
    assert_eq!(impact[1].hops, 2);

    let render = |importers: Vec<&'static str>| {
        let mut out = Vec::new();
        ImpactReport {
            root: ".",
            sym_name: "c",
            defs: Vec::new(),
            nodes: &impact,
            importers,
            declined: Vec::new(),
            gauge_attrs: GraphGauges::ZERO_XML_ATTRS,
            pr_iters: Some(1),
            page: PageRequest::default(),
        }
        .render_envelope(&mut out)
        .unwrap();
        String::from_utf8(out).unwrap()
    };
    let imp_str = render(vec!["src/main.rs"]);
    assert!(imp_str.contains(" importers=\"1\""));
    assert!(imp_str.contains("<f via=\"import\" p=\"src/main.rs\""));
    assert!(!imp_str.contains("<f via=\"import\" p=\"src/lib.rs\""));

    let same_file_str = render(Vec::new());
    assert!(same_file_str.contains(" importers=\"0\""));
    assert!(!same_file_str.contains("<f via="));
}

#[test]
fn test_callers_page_advances_and_points_at_uses() {
    use ripwire_rs::navigate::{NavItemsReport, SymbolNavInfo};
    use ripwire_rs::serialize::{PageRequest, XmlReport};
    let callers: Vec<SymbolNavInfo> = (0..45)
        .map(|i| SymbolNavInfo {
            id: i,
            name: format!("caller_{i}"),
            path: "src/lib.rs".into(),
            line: i + 1,
            kind: "fn",
            rank: 0.0,
            weight: 1.0,
            tested: false,
            sites: Vec::new(),
            of_defs: Vec::new(),
            prov: ripwire_rs::resolve::EdgeProv::Local,
        })
        .collect();
    let render = |page: PageRequest| {
        let mut out = Vec::new();
        NavItemsReport {
            dir: NavDir::Callers,
            root: ".",
            of: "target",
            defs: 2,
            def_rows: &[],
            items: &callers,
            declined: &[],
            gauge_attrs: GraphGauges::ZERO_XML_ATTRS,
            page,
        }
        .render_envelope(&mut out)
        .unwrap();
        String::from_utf8(out).unwrap()
    };
    let all = render(PageRequest::default());
    assert!(all.contains(" defs=\"2\" count=\"45\""), "{all}");
    assert_eq!(all.matches("<s t=").count(), 45, "{all}");
    assert!(all.contains(" next=\"--uses=target\""), "{all}");

    let first = render(PageRequest {
        limit: Some(40),
        ..PageRequest::default()
    });
    assert_eq!(first.matches("<s t=").count(), 40, "{first}");
    assert!(
        first.contains(" next=\"--callers=target --offset=40\" uses_next=\"--uses=target\""),
        "{first}"
    );

    let last = render(PageRequest {
        offset: Some(40),
        ..PageRequest::default()
    });
    assert_eq!(last.matches("<s t=").count(), 5, "{last}");
    assert!(
        last.contains("n=\"caller_40\"") && !last.contains("n=\"caller_39\""),
        "{last}"
    );
    assert!(last.contains(" next=\"--uses=target\""), "{last}");

    let tiny = render(PageRequest {
        budget: Some(1),
        ..PageRequest::default()
    });
    assert_eq!(tiny.matches("<s t=").count(), 1, "{tiny}");
    assert!(
        tiny.contains(" next=\"--callers=target --offset=1\""),
        "{tiny}"
    );
}

#[test]
fn selectors_pick_one_definition_by_file_owner_and_line() {
    let interner = Interner::new();
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[
            (
                "src/a.rs",
                Lang::Rust,
                b"struct A;\nimpl A {\n    fn run(&self) {}\n}\nstruct B;\nimpl B {\n    fn run(&self) {}\n}\n"
                    .as_slice(),
            ),
            ("src/b.rs", Lang::Rust, b"fn run() {}\n".as_slice()),
        ],
        &registry,
        &interner,
    );
    let picked = |selector: &str| -> Vec<String> {
        find_symbols_by_name(selector, &ingest, &interner)
            .into_iter()
            .map(|id| {
                let sym = &ingest.symbols[id as usize];
                let path = interner.resolve(&ingest.files[sym.file_id as usize].path);
                let scope = sym.scope.map_or("", |s| interner.resolve(&s));
                format!("{path}:{}:{scope}", sym.line)
            })
            .collect()
    };
    assert_eq!(picked("src/a.rs::B::run"), ["src/a.rs:7:B"]);
    assert_eq!(picked("./src/a.rs::run"), ["src/a.rs:3:A", "src/a.rs:7:B"]);
    assert_eq!(picked("src/a.rs:7"), ["src/a.rs:7:B"]);
    assert_eq!(picked("src/b.rs:1"), ["src/b.rs:1:"]);
}
