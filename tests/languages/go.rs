use crate::index_support::index;
use ripwire_rs::Lang;

#[test]
fn test_go_graph_protocol() {
    assert!(Lang::Go.semantics().is_symbol_name_public("Serve"));
    assert!(!Lang::Go.semantics().is_symbol_name_public("handleConn"));
}

#[test]
fn test_go_method_arity_ignores_receiver_list() {
    let source = b"package main\ntype Store struct{}\nfunc (s *Store) Put(key string, v int) {}\nfunc run() {\n\ts := &Store{}\n\ts.Put(\"a\", 1)\n}\n";
    let (ingest, _, interner) = index(&[("store.go", Lang::Go, source)]);
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
