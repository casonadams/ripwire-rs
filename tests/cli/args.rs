use ripwire_rs::DEFAULT_PACK_BUDGET_BYTES;
use ripwire_rs::cli::compute_effective_budget;
use ripwire_rs::commands::common::gate::validate_gate_modifiers;
use ripwire_rs::query::graph::split_endpoint_pair;

#[test]
fn test_gate_modifiers_refused_without_their_verb() {
    assert!(validate_gate_modifiers(true, false, false, false).is_err());
    assert!(validate_gate_modifiers(false, false, true, false).is_err());
    assert!(validate_gate_modifiers(true, true, true, true).is_ok());
}

#[test]
fn test_compute_effective_budget() {
    assert_eq!(compute_effective_budget(Some(1024), None, None), Some(1024));
    assert_eq!(compute_effective_budget(None, Some(100), None), Some(400));
    assert_eq!(compute_effective_budget(None, None, Some(50)), Some(200));
    assert_eq!(
        compute_effective_budget(None, None, None),
        Some(DEFAULT_PACK_BUDGET_BYTES)
    );
    assert_eq!(split_endpoint_pair("a,b"), ("a", "b"));
    assert_eq!(split_endpoint_pair("a:b"), ("a", "b"));
    assert_eq!(split_endpoint_pair("a->b"), ("a", "b"));
    assert_eq!(split_endpoint_pair("single"), ("single", ""));
}

#[test]
fn test_exclude_is_repeatable_and_splits_comma_lists() {
    let cli = ripwire_rs::parse_from(["rw", "--exclude=vendor", "--exclude=dist,build"]).unwrap();
    let opts = ripwire_rs::commands::context::crawler_options_for_cli(&cli);
    assert_eq!(opts.custom_excludes, ["vendor", "dist", "build"]);
}

/// (`--quality-delta` requested, its base, the positional path)
type DeltaParse = (bool, Option<String>, Option<String>);

#[test]
fn test_quality_delta_takes_a_base_only_with_equals() {
    use ripwire_rs::commands::quality_delta::QualityDeltaFlags;
    let parse = |args: &[&str]| -> DeltaParse {
        let cli = ripwire_rs::parse_from(args.iter().copied()).unwrap();
        let flags = cli
            .plugins
            .decode::<QualityDeltaFlags>()
            .unwrap_or_default();
        (
            flags.primary.requested(),
            flags.primary.base().map(str::to_string),
            cli.path.clone(),
        )
    };
    let cases: [(&[&str], DeltaParse); 4] = [
        (&["rw", "--quality-delta"], (true, None, None)),
        (
            &["rw", "--quality-delta", "."],
            (true, None, Some(".".into())),
        ),
        (
            &["rw", "--quality-delta=main..HEAD"],
            (true, Some("main..HEAD".into()), None),
        ),
        (&["rw", "."], (false, None, Some(".".into()))),
    ];
    for (args, want) in cases {
        assert_eq!(parse(args), want, "{args:?}");
    }
}
