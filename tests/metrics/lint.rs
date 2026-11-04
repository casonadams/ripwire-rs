use ripwire_rs::metrics::lint::{LintOptions, LintScan, run_lint_scan, serialize_lint_xml};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};

const COMMENTS_C: &str =
    "void track() { const char *text = \"strcpy(dst, src)\"; /* MD5(buf); */ strcpy(dst, src); }";
const SURFACE_TS: &str = "type AB = number; interface Surface { on(): number; setTrack(): number; isReady(): number; } function ab() {}";
const KNOWN_RETURNS_TS: &str = "function isReady(): number { return 1; } function isEnabled(): boolean { return true; } function setTrack(): number { return 1; } function setReady(): void {}";
const KNOWN_RETURNS_RS: &str = "fn is_ready(callback: fn() -> bool) -> usize { 0 } fn is_enabled() -> bool { true } fn set_track() -> u8 { 0 } fn set_ready() {}";
const KNOWN_RETURNS_PY: &str = "def is_ready(value: list[int]) -> int: return 0\ndef is_enabled() -> bool: return True\ndef set_track() -> int: return 0\ndef set_ready() -> None: pass\n";
const KNOWN_RETURNS_SWIFT: &str = "func isReady(callback: () -> Bool) -> Int { return 0 }\nfunc isEnabled() -> Bool { return true }\nfunc setTrack() -> Int { return 0 }\nfunc setReady() -> Void {}\n";
const SELF_ASSIGN_TS: &str = "function track(value: number) { value += value; value -= value; value *= value; value /* note */ = value; }";
const SELF_ASSIGN_JS: &str = "function track(value) { value += value; value -= value; value *= value; value /* note */ = value; }";
const SELF_ASSIGN_CPP: &str = "void track(int value) { value += value; value -= value; value *= value; value /* note */ = value; }";
const UNDERSCORE_C: &str = "void _Track() {} void track__value() {} void track_otherValue() {}";
const UNDERSCORE_PY: &str = "def __enter__(): pass\n";

type Files<'a> = &'a [(&'a str, Lang, &'a str)];

fn scan(files: Files, options: LintOptions) -> LintScan {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let sources: Vec<_> = files
        .iter()
        .map(|&(path, lang, source)| (path, lang, source.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&sources, &registry, &interner);
    run_lint_scan(
        &ingest,
        &interner,
        &|path| files.iter().find(|f| f.0 == path).map(|f| f.2.to_owned()),
        options,
    )
    .unwrap()
}

fn selecting(rule: &str) -> LintOptions {
    LintOptions {
        select: vec![rule.into()],
        ..LintOptions::default()
    }
}

fn count(scan: &LintScan, rule: &str) -> usize {
    scan.findings.iter().filter(|f| f.rule == rule).count()
}

fn xml(scan: &LintScan, page: PageRequest) -> String {
    let mut bytes = Vec::new();
    serialize_lint_xml(".", scan, page, &mut bytes).unwrap();
    String::from_utf8(bytes).unwrap()
}

fn capped(scan: &LintScan) -> bool {
    xml(scan, PageRequest::default()).contains("count_capped=\"1\"")
}

fn confusable_scan(path: &str, lang: Lang, source: &str) -> LintScan {
    scan(&[(path, lang, source)], selecting("naming-confusable"))
}

#[test]
fn language_applicability_excludes_c_only_expression_rules() {
    let result = scan(
        &[(
            "pkg/track.ts",
            Lang::TypeScript,
            "function track(value: number) { let odd = 0755; let other = value++ + 7; return value ? other : odd ? 4 : 5; }",
        )],
        LintOptions::default(),
    );
    assert!(
        !result
            .findings
            .iter()
            .any(|f| f.rule.starts_with("atom-") || f.rule == "magic-number")
    );
}

#[test]
fn single_fixture_rule_counts_and_selection() {
    let cases: [(Files, &str, &str, usize); 20] = [
        (
            &[("pkg/track.c", Lang::C, COMMENTS_C)],
            "",
            "unsafe-c-fn",
            1,
        ),
        (
            &[("pkg/track.c", Lang::C, COMMENTS_C)],
            "",
            "weak-crypto",
            0,
        ),
        (
            &[(
                "pkg/track.ts",
                Lang::TypeScript,
                "function track(value: number) { if (value) { return value; } value++; return value; value++; }",
            )],
            "",
            "unreachable-code",
            1,
        ),
        (
            &[(
                "pkg/track.tsx",
                Lang::TypeScript,
                "function track() { return <span>return value; next()</span>; } function after() {}",
            )],
            "",
            "unreachable-code",
            0,
        ),
        (
            &[("pkg/track.ts", Lang::TypeScript, SURFACE_TS)],
            "",
            "naming-short",
            1,
        ),
        (
            &[("pkg/track.ts", Lang::TypeScript, SURFACE_TS)],
            "",
            "naming-setter",
            0,
        ),
        (
            &[("pkg/track.ts", Lang::TypeScript, SURFACE_TS)],
            "",
            "naming-predicate",
            0,
        ),
        (
            &[(
                "tests/track.ts",
                Lang::TypeScript,
                "function testGetHTTPResponseCodeForUserNow() {} function track1a2b3() {}",
            )],
            "",
            "naming-wordy",
            2,
        ),
        (
            &[(
                "pkg/track.ts",
                Lang::TypeScript,
                "function track1() {} function track2() {} function read8() {} function read16() {}",
            )],
            "",
            "naming-series",
            2,
        ),
        (
            &[
                ("pkg/track.ts", Lang::TypeScript, KNOWN_RETURNS_TS),
                ("pkg/track.rs", Lang::Rust, KNOWN_RETURNS_RS),
                ("pkg/track.py", Lang::Python, KNOWN_RETURNS_PY),
                ("pkg/track.swift", Lang::Swift, KNOWN_RETURNS_SWIFT),
            ],
            "",
            "naming-predicate",
            4,
        ),
        (
            &[
                ("pkg/track.ts", Lang::TypeScript, KNOWN_RETURNS_TS),
                ("pkg/track.rs", Lang::Rust, KNOWN_RETURNS_RS),
                ("pkg/track.py", Lang::Python, KNOWN_RETURNS_PY),
                ("pkg/track.swift", Lang::Swift, KNOWN_RETURNS_SWIFT),
            ],
            "",
            "naming-setter",
            4,
        ),
        (
            &[
                (
                    "pkg/track.ts",
                    Lang::TypeScript,
                    "function object() {} function reject() {}",
                ),
                ("pkg/other.ts", Lang::TypeScript, "function objects() {}"),
            ],
            "",
            "naming-confusable",
            1,
        ),
        (
            &[(
                "pkg/track.ts",
                Lang::TypeScript,
                "const UPPER_VALUE = 0; const UPPER_VALUES = 0;",
            )],
            "naming-confusable",
            "naming-confusable",
            1,
        ),
        (
            &[(
                "pkg/track.dart",
                Lang::Dart,
                "void track() { value = value.abs(); value = value + 1; }",
            )],
            "",
            "self-assign",
            1,
        ),
        (
            &[("code.ts", Lang::TypeScript, SELF_ASSIGN_TS)],
            "",
            "self-assign",
            1,
        ),
        (
            &[("code.js", Lang::JavaScript, SELF_ASSIGN_JS)],
            "",
            "self-assign",
            1,
        ),
        (
            &[("code.cpp", Lang::Cpp, SELF_ASSIGN_CPP)],
            "",
            "self-assign",
            1,
        ),
        (
            &[
                (
                    "pkg/track.ts",
                    Lang::TypeScript,
                    "class Alpha { handleEvent() {} } class Beta { handleEvents() {} } class Gamma { handleEvents() {} }",
                ),
                (
                    "pkg/track.py",
                    Lang::Python,
                    "class Alpha:\n    def handleEvent(self): pass\nclass Beta:\n    def handleEvents(self): pass\n",
                ),
                (
                    "pkg/track.rs",
                    Lang::Rust,
                    "fn handle_event() {} mod inner { fn handle_events() {} }",
                ),
            ],
            "",
            "naming-confusable",
            1,
        ),
        (
            &[
                ("pkg/track.c", Lang::C, UNDERSCORE_C),
                ("pkg/track.py", Lang::Python, UNDERSCORE_PY),
            ],
            "",
            "naming-underscore",
            2,
        ),
        (
            &[
                ("pkg/track.c", Lang::C, UNDERSCORE_C),
                ("pkg/track.py", Lang::Python, UNDERSCORE_PY),
            ],
            "",
            "naming-case",
            1,
        ),
    ];
    for (files, select, rule, expected) in cases {
        let options = if select.is_empty() {
            LintOptions::default()
        } else {
            selecting(select)
        };
        let result = scan(files, options);
        assert_eq!(count(&result, rule), expected, "{rule} for {files:?}");
    }
}

#[test]
fn cpp_expression_and_container_rules_use_syntax_context() {
    let source = r#"
typedef int Count;
void track(std::shared_ptr<int> shared) {
    const int allowed = 7;
    int value = 7;
    int octal = 0755;
    value = value;
    int reversed = 1[items];
    int wrapped = ((value));
    int nested = value ? 1 : other ? 2 : 3;
    int embedded = value++ + 1;
    value = 1, other = 2;
    if (value = 1) {}
    if (value + other) {}
    if ((value)) ;
    std::map<int,int> indexed;
    std::vector<int*> pointers;
    std::vector<std::unique_ptr<int>> indirect;
    for (int index = 0; index < 10; index++) {
        auto pointer = new int(1);
        pointer = pointer->next;
        auto gathered = items[indices[index]];
    }
    do { _mm_prefetch(items, 0); } while (value--);
    try {} catch (...) {}
    goto done;
    done: return;
}
"#;
    let result = scan(
        &[("pkg/track.cpp", Lang::Cpp, source)],
        LintOptions::default(),
    );
    for rule in [
        "typedef-over-using",
        "magic-number",
        "atom-octal-literal",
        "self-assign",
        "atom-reversed-subscript",
        "atom-nested-ternary",
        "atom-embedded-crement",
        "atom-comma-operator",
        "atom-assign-as-value",
        "atom-implicit-predicate",
        "redundant-parens",
        "suspicious-semicolon",
        "cache-node-container",
        "cache-vector-of-raw-ptr",
        "cache-vector-of-indirect",
        "cache-heap-alloc-in-loop",
        "cache-pointer-chase-loop",
        "cache-gather-subscript",
        "cache-shared-ptr-by-value",
        "cache-manual-prefetch",
        "do-while",
        "empty-catch",
        "goto",
    ] {
        assert!(count(&result, rule) > 0, "{rule}");
    }
}

#[test]
fn c_function_size_and_brace_depth_are_not_generic_symbol_metrics() {
    let body = "    value++;\n".repeat(85);
    let source = format!(
        "void track() {{\n{body}if (value) {{ while (value) {{ if (other) {{ while (other) {{ if (ready) {{ return; }} }} }} }} }}\nreturn 7;\n}}\n"
    );
    let result = scan(&[("pkg/track.c", Lang::C, &source)], LintOptions::default());
    assert_eq!(count(&result, "large-function"), 1);
    assert_eq!(count(&result, "deep-nesting"), 1);
    assert_eq!(count(&result, "inconsistent-return"), 1);
}

#[test]
fn rule_selection_is_prefix_based_and_invalid_prefixes_are_errors() {
    let options = LintOptions {
        select: vec!["naming-short".into()],
        ..LintOptions::default()
    };
    let result = scan(
        &[(
            "pkg/track.ts",
            Lang::TypeScript,
            "function ab() {} function cd() {}",
        )],
        options,
    );
    assert_eq!(result.stats.len(), 1);
    assert_eq!(result.findings.len(), 2);
    let invalid = LintOptions {
        select: vec!["short".into()],
        ..LintOptions::default()
    };
    assert!(ripwire_rs::metrics::lint::selected_rules(&invalid).is_err());
}

const BUDGET_TS: &str = "declare function signature{i}(): void;\nclass Surface{i} {}\nconst marker{i} = 0;\nfunction marker() {}\n";
const BUDGET_PY: &str = "class Surface{i}:\n    def marker{i}(self): pass\n";

#[test]
fn window_and_rerun_hint_disclose_short_name_findings() {
    let result = scan(
        &[(
            "pkg/track.ts",
            Lang::TypeScript,
            "function ab() {} function cd() {}",
        )],
        selecting("naming-short"),
    );
    let report = xml(
        &result,
        PageRequest {
            limit: Some(1),
            offset: Some(1),
            ..PageRequest::default()
        },
    );
    assert!(report.contains("shown=\"1\" capped=\"1\" total=\"2\" has_more=\"0\" next_offset=\"2\" offset=\"1\" limit=\"1\""));
    assert!(!report.contains("findings_next="));
    assert!(report.contains("shown_rows=\"1\" rows_capped=\"1\""));

    let result = scan(
        &[(
            "pkg/track.ts",
            Lang::TypeScript,
            "function ab() {} function cd() {}",
        )],
        LintOptions {
            select: vec!["naming-short".into()],
            budget: 1,
            ..LintOptions::default()
        },
    );
    let report = xml(&result, PageRequest::default());
    assert!(report.contains("findings_capped=\"1\""));
    assert!(
        report.contains(
            "findings_next=\"--lint --lint-select=naming-short --lint-max-per-rule=50000\""
        )
    );
}

#[test]
fn confusable_lexical_budget_is_bounded_per_scan() {
    for (fillers, expected) in [(510, 1), (511, 0)] {
        let mut source = String::from("function satherRecord() {}\n");
        for i in 0..fillers {
            source.push_str(&format!("function marker{i}() {{}}\n"));
        }
        source.push_str("function gatherRecord() {}\n");
        let result = confusable_scan("pkg/track.ts", Lang::TypeScript, &source);
        assert_eq!(count(&result, "naming-confusable"), expected);
        assert_eq!(capped(&result), fillers == 511);
        if expected == 1 {
            assert_eq!(
                result.findings[0].message,
                "satherRecord ~ gatherRecord (edit distance <=2)"
            );
        }
    }

    for (between, expected) in [(23, 1), (24, 0)] {
        let mut source = String::from("function satherRecord() {}\n");
        for i in 0..between {
            source.push_str(&format!("function marker{i}() {{}}\n"));
        }
        for i in 0..520 {
            source.push_str(&format!("function zuluMarker{i}() {{}}\n"));
        }
        source.push_str("function gatherRecord() {}\n");
        let result = confusable_scan("pkg/track.ts", Lang::TypeScript, &source);
        assert_eq!(count(&result, "naming-confusable"), expected);
        assert!(capped(&result));
    }

    let mut source = String::new();
    for i in 0..520 {
        source.push_str(&format!("function marker{i}() {{}}\n"));
    }
    source.push_str("function gatherRecord() {}\nfunction gatherRecords() {}\n");
    let result = confusable_scan("pkg/track.ts", Lang::TypeScript, &source);
    assert_eq!(count(&result, "naming-confusable"), 1);
    assert!(capped(&result));

    let cases = [
        (
            "pkg/track.ts",
            Lang::TypeScript,
            BUDGET_TS,
            "function gatherRecord() {}\nfunction satherRecord() {}\n",
        ),
        (
            "pkg/track.py",
            Lang::Python,
            BUDGET_PY,
            "def gather_record(): pass\ndef sather_record(): pass\n",
        ),
    ];
    for (path, lang, filler, tail) in cases {
        let mut source: String = (0..520)
            .map(|i| filler.replace("{i}", &i.to_string()))
            .collect();
        source.push_str(tail);
        let result = confusable_scan(path, lang, &source);
        assert_eq!(count(&result, "naming-confusable"), 1, "{path}");
        assert!(!capped(&result), "{path}");
    }

    for (fillers, expected) in [(510, 1), (511, 0)] {
        let mut source = String::new();
        for i in 0..fillers {
            source.push_str(&format!("const MARKER_{i} = 0;\n"));
        }
        source.push_str("const GATHER_RECORD = 0; const SATHER_RECORD = 0;\n");
        let result = confusable_scan("pkg/track.ts", Lang::TypeScript, &source);
        assert_eq!(count(&result, "naming-confusable"), expected);
        assert_eq!(capped(&result), fillers == 511);
    }
}

#[test]
fn sarif_preserves_each_finding_severity() {
    let result = scan(
        &[(
            "code.cpp",
            Lang::Cpp,
            "void ab(char *dst, const char *src) { strcpy(dst, src); int value = 0; value = value; }",
        )],
        LintOptions::default(),
    );
    let mut bytes = Vec::new();
    ripwire_rs::metrics::lint::serialize_lint_sarif(&result, &mut bytes).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let run = &report["runs"][0];
    for (rule, expected) in [
        ("naming-short", "note"),
        ("self-assign", "warning"),
        ("unsafe-c-fn", "error"),
    ] {
        let finding = run["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["ruleId"] == rule)
            .unwrap();
        let descriptor = run["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == rule)
            .unwrap();
        let severity = finding
            .get("level")
            .unwrap_or(&descriptor["defaultConfiguration"]["level"]);
        assert_eq!(severity, expected, "{rule}");
    }
}
