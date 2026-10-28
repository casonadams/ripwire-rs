use ripwire_rs::gate::affected::AffectedTest;
use ripwire_rs::gate::pr_context::{
    CallerFile, ChangedLines, FileHistory, MAX_DETAIL_ROWS, PrContextReport, PrContextRequest,
    PrFileContext, compute_pr_context_from_facts, serialize_pr_context_xml, touched_symbols,
};
use ripwire_rs::git::repo::new_side_changes;
use ripwire_rs::serialize::GraphGauges;
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, LanguageRegistry, SymKind, Symbol,
    ingest_in_memory_files,
};
use std::collections::BTreeMap;

fn file_context(path: &str) -> PrFileContext {
    PrFileContext {
        path: path.to_string(),
        symbol_count: 0,
        changed_symbols: Vec::new(),
        dependents_count: 0,
        callers: Vec::new(),
        tests: Vec::new(),
        history: None,
    }
}

fn report_of(files: Vec<PrFileContext>, base_ref: Option<&str>, at_stamp: &str) -> PrContextReport {
    PrContextReport {
        base_ref: base_ref.map(str::to_string),
        at_stamp: at_stamp.to_string(),
        files_count: files.len(),
        files,
        gauges: GraphGauges::default(),
        page: ripwire_rs::serialize::PageRequest::default(),
    }
}

fn xml_of(report: &PrContextReport) -> String {
    let mut out = Vec::new();
    serialize_pr_context_xml(report, &mut out).expect("serialize should succeed");
    String::from_utf8(out).expect("valid utf-8")
}

/// Each `tag` element's attributes in document order, as `name=value` pairs.
fn rows(xml: &str, tag: &str) -> Vec<Vec<String>> {
    let doc = roxmltree::Document::parse(xml).expect("well-formed xml");
    doc.descendants()
        .filter(|n| n.has_tag_name(tag))
        .map(|n| {
            n.attributes()
                .map(|a| format!("{}={}", a.name(), a.value()))
                .collect()
        })
        .collect()
}

#[test]
fn test_serialize_pr_context_xml() {
    let mut test = AffectedTest::new("tests/lib_test.rs".to_string(), 1, false, false);
    test.run = Some("cargo test --test lib_test".to_string());
    let file = PrFileContext {
        symbol_count: 5,
        changed_symbols: vec!["run_cli".to_string(), "init".to_string()],
        dependents_count: 3,
        callers: vec![
            CallerFile {
                path: "src/main.rs".to_string(),
                deps: 2,
            },
            CallerFile {
                path: "src/app.rs".to_string(),
                deps: 1,
            },
        ],
        tests: vec![test],
        history: Some(FileHistory {
            commits: 4,
            partners: 0,
            authors: 1,
            bus_factor: 1,
        }),
        ..file_context("src/lib.rs")
    };
    let xml = xml_of(&report_of(vec![file], Some("main"), "abc123def"));
    assert!(xml.contains("schema=\"ripwire-rs.pr-context/v1\""));
    assert!(xml.contains("base=\"main\" direction=\"worktree-since-merge-base\""));
    assert!(xml.contains(" at=\"abc123def\""));
    assert!(xml.contains("<file p=\"src/lib.rs\" symbols=\"5\">"));
    assert!(xml.contains("<s n=\"run_cli\"/>"));
    assert!(xml.contains("<s n=\"init\"/>"));
    assert_eq!(
        (rows(&xml, "impact"), rows(&xml, "f"), rows(&xml, "test")),
        (
            vec![vec!["dependents=3".into(), "files_other=2".into()]],
            vec![
                vec!["p=src/main.rs".into(), "deps=2".into()],
                vec!["p=src/app.rs".into()],
            ],
            vec![vec![
                "p=tests/lib_test.rs".into(),
                "run=cargo test --test lib_test".into()
            ]],
        )
    );
    assert!(xml.contains("<cochange commits=\"4\" partners=\"0\"/>"));
    assert!(xml.contains("<owners authors=\"1\" bf=\"1\"/>"));
}

#[test]
fn test_pr_context_rows_cap_with_window_and_unknown_runner() {
    let callers = (0..7)
        .map(|i| CallerFile {
            path: format!("src/caller_{i}.rs"),
            deps: 1,
        })
        .collect();
    let tests = (0..6)
        .map(|i| AffectedTest::new(format!("tests/case_{i}.rs"), 1, false, false))
        .collect();
    let file = PrFileContext {
        callers,
        tests,
        ..file_context("src/hub.rs")
    };
    let xml = xml_of(&report_of(vec![file], None, ""));
    let shown = format!("shown={MAX_DETAIL_ROWS}");
    let observed = (
        rows(&xml, "impact"),
        rows(&xml, "tests"),
        rows(&xml, "f").len(),
        rows(&xml, "test")
            .iter()
            .filter(|attrs| attrs.contains(&"run_unknown=1".to_string()))
            .count(),
    );
    assert_eq!(
        observed,
        (
            vec![vec![
                "dependents=0".into(),
                "files_other=7".into(),
                shown.clone(),
                "capped=1".into()
            ]],
            vec![vec!["count=6".into(), shown, "capped=1".into()]],
            MAX_DETAIL_ROWS,
            MAX_DETAIL_ROWS,
        )
    );
}

#[test]
fn test_pr_context_file_without_history_claims_nothing() {
    let xml = xml_of(&report_of(vec![file_context("src/fresh.rs")], None, ""));
    assert!(xml.contains("base=\"working-tree\" direction=\"worktree-since-head\""));
    assert!(!xml.contains(" at=\""));
    assert!(xml.contains("<impact dependents=\"0\" files_other=\"0\"/>"));
    assert!(xml.contains("<tests count=\"0\"/>"));
    assert!(!xml.contains("<cochange"));
    assert!(!xml.contains("<owners"));
}

#[test]
fn test_pr_context_budget_tokens_reports_the_byte_budget_at_four_bytes_per_token() {
    let mut report = report_of(vec![file_context("src/a.rs")], None, "");
    report.page = report.page.with_budget(Some(16_000 * 4));
    assert!(xml_of(&report).contains(" budget_tokens=\"16000\""));
}

#[test]
fn test_compute_pr_context_report() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let p = interner.intern("src/worker.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: p,
        lang: Lang::Rust,
        byte_size: 100,
        line_count: 10,
    });
    let sname = interner.intern("run_job");
    ingest.symbols.push(Symbol::new(
        0,
        sname,
        sname,
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    ));
    let graph = Graph::build(&ingest, &interner);
    let files = ["src/worker.rs".to_string()];
    let req = PrContextRequest {
        changed_files: &files,
        changed_lines: None,
        base_ref: Some("HEAD~1"),
        at_stamp: "",
        commits: &[],
        page: Default::default(),
    };
    let report = compute_pr_context_from_facts(&req, &ingest, &graph, &interner);
    assert_eq!(report.base_ref.as_deref(), Some("HEAD~1"));
    assert_eq!(report.files[0].history, None);
    assert_eq!(report.files_count, 1);
    assert_eq!(report.files[0].path, "src/worker.rs");
    assert_eq!(report.files[0].changed_symbols, vec!["run_job"]);
}

const ENGINE_BASE: &[u8] =
    b"def helper():\n    return 1\n\n\ndef run():\n    return helper() + 1\n\n\ndef idle():\n    return 0\n";
const ENGINE_EDITED: &[u8] =
    b"def helper():\n    return 1\n\n\ndef run():\n    return helper() + 2\n\n\ndef idle():\n    return 0\n";
const APP: &[u8] = b"from engine import run\n\n\ndef start():\n    return run()\n";
const APP_TEST: &[u8] = b"from engine import run\n\n\ndef test_run():\n    assert run() == 3\n";

fn engine_file(changed: Option<ChangedLines>) -> PrFileContext {
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            ("src/engine.py", Lang::Python, ENGINE_EDITED),
            ("src/app.py", Lang::Python, APP),
            ("tests/test_engine.py", Lang::Python, APP_TEST),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let files = ["src/engine.py".to_string()];
    let lines: BTreeMap<String, ChangedLines> = changed
        .map(|c| ("src/engine.py".to_string(), c))
        .into_iter()
        .collect();
    let req = PrContextRequest {
        changed_files: &files,
        changed_lines: Some(&lines),
        base_ref: Some("main"),
        at_stamp: "",
        commits: &[],
        page: Default::default(),
    };
    let mut report = compute_pr_context_from_facts(&req, &ingest, &graph, &interner);
    report.files.remove(0)
}

#[test]
fn test_pr_context_lists_only_symbols_owning_changed_lines_with_their_callers_and_tests() {
    let file = engine_file(Some(new_side_changes(ENGINE_BASE, ENGINE_EDITED)));
    let tests: Vec<&str> = file.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(
        (file.changed_symbols.clone(), file.callers.clone(), tests),
        (
            vec!["run".to_string()],
            vec![
                CallerFile {
                    path: "src/app.py".to_string(),
                    deps: 1,
                },
                CallerFile {
                    path: "tests/test_engine.py".to_string(),
                    deps: 1,
                },
            ],
            vec!["tests/test_engine.py"],
        )
    );
}

#[test]
fn test_pr_context_without_a_line_diff_falls_back_to_every_symbol_in_the_file() {
    assert_eq!(engine_file(None).changed_symbols, ["helper", "run", "idle"]);
}

fn touched_after(old: &[u8], new: &[u8]) -> Vec<String> {
    use ripwire_rs::domain::CorpusView;
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[("src/engine.py", Lang::Python, new)],
        &registry,
        &interner,
    );
    let view = (&ingest, &interner);
    let syms: Vec<&Symbol> = ingest.symbols.iter().collect();
    touched_symbols(&syms, &new_side_changes(old, new))
        .iter()
        .map(|s| view.sym_name(s).to_string())
        .collect()
}

/// (case, old source, new source, symbols that own a changed line)
type TouchCase<'a> = (&'a str, &'a [u8], &'a [u8], Vec<&'a str>);

#[test]
fn test_touched_symbols_attribute_edits_and_deletions_to_their_owner() {
    let deleted_inside: &[u8] = b"def helper():\n    return 1\n\n\ndef run():\n    x = 1\n    return helper() + 1\n\n\ndef idle():\n    return 0\n";
    let with_extra: &[u8] = b"def helper():\n    return 1\n\n\ndef extra():\n    return 5\n\n\ndef run():\n    return helper() + 1\n\n\ndef idle():\n    return 0\n";
    let cases: [TouchCase<'_>; 3] = [
        ("edited body", ENGINE_BASE, ENGINE_EDITED, vec!["run"]),
        (
            "line deleted inside a body",
            deleted_inside,
            ENGINE_BASE,
            vec!["run"],
        ),
        ("whole definition deleted", with_extra, ENGINE_BASE, vec![]),
    ];
    for (case, old, new, want) in cases {
        assert_eq!(touched_after(old, new), want, "{case}");
    }
}

fn history_of(authors: &[&str]) -> Option<FileHistory> {
    let interner = Interner::new();
    let ingest = IngestResult::new();
    let graph = Graph::build(&ingest, &interner);
    let commits: Vec<ripwire_rs::git::history::CommitInfo> = authors
        .iter()
        .enumerate()
        .map(|(i, author)| ripwire_rs::git::history::CommitInfo {
            id: format!("c{i}"),
            timestamp: 1_700_000_000,
            author: (*author).to_string(),
            changed_files: vec!["src/ledger.rs".to_string()],
        })
        .collect();
    let files = ["src/ledger.rs".to_string()];
    let req = PrContextRequest {
        changed_files: &files,
        changed_lines: None,
        base_ref: None,
        at_stamp: "",
        commits: &commits,
        page: Default::default(),
    };
    let mut report = compute_pr_context_from_facts(&req, &ingest, &graph, &interner);
    report.files.remove(0).history
}

#[test]
fn test_pr_context_bus_factor_flags_a_single_dominant_owner_like_owners() {
    let history = |commits, authors, bus_factor| {
        Some(FileHistory {
            commits,
            partners: 0,
            authors,
            bus_factor,
        })
    };
    let cases = [
        (vec!["ana"], history(1, 1, 1)),
        (
            vec!["ana", "ana", "ana", "ana", "ana", "ben"],
            history(6, 2, 1),
        ),
        (vec!["ana", "ben"], history(2, 2, 0)),
        (vec![], None),
    ];
    for (authors, want) in cases {
        assert_eq!(history_of(&authors), want, "{authors:?}");
    }
}
