use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, LanguageRegistry, RefRole, Reference, SymKind,
    Symbol, ToXmlString, find_affected_tests, find_affected_tests_for_symbols,
    ingest_in_memory_files, is_test_path,
};

#[test]
fn test_is_test_path_detection() {
    assert!(is_test_path("tests/foo.rs"));
    assert!(is_test_path("src/lib_test.go"));
    assert!(is_test_path("ui/component.test.ts"));
    assert!(is_test_path("core/test_engine.rs"));
    assert!(!is_test_path("src/lib.rs"));
    assert!(!is_test_path("src/engine.rs"));
}

#[test]
fn test_affected_transitive_reachability() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();

    let p_core = interner.intern("src/core.rs");
    let p_service = interner.intern("src/service.rs");
    let p_test = interner.intern("tests/test_service.rs");

    ingest.files.push(FileInfo {
        id: 0,
        path: p_core,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });
    ingest.files.push(FileInfo {
        id: 1,
        path: p_service,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });
    ingest.files.push(FileInfo {
        id: 2,
        path: p_test,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });

    let s0 = Symbol::new(
        0,
        interner.intern("core_fn"),
        interner.intern("core_fn"),
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    );
    let s1 = Symbol::new(
        1,
        interner.intern("service_fn"),
        interner.intern("service_fn"),
        SymKind::Function,
        Lang::Rust,
        1,
        10,
    );
    let s2 = Symbol::new(
        2,
        interner.intern("test_fn"),
        interner.intern("test_fn"),
        SymKind::Function,
        Lang::Rust,
        2,
        15,
    );

    ingest.symbols.push(s0);
    ingest.symbols.push(s1);
    ingest.symbols.push(s2);

    ingest.references.push(Reference::new(
        1,
        interner.intern("core_fn"),
        1,
        12,
        Lang::Rust,
        RefRole::Call,
    ));
    ingest.references.push(Reference::new(
        2,
        interner.intern("service_fn"),
        2,
        18,
        Lang::Rust,
        RefRole::Call,
    ));

    let graph = Graph::build(&ingest, &interner);

    let report = find_affected_tests(&["src/core.rs"], &ingest, &graph, &interner);
    assert_eq!(report.seed_count, 1);
    assert_eq!(report.total_tests, 1);
    assert_eq!(report.tests[0].path, "tests/test_service.rs");
    assert_eq!(report.tests[0].hops, 2);

    let xml = report.to_xml_string();
    assert!(xml.contains("<affected schema=\"ripwire-rs.affected/v1\""));
    assert!(xml.contains("<test p=\"tests/test_service.rs\" hops=\"2\" run_unknown=\"1\"/>"));
}

#[test]
fn test_partner_subject_stem_naming_cases() {
    use ripwire_rs::gate::affected::partner_subject_stem;
    let cases: [(&str, Option<&str>); 24] = [
        ("scripts/test_lint_rules.py", Some("lint_rules")),
        ("src/lint_test.py", Some("lint")),
        ("pkg/ring.test.ts", Some("ring")),
        ("pkg/markdown.test.tsx", Some("markdown")),
        ("pkg/dirs.spec.js", Some("dirs")),
        ("src/server_test.go", Some("server")),
        ("src/widget_test.rs", Some("widget")),
        ("spec/widget_spec.rb", Some("widget")),
        ("tests/test_widget.ts", Some("widget")),
        ("tests/test_config.v2.py", Some("config.v2")),
        ("test/WidgetTest.py", Some("Widget")),
        ("src/widget_test.cpp", Some("widget")),
        ("tests/WidgetTest.java", Some("Widget")),
        ("src/test/java/WidgetTest.java", Some("Widget")),
        ("tests/WidgetTest.kt", Some("Widget")),
        ("tests/WidgetTest.php", Some("Widget")),
        ("src/WidgetTest.java", None),
        ("tests/WidgetTest.py", Some("Widget")),
        ("test/WidgetTest.scala", Some("Widget")),
        ("tests/widget.rs", None),
        ("src/widget_unittest.cc", None),
        ("tests/Widget_Test.php", None),
        ("scripts/test_.py", None),
        ("Makefile", None),
    ];
    for (path, want) in cases {
        let lang = Lang::from_path(std::path::Path::new(path));
        assert_eq!(partner_subject_stem(path, lang), want, "{path}");
    }
}

fn partner_fixture(paths: &[&str]) -> (IngestResult, Interner) {
    partner_fixture_where(paths, |_| true)
}

fn partner_fixture_where(
    paths: &[&str],
    has_symbols: impl Fn(&str) -> bool,
) -> (IngestResult, Interner) {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    for (id, p) in paths.iter().enumerate() {
        let file_id = u32::try_from(id).unwrap_or(u32::MAX);
        let lang = Lang::from_path(std::path::Path::new(p));
        ingest.files.push(FileInfo {
            id: file_id,
            path: interner.intern(p),
            lang,
            line_count: 10,
            byte_size: 100,
        });
        if has_symbols(p) {
            let name = interner.intern("f");
            ingest.symbols.push(Symbol::new(
                file_id,
                name,
                name,
                SymKind::Function,
                lang,
                file_id,
                1,
            ));
        }
    }
    (ingest, interner)
}

fn affected_in(ingest: &IngestResult, interner: &Interner, changed: &str) -> Vec<String> {
    let graph = Graph::build(ingest, interner);
    let report = find_affected_tests(&[changed], ingest, &graph, interner);
    report.tests.iter().map(|t| t.path.clone()).collect()
}

fn affected_paths(paths: &[&str], changed: &str) -> Vec<String> {
    let (ingest, interner) = partner_fixture(paths);
    affected_in(&ingest, &interner, changed)
}

#[test]
fn test_affected_partner_row_for_unique_convention_match() {
    let (ingest, interner) = partner_fixture(&[
        "scripts/lint_rules.py",
        "scripts/test_lint_rules.py",
        "scripts/test_lint_rules_extra.py",
        "tests/lint_rules.py",
        "jobs/a/lint_rules.py",
    ]);
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(&["scripts/lint_rules.py"], &ingest, &graph, &interner);
    let paths: Vec<&str> = report.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["scripts/test_lint_rules.py"]);
    assert!(report.tests[0].is_partner && report.tests[0].hops == 0);
    assert!(
        report
            .to_xml_string()
            .contains("<test p=\"scripts/test_lint_rules.py\" partner=\"1\" run_unknown=\"1\"/>")
    );
}

#[test]
fn test_affected_partner_cross_language_and_cross_package() {
    assert_eq!(
        affected_paths(
            &["pkg1/src/env.ts", "pkg2/test/env.test.ts"],
            "pkg1/src/env.ts"
        ),
        ["pkg2/test/env.test.ts"]
    );
    assert_eq!(
        affected_paths(&["src/widget.py", "tests/test_widget.ts"], "src/widget.py"),
        ["tests/test_widget.ts"]
    );
}

#[test]
fn test_affected_partner_lists_every_candidate() {
    assert_eq!(
        affected_paths(
            &["a/widget.py", "a/test_widget.py", "a/widget_test.py"],
            "a/widget.py"
        ),
        ["a/test_widget.py", "a/widget_test.py"]
    );
    assert_eq!(
        affected_paths(
            &["a/widget.py", "a/test_widget.py", "b/test_widget.py"],
            "a/widget.py"
        ),
        ["a/test_widget.py", "b/test_widget.py"]
    );
    assert_eq!(
        affected_paths(
            &["a/widget.ts", "a/widget.test.ts", "a/widget.spec.ts"],
            "a/widget.ts"
        ),
        ["a/widget.spec.ts", "a/widget.test.ts"]
    );
}

#[test]
fn test_affected_partner_declined_when_unnamed() {
    assert!(
        affected_paths(
            &["src/gate/affected.rs", "tests/gate/affected.rs"],
            "src/gate/affected.rs"
        )
        .is_empty()
    );
    assert!(
        affected_paths(
            &["src/Widget.java", "src/WidgetTest.java"],
            "src/Widget.java"
        )
        .is_empty()
    );
}

#[test]
fn test_affected_partner_changed_test_keeps_row_and_flags_partner() {
    let paths = [
        "src/commands/gate.rs",
        "src/commands/test_gate.rs",
        "src/gate/test_gate.rs",
    ];
    let (ingest, interner) = partner_fixture(&paths);
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(
        &["src/commands/gate.rs", "src/commands/test_gate.rs"],
        &ingest,
        &graph,
        &interner,
    );
    let rows: Vec<(&str, bool, bool)> = report
        .tests
        .iter()
        .map(|t| (t.path.as_str(), t.is_direct_change, t.is_partner))
        .collect();
    assert_eq!(
        rows,
        [
            ("src/commands/test_gate.rs", true, true),
            ("src/gate/test_gate.rs", false, true),
        ]
    );
    let xml = report.to_xml_string();
    assert!(xml.contains(
        "<test p=\"src/commands/test_gate.rs\" partner=\"1\" seed_kind=\"test\" run_unknown=\"1\"/>"
    ));
    assert!(xml.contains("<test p=\"src/gate/test_gate.rs\" partner=\"1\" run_unknown=\"1\"/>"));
}

#[test]
fn test_affected_partner_dotted_stem_matches_full_subject() {
    assert_eq!(
        affected_paths(
            &[
                "src/config.v2.py",
                "tests/test_config.py",
                "tests/test_config.v2.py",
            ],
            "src/config.v2.py"
        ),
        ["tests/test_config.v2.py"]
    );
}

#[test]
fn test_affected_partner_needs_symbols_on_changed_test_only() {
    let paths = ["src/widget.ts", "tests/widget.test.ts"];
    // A symbol-less changed source still names its partner test.
    let (ingest, interner) = partner_fixture_where(&paths, |p| p != "src/widget.ts");
    assert_eq!(
        affected_in(&ingest, &interner, "src/widget.ts"),
        ["tests/widget.test.ts"]
    );
    // A symbol-less changed test keeps no row of its own.
    let (ingest, interner) = partner_fixture_where(&paths, |p| p != "tests/widget.test.ts");
    assert_eq!(
        affected_in(&ingest, &interner, "src/widget.ts"),
        ["tests/widget.test.ts"]
    );
    assert!(affected_in(&ingest, &interner, "tests/widget.test.ts").is_empty());
}

#[test]
fn test_affected_partner_indexes_every_language_test_convention() {
    assert_eq!(
        affected_paths(
            &["src/widget.json", "src/test_widget.json"],
            "src/widget.json"
        ),
        ["src/test_widget.json"]
    );
    assert_eq!(
        affected_paths(
            &["src/widget.toml", "src/test_widget.toml"],
            "src/widget.toml"
        ),
        ["src/test_widget.toml"]
    );
    // Languages upstream does not index still pair tests by convention (a recorded divergence).
    assert_eq!(
        affected_paths(&["db/widget.sql", "db/test_widget.sql"], "db/widget.sql"),
        ["db/test_widget.sql"]
    );
    assert_eq!(
        affected_paths(
            &["src/widget.scala", "src/test_widget.scala"],
            "src/widget.scala"
        ),
        ["src/test_widget.scala"]
    );
    assert!(
        affected_paths(&["src/widget.xyz", "src/test_widget.xyz"], "src/widget.xyz").is_empty()
    );
}

#[test]
fn test_affected_partner_camel_convention_needs_test_dir() {
    assert_eq!(
        affected_paths(&["src/Widget.py", "tests/WidgetTest.py"], "src/Widget.py"),
        ["tests/WidgetTest.py"]
    );
    assert_eq!(
        affected_paths(&["src/Widget.ts", "test/WidgetTest.ts"], "src/Widget.ts"),
        ["test/WidgetTest.ts"]
    );
    assert!(affected_paths(&["src/Widget.py", "src/WidgetTest.py"], "src/Widget.py").is_empty());
    assert!(!is_test_path("src/WidgetTest.java"));
}

#[test]
fn test_runner_command_by_language_and_nearest_manifest() {
    use ripwire_rs::gate::runners::nearest_manifest;
    use std::collections::HashMap;
    let cases: [(&str, &str, Option<&str>, Option<&str>); 10] = [
        (
            "jobs/test_main.py",
            "if __name__ == \"__main__\":\n    pass\n",
            None,
            Some("python3 jobs/test_main.py"),
        ),
        (
            "jobs/test_main.py",
            "if __name__ == '__main__':\n    pass\n",
            None,
            Some("python3 jobs/test_main.py"),
        ),
        (
            "scripts/test_lint.py",
            "#!/usr/bin/env python3\nassert 1\n",
            None,
            None,
        ),
        (
            "test/run_test.sh",
            "echo ok\n",
            None,
            Some("bash test/run_test.sh"),
        ),
        ("test/run_test.bash", "echo ok\n", None, None),
        (
            "pkg/a.test.ts",
            "test('x',()=>{})",
            Some("{\"devDependencies\":{\"vitest\":\"1\"}}"),
            Some("npx vitest run pkg/a.test.ts"),
        ),
        (
            "pkg/a.test.ts",
            "test('x',()=>{})",
            Some("{\"devDependencies\":{\"jest\":\"1\"}}"),
            Some("npx jest pkg/a.test.ts"),
        ),
        (
            "pkg/a.test.js",
            "const t = require('node:test');",
            None,
            Some("node --test pkg/a.test.js"),
        ),
        (
            "pkg/a.test.ts",
            "const t = require('node:test');",
            None,
            None,
        ),
        ("tests/foo_test.go", "package x", None, None),
    ];
    for (path, src, manifest, want) in cases {
        let lang = Lang::from_path(std::path::Path::new(path));
        assert_eq!(
            lang.semantics()
                .test_run_command(path, src, manifest)
                .as_deref(),
            want,
            "{path}"
        );
    }
    let indexed: HashMap<&str, Lang> = [
        ("package.json", Lang::Json),
        ("packages/a/package.json", Lang::Json),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        nearest_manifest("packages/a/test/w.test.ts", "package.json", &indexed),
        Some("packages/a/package.json")
    );
    assert_eq!(
        nearest_manifest("packages/b/test/w.test.ts", "package.json", &indexed),
        Some("package.json")
    );
}

#[test]
fn test_assertion_script_run_comes_from_its_documented_header() {
    let cases: &[(&str, &str, Option<&str>)] = &[
        (
            "src/lib/cart-pricing.check.ts",
            "// Self-check.\n//\n//   pnpm dlx tsx src/lib/cart-pricing.check.ts\n\nimport x from 'y';\n",
            Some("pnpm dlx tsx src/lib/cart-pricing.check.ts"),
        ),
        (
            "src/lib/queries/payment-methods-promote.check.ts",
            "// pnpm dlx tsx src/lib/queries/payment-methods-promote.check.ts - because\n",
            Some("pnpm dlx tsx src/lib/queries/payment-methods-promote.check.ts"),
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "// Run: npx tsx cart-pricing.check.ts\n",
            Some("npx tsx src/lib/cart-pricing.check.ts"),
        ),
        (
            "pkg/a.check.ts",
            "// tsx a.check.ts\n",
            Some("tsx pkg/a.check.ts"),
        ),
        (
            "pkg/a.check.ts",
            "// bun pkg/a.check.ts\n",
            Some("bun pkg/a.check.ts"),
        ),
        (
            "pkg/a.check.ts",
            "// deno run pkg/a.check.ts\n",
            Some("deno run pkg/a.check.ts"),
        ),
        (
            "pkg/a.check.js",
            "/* node pkg/a.check.js */\n",
            Some("node pkg/a.check.js"),
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "import x from 'y';\n",
            None,
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "import x;\n// pnpm dlx tsx src/lib/cart-pricing.check.ts\n",
            None,
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "// pnpm dlx tsx src/lib/other.check.ts\n",
            None,
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "// tsx src/other/cart-pricing.check.ts\n",
            None,
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "// deno test src/lib/cart-pricing.check.ts\n",
            None,
        ),
        (
            "src/lib/cart-pricing.check.ts",
            "// src/lib/cart-pricing.check.ts\n",
            None,
        ),
    ];
    for &(path, src, want) in cases {
        let lang = Lang::from_path(std::path::Path::new(path));
        assert_eq!(
            lang.semantics()
                .test_run_command(path, src, None)
                .as_deref(),
            want,
            "{path}"
        );
    }
}

#[test]
fn test_summarize_test_gate_separates_modscope_and_sorts_by_ccx() {
    use ripwire_rs::gate::summarize_test_gate;
    use ripwire_rs::navigate::compute_tested_node_mask;
    let interner = Interner::new();
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[
            ("src/core.py", Lang::Python, b"def leaf():\n    return 1\n"),
            (
                "src/app.py",
                Lang::Python,
                b"def simple():\n    return leaf()\ndef branchy(a, b):\n    if a:\n        if b:\n            return leaf()\n    return 0\nleaf()\n",
            ),
            ("tests/test_core.py", Lang::Python, b"def test_it():\n    simple()\n"),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(&["src/core.py"], &ingest, &graph, &interner);
    let tested = compute_tested_node_mask(&ingest, &graph, &interner);
    let sum = summarize_test_gate(&report, &tested, &ingest, &interner);
    assert_eq!(sum.untested_modscope, 1);
    let names: Vec<&str> = sum
        .untested
        .iter()
        .map(|&id| interner.resolve(&ingest[id].name))
        .collect();
    assert_eq!(names, ["branchy"]);
    assert_eq!(sum.run_first, 1);
}

#[test]
fn test_test_gate_rows_fold_like_upstream() {
    use ripwire_rs::gate::affected::AffectedTest;
    use ripwire_rs::serialize::arch_xml::serialize_test_gate_xml;
    let render = |tests: &[AffectedTest]| {
        let mut buf = Vec::new();
        serialize_test_gate_xml(".", tests, 0, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    };
    let siblings = [
        AffectedTest::new("src/a/test_widget.py".to_string(), 0, false, true),
        AffectedTest::new("src/a/widget_test.py".to_string(), 0, false, true),
    ];
    let xml = render(&siblings);
    assert!(
        xml.contains(
            "<g partner=\"1\" n=\"2\" p=\"src/a/test_widget.py,src/a/widget_test.py\" run_unknown=\"1\"/>"
        ),
        "{xml}"
    );
    let pair = [
        AffectedTest::new("src/commands/test_gate.rs".to_string(), 0, true, true),
        AffectedTest::new("src/gate/test_gate.rs".to_string(), 0, false, true),
    ];
    let xml = render(&pair);
    assert!(
        xml.contains(
            "<t p=\"src/commands/test_gate.rs\" changed=\"1\" partner=\"1\" run_unknown=\"1\"/>"
        ),
        "{xml}"
    );
    assert!(
        xml.contains("<t p=\"src/gate/test_gate.rs\" partner=\"1\" run_unknown=\"1\"/>"),
        "{xml}"
    );
}

#[test]
fn test_pack_task_seeds_follow_ranked_symbols_not_their_whole_file() {
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "src/query/search.rs",
                Lang::Rust,
                b"pub fn ranked_search() -> u32 {\n    1\n}\n\npub fn unrelated_helper() -> u32 {\n    2\n}\n".as_slice(),
            ),
            (
                "tests/alpha/uses_ranked.rs",
                Lang::Rust,
                b"fn test_ranked() {\n    ranked_search();\n}\n".as_slice(),
            ),
            (
                "tests/beta/uses_helper.rs",
                Lang::Rust,
                b"fn test_helper() {\n    unrelated_helper();\n}\n".as_slice(),
            ),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let ranked = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "ranked_search")
        .unwrap_or_else(|| panic!("ranked_search symbol"))
        .id;

    let scoped = find_affected_tests_for_symbols(
        &[ranked],
        &["src/query/search.rs"],
        &ingest,
        &graph,
        &interner,
    );
    let scoped_paths: Vec<&str> = scoped.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(scoped_paths, ["tests/alpha/uses_ranked.rs"]);

    let whole_file = find_affected_tests(&["src/query/search.rs"], &ingest, &graph, &interner);
    let whole_paths: Vec<&str> = whole_file.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(
        whole_paths,
        ["tests/alpha/uses_ranked.rs", "tests/beta/uses_helper.rs"]
    );
}

#[test]
fn test_affected_runnerless_partner_siblings_fold_with_partner_count() {
    let (ingest, interner) =
        partner_fixture(&["a/widget.py", "a/test_widget.py", "a/widget_test.py"]);
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(&["a/widget.py"], &ingest, &graph, &interner);
    let xml = report.to_xml_string();
    assert!(xml.contains(" partners=\"2\""), "{xml}");
    assert!(
        xml.contains(
            "<g partner=\"1\" n=\"2\" p=\"a/test_widget.py,a/widget_test.py\" run_unknown=\"1\"/>"
        ),
        "{xml}"
    );
}

#[test]
fn test_affected_assertion_scripts_name_their_subject() {
    assert!(is_test_path(
        "src/lib/onboarding-flow/cart-pricing.check.ts"
    ));
    assert!(is_test_path("src/lib/review-pricing.check.tsx"));
    assert!(is_test_path("scripts/tax.check.js"));
    assert!(!is_test_path("src/lib/cart-pricing.ts"));
    assert!(!is_test_path("src/lib/checks.ts"));
    assert_eq!(
        affected_paths(
            &[
                "src/lib/cart-pricing.ts",
                "src/lib/cart-pricing.check.ts",
                "src/lib/cart-pricing.check.tsx",
            ],
            "src/lib/cart-pricing.ts"
        ),
        [
            "src/lib/cart-pricing.check.ts",
            "src/lib/cart-pricing.check.tsx"
        ]
    );
    assert!(
        affected_paths(
            &["src/lib/cart-pricing.ts", "src/lib/tax.check.ts"],
            "src/lib/cart-pricing.ts"
        )
        .is_empty()
    );
}

#[test]
fn test_affected_assertion_script_reaches_subject_without_marking_importers() {
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "src/lib/onboarding-flow/cart-pricing.ts",
                Lang::TypeScript,
                b"export function priceCart(units: number): number {\n  return units * 2;\n}\n"
                    .as_slice(),
            ),
            (
                "src/lib/onboarding-flow/cart-pricing.check.ts",
                Lang::TypeScript,
                b"import { priceCart } from \"./cart-pricing\";\n\nfunction checkTotal(): void {\n  priceCart(1);\n}\n"
                    .as_slice(),
            ),
            (
                "src/lib/checkout/order.ts",
                Lang::TypeScript,
                b"import { priceCart } from \"../onboarding-flow/cart-pricing\";\n\nexport function placeOrder(): number {\n  return priceCart(2);\n}\n"
                    .as_slice(),
            ),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(
        &["src/lib/onboarding-flow/cart-pricing.ts"],
        &ingest,
        &graph,
        &interner,
    );
    let paths: Vec<&str> = report.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["src/lib/onboarding-flow/cart-pricing.check.ts"]);
    assert!(report.tests[0].is_partner);
}

#[test]
fn test_affected_root_names_the_reading_that_seeded_the_walk() {
    let (ingest, interner) =
        partner_fixture(&["scripts/lint_rules.py", "scripts/test_lint_rules.py"]);
    let graph = Graph::build(&ingest, &interner);
    let by_file = find_affected_tests(&["scripts/lint_rules.py"], &ingest, &graph, &interner);
    assert_eq!(by_file.seeded_by, "file");
    assert!(
        by_file
            .to_xml_string()
            .contains(" changed=\"scripts/lint_rules.py\" seeded_by=\"file\"")
    );
    let unmatched = find_affected_tests(&["no_such_symbol"], &ingest, &graph, &interner);
    assert_eq!(unmatched.seeded_by, "none");
    assert!(unmatched.to_xml_string().contains(" seeded_by=\"none\""));
}

#[test]
fn test_affected_reaches_a_test_through_a_static_initializer_read() {
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "src/cmds/mod.rs",
                Lang::Rust,
                b"pub fn options() -> u32 {\n    1\n}\n\npub fn tools() -> u32 {\n    static OWNED: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| options());\n    *OWNED\n}\n",
            ),
            (
                "tests/cmds.rs",
                Lang::Rust,
                b"#[test]\nfn lists_tools() {\n    tools();\n}\n",
            ),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let report = find_affected_tests(&["options"], &ingest, &graph, &interner);
    let paths: Vec<&str> = report.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["tests/cmds.rs"]);
}
