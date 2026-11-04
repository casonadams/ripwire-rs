use ripwire_rs::{Lang, SymKind};

#[test]
fn test_python_graph_protocol_and_gates() {
    let sem = Lang::Python.semantics();
    assert!(!sem.is_entrypoint_symbol("main", None, SymKind::Function, "docs/build_docs.py"));
}
