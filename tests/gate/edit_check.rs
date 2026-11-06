use ripwire_rs::query::safety::{
    EditCheckRequest, SafetyContext, affected_from_sources, write_edit_check_from_facts,
};
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, RefRole, Reference, SignatureContract, SymKind,
    Symbol, ToXmlString, check_symbol_contract, find_affected_tests,
};
use std::io::Cursor;

fn setup_test_repository(interner: &Interner) -> IngestResult {
    let mut ingest = IngestResult::new();

    let p_service = interner.intern("src/service.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: p_service,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });

    let p_legacy = interner.intern("src/legacy_caller.rs");
    ingest.files.push(FileInfo {
        id: 1,
        path: p_legacy,
        lang: Lang::Rust,
        line_count: 40,
        byte_size: 800,
    });

    let p_modern = interner.intern("src/modern_caller.rs");
    ingest.files.push(FileInfo {
        id: 2,
        path: p_modern,
        lang: Lang::Rust,
        line_count: 40,
        byte_size: 800,
    });

    let p_test = interner.intern("tests/test_service.rs");
    ingest.files.push(FileInfo {
        id: 3,
        path: p_test,
        lang: Lang::Rust,
        line_count: 60,
        byte_size: 1200,
    });

    let s_compute = interner.intern("compute");
    let mut sym_compute = Symbol::new(
        0,
        s_compute,
        s_compute,
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    sym_compute.params = 3;
    sym_compute.required_params = 3;

    let s_legacy = interner.intern("legacy_worker");
    let sym_legacy = Symbol::new(1, s_legacy, s_legacy, SymKind::Function, Lang::Rust, 1, 15);

    let s_modern = interner.intern("modern_worker");
    let sym_modern = Symbol::new(2, s_modern, s_modern, SymKind::Function, Lang::Rust, 2, 20);

    let s_test = interner.intern("test_integration");
    let sym_test = Symbol::new(3, s_test, s_test, SymKind::Function, Lang::Rust, 3, 25);

    ingest.symbols.push(sym_compute);
    ingest.symbols.push(sym_legacy);
    ingest.symbols.push(sym_modern);
    ingest.symbols.push(sym_test);

    let mut r1 = Reference::new(1, s_compute, 1, 18, Lang::Rust, RefRole::Call);
    r1.arg_count = 2;
    r1.arg_count_known = true;

    let mut r2 = Reference::new(2, s_compute, 2, 22, Lang::Rust, RefRole::Call);
    r2.arg_count = 3;
    r2.arg_count_known = true;

    let mut r3 = Reference::new(3, s_legacy, 3, 28, Lang::Rust, RefRole::Call);
    r3.arg_count = 0;
    r3.arg_count_known = true;

    ingest.references.push(r1);
    ingest.references.push(r2);
    ingest.references.push(r3);

    ingest
}

#[test]
fn test_caller_compatibility_verification() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();

    let p = interner.intern("src/calc.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: p,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });

    let target_name = interner.intern("compute");
    let mut target_sym = Symbol::new(
        0,
        target_name,
        target_name,
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    target_sym.params = 3;
    target_sym.required_params = 3;

    let caller_name = interner.intern("runner");
    let caller_sym = Symbol::new(
        1,
        caller_name,
        caller_name,
        SymKind::Function,
        Lang::Rust,
        0,
        20,
    );

    ingest.symbols.push(target_sym);
    ingest.symbols.push(caller_sym);

    let mut ref_call = Reference::new(1, target_name, 0, 25, Lang::Rust, RefRole::Call);
    ref_call.arg_count = 2;
    ref_call.arg_count_known = true;
    ingest.references.push(ref_call);

    let graph = Graph::build(&ingest, &interner);

    let baseline = SignatureContract {
        name: "compute".to_string(),
        params: 2,
        required_params: 2,
        is_public: true,
    };

    let report = check_symbol_contract(0, Some(&baseline), &ingest, &graph, &interner).unwrap();
    assert_eq!(report.symbol_name, "compute");
    assert_eq!(report.total_callers, 1);
    assert_eq!(report.incompatible_callers, 1);
    assert!(report.callers[0].is_incompatible);
    assert_eq!(report.callers[0].sites.len(), 1);
    assert_eq!(report.callers[0].sites[0].arg_count, 2);
    assert_eq!(report.callers[0].sites[0].required_params, 3);

    let xml = report.to_xml_string();
    assert!(xml.contains("status=\"contract-change\""));
    assert!(xml.contains("incompatible=\"1\""));
    assert!(xml.contains("<site line=\"25\" args=\"2\" required=\"3\"/>"));
}

#[test]
fn test_edit_check_arity_break() {
    let interner = Interner::new();
    let ingest = setup_test_repository(&interner);
    let graph = Graph::build(&ingest, &interner);

    let baseline = SignatureContract {
        name: "compute".to_string(),
        params: 2,
        required_params: 2,
        is_public: true,
    };

    let report = check_symbol_contract(0, Some(&baseline), &ingest, &graph, &interner)
        .expect("check_symbol_contract report");

    assert_eq!(report.symbol_name, "compute");
    assert_eq!(report.total_callers, 2);
    assert_eq!(report.incompatible_callers, 1);

    let legacy_caller = report
        .callers
        .iter()
        .find(|c| c.caller_name == "legacy_worker")
        .expect("legacy_worker caller");
    assert!(legacy_caller.is_incompatible);
    assert_eq!(legacy_caller.sites.len(), 1);
    assert_eq!(legacy_caller.sites[0].arg_count, 2);
    assert_eq!(legacy_caller.sites[0].required_params, 3);
    assert_eq!(legacy_caller.sites[0].line, 18);

    let modern_caller = report
        .callers
        .iter()
        .find(|c| c.caller_name == "modern_worker")
        .expect("modern_worker caller");
    assert!(!modern_caller.is_incompatible);
    assert!(modern_caller.sites.is_empty());

    let xml = report.to_xml_string();
    assert!(xml.contains("<edit-check schema=\"ripwire-rs.edit-check/v1\" sym=\"compute\" t=\"fn\" p=\"src/service.rs:10\" status=\"contract-change\" defs=\"1\" callers=\"2\" incompatible=\"1\""));
    assert!(
        xml.contains("<c n=\"legacy_worker\" p=\"src/legacy_caller.rs:15\" incompatible=\"1\">")
    );
    assert!(xml.contains("<site line=\"18\" args=\"2\" required=\"3\"/>"));
    assert!(xml.contains("<c n=\"modern_worker\" p=\"src/modern_caller.rs:20\"/>"));
}

#[test]
fn test_edit_check_clean_unchanged_emits_rows_and_honors_limit() {
    let interner = Interner::new();
    let ingest = setup_test_repository(&interner);
    let graph = Graph::build(&ingest, &interner);
    let mut report = check_symbol_contract(0, None, &ingest, &graph, &interner).unwrap();
    report.status = ripwire_rs::ContractStatus::Unchanged;
    report
        .callers
        .iter_mut()
        .for_each(|c| c.is_incompatible = false);
    report
        .callers
        .sort_by(|a, b| a.caller_name.cmp(&b.caller_name));
    report.incompatible_callers = 0;
    report.at_stamp = "6e9b85eb8".to_string();

    let xml = report.to_xml_string();
    assert!(!xml.contains("rows_next="));
    assert!(!xml.contains("shown_unflagged="));
    assert!(xml.contains("<c n=\"legacy_worker\" p=\"src/legacy_caller.rs:15\""));
    assert!(xml.contains("<c n=\"modern_worker\" p=\"src/modern_caller.rs:20\"/>"));

    report.page = Some((1, 1));
    let paged = report.to_xml_string();
    assert!(paged.contains(" shown_unflagged=\"1\" unflagged_capped=\"1\" total=\"2\" has_more=\"0\" next_offset=\"2\" offset=\"1\" limit=\"1\""));
    assert!(!paged.contains("legacy_worker"));
    assert!(paged.contains("<c n=\"modern_worker\" p=\"src/modern_caller.rs:20\"/>"));
}

#[test]
fn test_affected_tests_transitive_reachability() {
    let interner = Interner::new();
    let ingest = setup_test_repository(&interner);
    let graph = Graph::build(&ingest, &interner);

    let report = find_affected_tests(&["src/service.rs"], &ingest, &graph, &interner);
    assert_eq!(report.seed_count, 1);
    assert_eq!(report.total_tests, 1);

    let test = &report.tests[0];
    assert_eq!(test.path, "tests/test_service.rs");
    assert_eq!(test.hops, 2);
    assert!(!test.is_direct_change);

    assert_eq!(report.reached_symbols, 4);
    let xml = report.to_xml_string();
    assert!(xml.contains(
        "<affected schema=\"ripwire-rs.affected/v1\" changed=\"src/service.rs\" seeded_by=\"file\""
    ));
    assert!(xml.contains(
        "<test p=\"tests/test_service.rs\" partner=\"1\" hops=\"2\" run_unknown=\"1\"/>"
    ));
}

#[test]
fn test_edit_check_and_affected_from_in_memory_facts() {
    let interner = Interner::new();
    let ingest = setup_test_repository(&interner);
    let graph = Graph::build(&ingest, &interner);
    let state = SafetyContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    let baseline = SignatureContract {
        name: "compute".to_string(),
        params: 2,
        required_params: 2,
        is_public: true,
    };
    let mut out = Cursor::new(Vec::new());
    write_edit_check_from_facts(
        &EditCheckRequest {
            sym_name: "compute",
            head_contract: Some(&baseline),
            page: None,
            at_stamp: Some("fixture-head"),
        },
        &state,
        &mut out,
    )
    .expect("write edit-check from supplied contract");
    let xml = String::from_utf8(out.into_inner()).unwrap();
    assert!(xml.contains("status=\"contract-change\""));
    assert!(xml.contains("callers=\"2\" incompatible=\"1\""));
    assert!(xml.contains("at=\"fixture-head\""));
    assert!(xml.contains("<site line=\"18\" args=\"2\" required=\"3\"/>"));
    assert!(xml.contains("<c n=\"modern_worker\" p=\"src/modern_caller.rs:20\"/>"));

    let sources: [(&str, &[u8]); 1] = [(
        "tests/test_service.rs",
        b"#[test]\nfn test_integration() { legacy_worker(); }\n",
    )];
    let affected = affected_from_sources("src/service.rs", &state, &sources);
    assert_eq!(affected.seed_count, 1);
    assert_eq!(affected.total_tests, 1);
    assert_eq!(affected.tests[0].hops, 2);
    assert!(!affected.tests[0].is_direct_change);
    assert!(affected.tests[0].run.is_none());
    let xml = affected.to_xml_string();
    assert!(xml.contains("<test p=\"tests/test_service.rs\""));
    assert!(xml.contains("hops=\"2\""));
    assert!(xml.contains("run_unknown=\"1\""));
}

#[test]
fn test_edit_check_reports_real_kind_and_definition_count() {
    use ripwire_rs::{LanguageRegistry, ToXmlString, ingest_in_memory_files};
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "src/a.rs",
                Lang::Rust,
                b"pub struct Pt { x: i32 }\npub fn helper() {}\n",
            ),
            ("src/b.rs", Lang::Rust, b"pub fn helper() {}\n"),
        ],
        &reg,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let pt = ripwire_rs::find_symbols_by_name("Pt", &ingest, &interner)[0];
    let xml = check_symbol_contract(pt, None, &ingest, &graph, &interner)
        .unwrap()
        .to_xml_string();
    assert!(xml.contains(" t=\"cls\""), "{xml}");
    assert!(xml.contains(" defs=\"1\""), "{xml}");

    let helper = ripwire_rs::find_symbols_by_name("helper", &ingest, &interner)[0];
    let xml = check_symbol_contract(helper, None, &ingest, &graph, &interner)
        .unwrap()
        .to_xml_string();
    assert!(xml.contains(" t=\"fn\""), "{xml}");
    assert!(xml.contains(" defs=\"2\""), "{xml}");
}
