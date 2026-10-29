use ripwire_rs::commands::pr_context::{PrContextTarget, pr_context_target};

#[test]
fn test_pr_context_value_is_a_ref_unless_only_a_directory_matches() {
    use PrContextTarget::{Path, Ref};
    let cases = [
        (("main", true, false), Ref("main")),
        (("main", true, true), Ref("main")),
        ((".", false, true), Path(".")),
        (("nosuchref", false, false), Ref("nosuchref")),
    ];
    for ((value, is_revision, is_dir), want) in cases {
        assert_eq!(
            pr_context_target(value, is_revision, is_dir),
            want,
            "{value}"
        );
    }
}
