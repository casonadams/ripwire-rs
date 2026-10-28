use ripwire_rs::crawler::{
    CrawledFile, CrawlerOptions, DEFAULT_JSON_CEILING, DEFAULT_YAML_CEILING, SkippedCensus,
    classify_skipped_item, evaluate_candidate, is_binary_buffer,
};
use ripwire_rs::domain::Lang;

#[test]
fn test_crawl_deterministic_and_denylist() {
    let candidates = [
        ("src/b.rs", 10, false),
        ("src/a.rs", 10, false),
        ("target/ign.rs", 10, false),
        (".git/config", 50, false),
        ("src/unknown.xyz", 10, false),
        ("binary.rs", 10, true),
        ("empty.rs", 0, false),
        ("Cargo.toml", 20, false),
    ];

    let opts = CrawlerOptions::default();
    let mut files: Vec<CrawledFile> = candidates
        .iter()
        .filter_map(|&(rel_path, size, is_bin)| evaluate_candidate(rel_path, size, is_bin, &opts))
        .collect();

    files.sort_by(|a, b| a.rel_path.as_bytes().cmp(b.rel_path.as_bytes()));

    assert_eq!(files.len(), 3);
    assert_eq!(files[0].rel_path, "Cargo.toml");
    assert_eq!(files[0].lang, Lang::Toml);
    assert_eq!(files[1].rel_path, "src/a.rs");
    assert_eq!(files[2].rel_path, "src/b.rs");
    assert_eq!(files[1].lang, Lang::Rust);
    assert!(is_binary_buffer(b"\0hello"));
    assert!(!is_binary_buffer(b"hello world"));
}

#[test]
fn test_crawl_scope_ignore_tests_and_max_file_size() {
    let scoped = CrawlerOptions {
        max_file_size: 100,
        ignore_tests: true,
        ..CrawlerOptions::default()
    };
    let kept = |rel: &str, size: u64| evaluate_candidate(rel, size, false, &scoped).is_some();
    assert!(kept("src/lib.rs", 100));
    assert!(!kept("src/lib.rs", 101));
    assert!(!kept("tests/test_cli.rs", 10));
    assert!(!kept("src/foo_test.go", 10));
    let defaults = CrawlerOptions::default();
    assert!(evaluate_candidate("tests/test_cli.rs", 10, false, &defaults).is_some());
    assert!(evaluate_candidate("src/lib.rs", 101, false, &defaults).is_some());
    assert!(evaluate_candidate("data.json", DEFAULT_JSON_CEILING, false, &defaults).is_some());
    assert!(evaluate_candidate("data.json", DEFAULT_JSON_CEILING + 1, false, &defaults).is_none());
    assert!(
        evaluate_candidate("config.yaml", DEFAULT_YAML_CEILING + 1, false, &defaults).is_none()
    );
    assert!(evaluate_candidate(".github/workflows/ci.yml", 20, false, &defaults).is_some());
    assert!(evaluate_candidate(".eslintrc.json", 20, false, &defaults).is_some());
    assert!(evaluate_candidate(".vscode/settings.json", 20, false, &defaults).is_none());
    assert!(evaluate_candidate(".claude/settings.json", 20, false, &defaults).is_none());
    for generated in ["api/x.pb.go", "m_pb2.py", "web/app.min.js"] {
        assert!(
            evaluate_candidate(generated, 20, false, &defaults).is_none(),
            "{generated}"
        );
    }
    for kept in ["api/x.pb.ts", "y_gen.go", "web/app.min.mjs"] {
        assert!(
            evaluate_candidate(kept, 20, false, &defaults).is_some(),
            "{kept}"
        );
    }
}

#[test]
fn test_single_file_candidate_keeps_typed_path_and_applies_file_rules() {
    use ripwire_rs::crawler::single_file_candidate;
    use std::path::Path;
    let defaults = CrawlerOptions::default();
    let no_tests = CrawlerOptions {
        ignore_tests: true,
        ..CrawlerOptions::default()
    };
    let small = CrawlerOptions {
        max_file_size: 100,
        ..CrawlerOptions::default()
    };
    type Case<'a> = (&'a str, u64, &'a CrawlerOptions, Option<(&'a str, Lang)>);
    let cases: &[Case<'_>] = &[
        (
            "data/member/lib/a.dart",
            10,
            &defaults,
            Some(("data/member/lib/a.dart", Lang::Dart)),
        ),
        ("a.dart", 10, &defaults, Some(("a.dart", Lang::Dart))),
        (
            "/abs/build/a.rs",
            10,
            &defaults,
            Some(("/abs/build/a.rs", Lang::Rust)),
        ),
        ("lib/notes.xyz", 10, &defaults, None),
        ("api/x.pb.go", 10, &defaults, None),
        ("lib/a.rs", 101, &small, None),
        ("tests/helper.py", 10, &no_tests, None),
        (
            "tests/a_test.go",
            10,
            &defaults,
            Some(("tests/a_test.go", Lang::Go)),
        ),
    ];
    for &(typed, size, opts, expected) in cases {
        let got = single_file_candidate(Path::new(typed), size, opts)
            .map(|f| (f.rel_path, f.lang, f.path));
        assert_eq!(
            got.as_ref().map(|(rel, lang, _)| (rel.as_str(), *lang)),
            expected,
            "{typed}"
        );
        if let Some((_, _, path)) = got {
            assert_eq!(path, Path::new(typed));
        }
    }
}

#[test]
fn test_classify_skipped_item_all_branches() {
    use std::collections::HashMap;
    let mut census = SkippedCensus::default();
    let mut ext_counts = HashMap::new();
    let opts = CrawlerOptions {
        custom_excludes: vec!["vendor/*".to_string()],
        max_file_size: 1_000,
        ..CrawlerOptions::default()
    };

    // Directory is skipped without modifying census
    classify_skipped_item("src/dir", true, 0, &opts, &mut census, &mut ext_counts);
    assert!(census.excluded.is_empty());
    assert!(census.oversize.is_empty());
    assert!(census.unsupported.is_empty());

    // Custom exclude path
    classify_skipped_item(
        "vendor/lib.rs",
        false,
        50,
        &opts,
        &mut census,
        &mut ext_counts,
    );
    assert_eq!(census.excluded.len(), 1);
    assert_eq!(
        census.excluded[0],
        ("vendor/lib.rs".to_string(), 50, "rs".to_string())
    );

    // Unknown language with extension increments ext_counts and adds to unsupported
    classify_skipped_item(
        "assets/logo.png",
        false,
        200,
        &opts,
        &mut census,
        &mut ext_counts,
    );
    assert_eq!(census.unsupported.len(), 1);
    assert_eq!(
        census.unsupported[0],
        ("assets/logo.png".to_string(), 200, "png".to_string())
    );
    assert_eq!(ext_counts.get("png"), Some(&1));

    // Unknown language without extension adds to unsupported but does not increment ext_counts
    classify_skipped_item(
        "LICENSE_UNKNOWN",
        false,
        150,
        &opts,
        &mut census,
        &mut ext_counts,
    );
    assert_eq!(census.unsupported.len(), 2);
    assert_eq!(
        census.unsupported[1],
        ("LICENSE_UNKNOWN".to_string(), 150, String::new())
    );

    // Known language within size limit is neither unsupported nor oversize
    classify_skipped_item(
        "src/main.rs",
        false,
        500,
        &opts,
        &mut census,
        &mut ext_counts,
    );
    assert_eq!(census.oversize.len(), 0);
    assert_eq!(census.unsupported.len(), 2);

    // Known language exceeding size limit adds to oversize
    classify_skipped_item(
        "src\\large.rs",
        false,
        5000,
        &opts,
        &mut census,
        &mut ext_counts,
    );
    assert_eq!(census.oversize.len(), 1);
    assert_eq!(
        census.oversize[0],
        ("src/large.rs".to_string(), 5000, "rs".to_string())
    );
}

#[test]
fn test_cache_tag_separates_option_sets_but_not_exclude_order() {
    let excluding = |globs: &[&str]| CrawlerOptions {
        custom_excludes: globs.iter().map(|g| (*g).to_string()).collect(),
        ..CrawlerOptions::default()
    };
    let tags = (
        CrawlerOptions::default().cache_tag(),
        excluding(&["dist", "vendor"]).cache_tag() == excluding(&["vendor", "dist"]).cache_tag(),
        excluding(&["dist"]).cache_tag() == excluding(&["dist", "vendor"]).cache_tag(),
    );
    assert_eq!(tags, (String::new(), true, false));
}
