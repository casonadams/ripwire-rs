use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};

#[test]
fn test_go_graph_protocol() {
    assert!(Lang::Go.semantics().is_symbol_name_public("Serve"));
    assert!(!Lang::Go.semantics().is_symbol_name_public("handleConn"));
}

#[test]
fn test_go_method_arity_ignores_receiver_list() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let source = b"package main\ntype Store struct{}\nfunc (s *Store) Put(key string, v int) {}\nfunc run() {\n\ts := &Store{}\n\ts.Put(\"a\", 1)\n}\n";
    let ingest = ingest_in_memory_files(&[("store.go", Lang::Go, source)], &registry, &interner);
    let put = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "Put")
        .expect("Put is indexed");
    assert_eq!(
        put.params, 2,
        "the receiver list must not count as a parameter"
    );
}
