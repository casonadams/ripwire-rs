use ripwire_rs::Lang;

#[test]
fn test_elixir_graph_protocol() {
    assert!(Lang::Elixir.semantics().is_std_or_builtin_member("map/2"));
}
