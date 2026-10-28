use ripwire_rs::gate::baseline::{
    QualitySnapshot, parse_snapshot_from_str, serialize_snapshot_to_string,
};

#[test]
fn test_snapshot_roundtrip_serialization() {
    let mut snap = QualitySnapshot {
        head_sha: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        dirty: true,
        absorbed: 2,
        ..Default::default()
    };
    snap.ccx_by_sym.insert("src/calc.rs::add".to_string(), 3);
    snap.cx_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.loc_by_sym.insert("src/calc.rs::add".to_string(), 10);
    snap.nest_by_sym.insert("src/calc.rs::add".to_string(), 1);
    snap.params_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.dead_symbols.insert("src/calc.rs::unused".to_string());
    snap.public_api.insert("src/calc.rs::add".to_string());

    let text = serialize_snapshot_to_string(&snap);
    assert!(text.contains("head abcdef1234567890abcdef1234567890abcdef12"));
    assert!(text.contains("dirty 1"));
    assert!(text.contains("ccx src/calc.rs::add 3"));

    let parsed = parse_snapshot_from_str(&text);
    assert_eq!(parsed.head_sha, snap.head_sha);
    assert!(parsed.dirty);
    assert_eq!(parsed.absorbed, 2);
    assert_eq!(parsed.ccx_by_sym["src/calc.rs::add"], 3);
    assert_eq!(parsed.params_by_sym["src/calc.rs::add"], 2);
    assert!(parsed.dead_symbols.contains("src/calc.rs::unused"));
    assert!(parsed.public_api.contains("src/calc.rs::add"));
}

#[test]
fn test_snapshot_dead_symbols_follow_the_dead_code_rule() {
    use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "src/store.rs",
                Lang::Rust,
                b"fn orphan() -> u32 {\n    1\n}\n\nstruct Store;\n\nimpl Store {\n    fn size(&self) -> u32 {\n        2\n    }\n}\n".as_slice(),
            ),
            (
                "tests/store_test.rs",
                Lang::Rust,
                b"#[test]\nfn store_is_small() {\n    assert!(true);\n}\n".as_slice(),
            ),
        ],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let snap = ripwire_rs::gate::baseline::compute_snapshot(&ingest, &graph, &interner, "head");
    let mut dead: Vec<&str> = snap.dead_symbols.iter().map(String::as_str).collect();
    dead.sort_unstable();
    assert_eq!(dead, ["src/store.rs::orphan"]);
}
