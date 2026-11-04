use ripwire_rs::Lang;

#[test]
fn test_objc_graph_protocol() {
    assert!(
        Lang::ObjC
            .semantics()
            .is_std_or_builtin_qualifier("NSString")
    );
    assert!(Lang::ObjC.semantics().is_std_or_builtin_member("alloc"));
}
