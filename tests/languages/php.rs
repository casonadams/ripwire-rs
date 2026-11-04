use ripwire_rs::Lang;

#[test]
fn test_php_graph_protocol() {
    assert!(Lang::Php.semantics().is_std_or_builtin_member("explode"));
}
