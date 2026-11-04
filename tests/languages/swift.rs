use ripwire_rs::Lang;

#[test]
fn test_swift_graph_protocol() {
    assert!(Lang::Swift.semantics().is_std_or_builtin_member("append"));
}
