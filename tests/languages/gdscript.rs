use ripwire_rs::Lang;

#[test]
fn test_gdscript_graph_protocol() {
    assert!(
        Lang::GDScript
            .semantics()
            .is_std_or_builtin_member("queue_free")
    );
}
