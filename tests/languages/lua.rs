use ripwire_rs::Lang;

#[test]
fn test_lua_graph_protocol() {
    assert!(Lang::Lua.semantics().is_std_or_builtin_member("insert"));
}
