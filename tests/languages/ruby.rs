use ripwire_rs::Lang;

#[test]
fn test_ruby_graph_protocol() {
    assert!(Lang::Ruby.semantics().is_std_or_builtin_member("each"));
}
