use ripwire_rs::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, MODULE_SCOPE_NAME, NO_NODE,
    PageRankConfig, SymKind, find_affected_tests, ingest_in_memory_files, navigate_1hop,
};

fn ingest(files: &[(&str, Lang, &[u8])]) -> (IngestResult, Interner) {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    (ingest, interner)
}

fn sym_id(ingest: &IngestResult, interner: &Interner, name: &str) -> u32 {
    ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == name)
        .unwrap_or_else(|| panic!("symbol {name}"))
        .id
}

#[test]
fn top_level_calls_get_a_file_scope_caller() {
    let cases: &[(&str, Lang, &[u8])] = &[
        (
            "scripts/lint.py",
            Lang::Python,
            b"def check(x):\n    return x\n\nfor p in [1, 2]:\n    check(p)\n",
        ),
        (
            "web/app.js",
            Lang::JavaScript,
            b"function check(x) { return x; }\n[1, 2].forEach((p) => check(p));\n",
        ),
    ];
    for &(path, lang, src) in cases {
        let (ingest, interner) = ingest(&[(path, lang, src)]);
        let graph = Graph::build(&ingest, &interner);
        let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
        let check = sym_id(&ingest, &interner, "check");

        let nav = navigate_1hop(check, &ingest, &graph, &ranks, &interner).unwrap();
        assert_eq!(nav.callers.len(), 1, "{path}");
        assert_eq!(nav.callers[0].name, MODULE_SCOPE_NAME, "{path}");
        assert_eq!(nav.callers[0].kind, SymKind::ModuleScope.tag(), "{path}");
        assert_eq!(nav.callers[0].path, path);
        assert!(ingest.references.iter().all(|r| r.from_symbol != NO_NODE));
        assert_eq!(ingest.symbol_count(), ingest.symbols.len() - 1, "{path}");

        let mut xml = Vec::new();
        let ctx = NavContext {
            ingest: &ingest,
            graph: &graph,
            ranks: &ranks,
            interner: &interner,
            page: PageRequest::default(),
            max_depth: None,
            pr_iters: None,
        };
        execute_nav_symbol_query_in_root(".", "check", NavMode::Callers, &ctx, &mut xml).unwrap();
        let xml = String::from_utf8(xml).unwrap();
        assert!(
            xml.contains(&format!(
                "t=\"modscope\" n=\"&lt;file-scope&gt;\" p=\"{path}:1\""
            )),
            "{xml}"
        );
    }
}

#[test]
fn file_with_only_definitions_gets_no_owner() {
    let (ingest, _) = ingest(&[(
        "lib/util.py",
        Lang::Python,
        b"def a():\n    b()\n\ndef b():\n    return 1\n",
    )]);
    assert!(
        ingest
            .symbols
            .iter()
            .all(|s| s.kind != SymKind::ModuleScope)
    );
}

#[test]
fn test_module_scope_script_is_affected_and_import_is_not_a_caller() {
    let (ingest, interner) = ingest(&[
        (
            "scripts/lint.py",
            Lang::Python,
            b"def check(x):\n    return x\n",
        ),
        (
            "scripts/test_lint.py",
            Lang::Python,
            b"from lint import check\nassert check(1) == 1\n",
        ),
    ]);
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(&["scripts/lint.py"], &ingest, &graph, &interner);
    let tests: Vec<(&str, u32)> = report
        .tests
        .iter()
        .map(|t| (t.path.as_str(), t.hops))
        .collect();
    assert_eq!(tests, [("scripts/test_lint.py", 1)]);
    {
        let (ingest, interner) = self::ingest(&[
            (
                "src/util/Square.kt",
                Lang::Kotlin,
                b"package com.example.util\nfun square(x: Int): Int = x * x\n",
            ),
            (
                "src/Greeter.kt",
                Lang::Kotlin,
                b"package com.example\nimport com.example.util.square\nfun greet(): Int = square(2)\n",
            ),
        ]);
        let graph = Graph::build(&ingest, &interner);
        let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
        let square = sym_id(&ingest, &interner, "square");
        let nav = navigate_1hop(square, &ingest, &graph, &ranks, &interner).unwrap();
        let callers: Vec<&str> = nav.callers.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(callers, ["greet"]);
    }
}

#[test]
fn test_python_module_alias_qualifier_binds_into_that_module() {
    let other_areas: Vec<(String, &[u8])> = (0..5)
        .map(|i| {
            (
                format!("pkg/shape_{i}.py"),
                b"def area(x):\n    return x\n".as_slice(),
            )
        })
        .collect();
    let mut files: Vec<(&str, Lang, &[u8])> = other_areas
        .iter()
        .map(|(p, src)| (p.as_str(), Lang::Python, *src))
        .collect();
    files.push((
        "pkg/geometry.py",
        Lang::Python,
        b"def area(side):\n    return side * side\n",
    ));
    files.push((
        "pkg/report.py",
        Lang::Python,
        b"import geometry as geo\ndef summarize():\n    return geo.area(1)\n",
    ));
    let (ingest, interner) = ingest(&files);
    let graph = Graph::build(&ingest, &interner);
    let ranks = vec![1.0; ingest.symbols.len()];
    let callers_by_file: Vec<(String, Vec<String>)> = ingest
        .symbols
        .iter()
        .filter(|s| interner.resolve(&s.name) == "area")
        .map(|s| {
            let nav = navigate_1hop(s.id, &ingest, &graph, &ranks, &interner).unwrap();
            (
                ingest.sym_file_path(s, &interner).to_string(),
                nav.callers
                    .iter()
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, callers)| !callers.is_empty())
        .collect();
    assert_eq!(
        callers_by_file,
        [("pkg/geometry.py".to_string(), vec!["summarize".to_string()])]
    );
}

#[test]
fn test_parameter_shadows_same_named_function() {
    let cases: &[(&str, Lang, &[u8], &[&str])] = &[
        (
            "a.py",
            Lang::Python,
            b"def scale(n):\n    return n\n\ndef apply(item, scale):\n    return scale(item)\n\ndef direct():\n    return scale(1)\n",
            &["direct"],
        ),
        (
            "a.js",
            Lang::JavaScript,
            b"function scale(n) { return n; }\nfunction apply(item, scale) { return scale(item); }\nfunction direct() { return scale(1); }\n",
            &["direct"],
        ),
        (
            "a.rs",
            Lang::Rust,
            b"fn scale(n: u32) -> u32 { n }\nfn apply(item: u32, scale: fn(u32) -> u32) -> u32 { scale(item) }\nfn direct() -> u32 { scale(1) }\n",
            &["direct"],
        ),
        (
            "a.rb",
            Lang::Ruby,
            b"def scale(n)\n  n\nend\n\ndef apply(item, scale)\n  scale(item)\nend\n\ndef direct\n  scale(1)\nend\n",
            &["apply", "direct"],
        ),
    ];
    for &(path, lang, src, want) in cases {
        let (ingest, interner) = ingest(&[(path, lang, src)]);
        let graph = Graph::build(&ingest, &interner);
        let ranks = vec![1.0; ingest.symbols.len()];
        let scale = sym_id(&ingest, &interner, "scale");
        let nav = navigate_1hop(scale, &ingest, &graph, &ranks, &interner).unwrap();
        let mut callers: Vec<&str> = nav.callers.iter().map(|c| c.name.as_str()).collect();
        callers.sort_unstable();
        assert_eq!(callers, want, "{path}");
    }
}
