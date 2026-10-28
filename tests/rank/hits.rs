use ripwire_rs::{
    EdgeAcc, EdgeProv, Graph, HitsConfig, IngestResult, Interner, Lang, ResolutionResult, SymKind,
    Symbol, compute_hits,
};
use std::collections::HashMap;

#[test]
fn test_hits_empty_graph() {
    let interner = Interner::new();
    let ingest = IngestResult::new();
    let graph = Graph::build(&ingest, &interner);
    let res = compute_hits(&graph, &HitsConfig::default());
    assert_eq!(res.authority.len(), 0);
    assert_eq!(res.hub.len(), 0);
    assert!(res.converged);
}

#[test]
fn test_hits_two_node_chain() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let f = interner.intern("test.rs");
    let n0 = interner.intern("caller");
    let n1 = interner.intern("callee");

    ingest
        .symbols
        .push(Symbol::new(0, f, n0, SymKind::Function, Lang::Rust, 0, 1));
    ingest
        .symbols
        .push(Symbol::new(1, f, n1, SymKind::Function, Lang::Rust, 0, 10));

    let resolution = ResolutionResult {
        edge_accumulators: HashMap::from([(
            (0, 1),
            EdgeAcc {
                n_ref: 1,
                calls: 1,
                conf_sum: 1.0,
                prov: EdgeProv::Local,
            },
        )]),
        amb_out: vec![0, 0],
        unresolved: 0,
        external: 0,
        declined: 0,
    };
    let graph = Graph::from_resolution(2, resolution);
    let res = compute_hits(&graph, &HitsConfig::default());

    assert_eq!(res.authority.len(), 2);
    assert_eq!(res.hub.len(), 2);
    assert!(res.converged);
    assert!(res.authority[1] > res.authority[0]);
    assert!(res.hub[0] > res.hub[1]);
}

/// Authority of `helper` (six callers in one test file) and `shared_core` (one caller in each of
/// three files) under raw HITS and under HITS that counts calling files.
fn helper_and_core_authority() -> ((f64, f64), (f64, f64)) {
    let registry =
        ripwire_rs::LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let tests: String = (1..=6)
        .map(|i| format!("#[test]\nfn case_{i}() {{\n    helper();\n}}\n\n"))
        .collect::<String>()
        + "fn helper() -> u32 {\n    1\n}\n";
    let files = [
        ("tests/ledger_test.rs", tests),
        (
            "src/core.rs",
            "pub fn shared_core() -> u32 {\n    1\n}\n".to_string(),
        ),
        (
            "src/alpha.rs",
            "pub fn alpha() -> u32 {\n    shared_core()\n}\n".to_string(),
        ),
        (
            "src/beta.rs",
            "pub fn beta() -> u32 {\n    shared_core()\n}\n".to_string(),
        ),
        (
            "src/gamma.rs",
            "pub fn gamma() -> u32 {\n    shared_core()\n}\n".to_string(),
        ),
    ];
    let slices: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(p, s)| (*p, Lang::Rust, s.as_bytes()))
        .collect();
    let ingest = ripwire_rs::ingest_in_memory_files(&slices, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let id = |name: &str| {
        ingest
            .symbols
            .iter()
            .find(|s| interner.resolve(&s.name) == name)
            .map(|s| s.id as usize)
            .unwrap_or_else(|| panic!("{name} indexed"))
    };
    let node_files: Vec<u32> = ingest.symbols.iter().map(|s| s.file_id).collect();
    let raw = compute_hits(&graph, &HitsConfig::default()).authority;
    let by_file =
        ripwire_rs::compute_hits_by_file(&graph, &node_files, &HitsConfig::default()).authority;
    let pick = |a: &[f64]| (a[id("helper")], a[id("shared_core")]);
    (pick(&raw), pick(&by_file))
}

#[test]
fn test_authority_by_file_prefers_reliance_across_files_over_one_file_of_callers() {
    let ((raw_helper, raw_core), (file_helper, file_core)) = helper_and_core_authority();
    assert_eq!(
        (raw_helper > raw_core, file_core > file_helper),
        (true, true)
    );
}
