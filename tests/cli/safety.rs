use ripwire_rs::commands::common::changes::resolve_changed_or_spec_files;
use ripwire_rs::commands::common::gate::validate_gate_modifiers;
use ripwire_rs::commands::common::safety_queries::*;
use ripwire_rs::git::history::CommitInfo;
use ripwire_rs::query::safety::SafetyContext;
use ripwire_rs::serialize::{PageRequest, XmlReport};
use ripwire_rs::skills::{SkillScanFinding, SkillScanReport};
use ripwire_rs::{Graph, IngestResult, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

fn sample_corpus(files: &[(&str, Lang, &[u8])]) -> (IngestResult, Graph, Interner) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    (ingest, graph, interner)
}

#[test]
fn test_resolve_changed_or_spec_files_pure_facts() {
    let files =
        resolve_changed_or_spec_files("any-root", Some("src/alpha.rs, src/beta.rs, src/gamma.rs "));
    assert_eq!(files, vec!["src/alpha.rs", "src/beta.rs", "src/gamma.rs"]);

    let empty = resolve_changed_or_spec_files("any-root", Some(""));
    assert!(empty.is_empty());
}

#[test]
fn test_render_skill_scan_report_verdicts_and_xml() {
    let mut out = Vec::new();
    let clean_rep = SkillScanReport {
        files_scanned: 5,
        findings: Vec::new(),
        verdict: "clean",
    };
    let code_clean = render_skill_scan_report(&clean_rep, &mut out).unwrap();
    assert_eq!(code_clean, 0);

    let mut out = Vec::new();
    let warn_rep = SkillScanReport {
        files_scanned: 2,
        findings: vec![SkillScanFinding {
            path: "skills/tool.md".to_string(),
            line: 12,
            rule: "exfil-domain",
            sev: "warn",
        }],
        verdict: "warn",
    };
    let code_warn = render_skill_scan_report(&warn_rep, &mut out).unwrap();
    assert_eq!(code_warn, 1);

    let mut out = Vec::new();
    let crit_rep = SkillScanReport {
        files_scanned: 3,
        findings: vec![SkillScanFinding {
            path: "skills/leak.md".to_string(),
            line: 4,
            rule: "token-theft",
            sev: "critical",
        }],
        verdict: "critical",
    };
    let code_crit = render_skill_scan_report(&crit_rep, &mut out).unwrap();
    assert_eq!(code_crit, 2);
}

#[test]
fn test_execute_edit_plan_from_facts_behavior() {
    let (ingest, graph, interner) =
        sample_corpus(&[("src/lib.rs", Lang::Rust, b"pub fn original() {}\n")]);
    let ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let mut out = Vec::new();
    let malformed_plan = "{not_json";
    let code_err = execute_edit_plan_from_facts(&ctx, malformed_plan, true, &mut out).unwrap();
    assert_eq!(code_err, 1);
    let output_err = String::from_utf8(out).unwrap();
    assert!(output_err.contains("\"error\""));
}

#[test]
fn test_execute_pr_context_from_facts() {
    let (ingest, graph, interner) =
        sample_corpus(&[("src/lib.rs", Lang::Rust, b"pub fn worker() {}\n")]);
    let ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    let commits = [CommitInfo {
        id: "c1".to_string(),
        timestamp: 1_700_000_000,
        author: "Dev".to_string(),
        changed_files: vec!["src/lib.rs".to_string()],
    }];
    let mut out = Vec::new();
    let changed = vec!["src/lib.rs".to_string()];
    let pr_req = ripwire_rs::gate::pr_context::PrContextRequest {
        changed_files: &changed,
        changed_lines: None,
        base_ref: Some("main"),
        at_stamp: "",
        commits: &commits,
        page: ripwire_rs::serialize::PageRequest::default(),
    };
    let rep = execute_pr_context_from_facts(&ctx, &pr_req, &mut out).unwrap();
    assert_eq!(rep.base_ref.as_deref(), Some("main"));
    assert_eq!(rep.files_count, 1);
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("<pr-context schema=\"ripwire-rs.pr-context/v1\""));
    assert!(xml.contains("base=\"main\""));
    assert!(xml.contains("<file p=\"src/lib.rs\" symbols=\"1\">"));
}

#[test]
fn test_execute_test_gate_and_situ_from_facts() {
    let (ingest, graph, interner) = sample_corpus(&[
        ("src/lib.rs", Lang::Rust, b"pub fn target() {}\n"),
        (
            "tests/test_lib.rs",
            Lang::Rust,
            b"pub fn test_target() { target(); }\n",
        ),
    ]);
    let ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let mut out = Vec::new();
    let changed = vec!["src/lib.rs".to_string()];
    let empty_sources: [(&str, &[u8]); 0] = [];
    let empty_gates = ripwire_rs::gate::script_gates::ScriptGates::default();
    let req = TestGateRequest {
        files: &changed,
        at_stamp: "head_stamp_abc",
        page: PageRequest::default(),
        sources: &empty_sources,
        gates: &empty_gates,
    };
    let has_findings = execute_test_gate_from_facts(&ctx, &req, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("<test-gate schema=\"ripwire-rs.test-gate/v1\""));
    assert!(xml.contains("at=\"head_stamp_abc\""));
    assert!(has_findings);
    assert!(xml.contains("<t p=\"tests/test_lib.rs\""));
    let mut out = Vec::new();
    let commits = [CommitInfo {
        id: "c1".to_string(),
        timestamp: 1_700_000_000,
        author: "Dev".to_string(),
        changed_files: vec!["src/lib.rs".to_string()],
    }];
    execute_situ_from_facts(&ctx, &changed, Some(&commits), Default::default(), &mut out).unwrap();
    let situ_xml = String::from_utf8(out).unwrap();
    assert!(situ_xml.contains("<situ schema=\"ripwire-rs.situ/v1\""));
    assert!(situ_xml.contains("<changed path=\"src/lib.rs\"/>"));
}
#[test]
fn test_validate_gate_modifiers_policy() {
    let err_dirty = validate_gate_modifiers(true, false, false, false).unwrap_err();
    assert!(
        err_dirty
            .to_string()
            .contains("--allow-dirty modifies --quality-baseline")
    );

    let err_stale = validate_gate_modifiers(false, false, true, false).unwrap_err();
    assert!(
        err_stale
            .to_string()
            .contains("--show-stale modifies --quality-delta")
    );

    assert!(validate_gate_modifiers(true, true, false, false).is_ok());
}

#[test]
fn seams_report_preserves_the_uncovered_edge() {
    let (ingest, graph, interner) = sample_corpus(&[
        ("api/entry.rs", Lang::Rust, b"pub fn entry() { store(); }\n"),
        ("data/store.rs", Lang::Rust, b"pub fn store() {}\n"),
    ]);
    let mut out = Vec::new();
    let rep = ripwire_rs::gate::seams::find_integration_seams(
        &ingest,
        &graph,
        &interner,
        PageRequest::default(),
    );
    rep.write_xml_report(":memory:", &mut out).unwrap();
    let report = roxmltree::Document::parse(std::str::from_utf8(&out).unwrap()).unwrap();
    assert_eq!(report.root_element().tag_name().name(), "seams");
    assert_eq!(report.root_element().attribute("untested"), Some("1"));
    let edge = report
        .descendants()
        .find(|node| node.has_tag_name("edge"))
        .unwrap();
    assert_eq!(
        (
            edge.attribute("caller"),
            edge.attribute("p"),
            edge.attribute("callee"),
            edge.attribute("cp")
        ),
        (
            Some("entry"),
            Some("api/entry.rs:1"),
            Some("store"),
            Some("data/store.rs:1")
        ),
    );
}

fn safe_delete_xml(files: &[(&str, Lang, &[u8])], sym: &str) -> String {
    let (ingest, graph, interner) = sample_corpus(files);
    let facts = SafetyContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    let mut out = Vec::new();
    execute_safe_delete_query(&facts, sym, PageRequest::default(), &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn test_safe_delete_radius_and_defs_come_from_the_graph() {
    let xml = safe_delete_xml(
        &[
            (
                "src/lib.rs",
                Lang::Rust,
                b"pub fn alpha(x: u8) -> u8 { x }\npub fn run() { let _ = alpha(1); }\n",
            ),
            (
                "tests/check.rs",
                Lang::Rust,
                b"fn checks() { assert_eq!(alpha(2), 2); }\n",
            ),
        ],
        "alpha",
    );
    assert!(xml.contains(" defs=\"1\""), "{xml}");
    assert!(xml.contains(" callers=\"2\""), "{xml}");
    assert!(xml.contains(" impact_reaches=\"2\""), "{xml}");
    assert!(
        xml.contains(" radius_tested=\"1\" radius_untested=\"1\""),
        "{xml}"
    );
    assert!(xml.contains(" dead_code_candidate=\"0\""), "{xml}");
}

#[test]
fn test_safe_delete_counts_each_definition_of_a_shared_name() {
    let xml = safe_delete_xml(
        &[
            ("src/a.rs", Lang::Rust, b"pub fn helper() {}\n"),
            (
                "src/b.rs",
                Lang::Rust,
                b"pub fn helper() {}\npub fn caller() { helper(); }\n",
            ),
        ],
        "helper",
    );
    assert!(xml.contains(" defs=\"2\""), "{xml}");
}

#[test]
fn test_safe_delete_macro_argument_call_is_not_a_low_risk_dead_symbol() {
    let xml = safe_delete_xml(
        &[
            (
                "src/lib.rs",
                Lang::Rust,
                b"pub fn alpha(x: u8) -> u8 { x }\n",
            ),
            (
                "tests/check.rs",
                Lang::Rust,
                b"fn checks_alpha() { assert_eq!(alpha(2), 4); }\n",
            ),
        ],
        "alpha",
    );
    assert!(xml.contains(" callers=\"1\""), "{xml}");
    assert!(xml.contains(" risk=\"uses-exist\""), "{xml}");
    assert!(xml.contains(" dead_code_candidate=\"0\""), "{xml}");
}
