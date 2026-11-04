use ripwire_rs::git::cochange::CoPartner;
use ripwire_rs::git::history::CommitInfo;
use ripwire_rs::query::safety::*;
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ToXmlString, ingest_in_memory_files};

#[test]
fn test_quality_baseline_from_facts_clean() {
    crate::a_safety_context::with_safety_context(
        "pure-facts",
        &[("src/lib.rs", "pub fn clean_func() {}\n")],
        |context| {
            let snap = baseline_from_facts(context, "sha_clean_123", None, false).unwrap();
            assert_eq!(snap.head_sha, "sha_clean_123");
            assert!(!snap.dirty);
            assert_eq!(snap.absorbed, 0);
        },
    );
}

#[test]
fn test_quality_baseline_from_facts_dirty_policy() {
    crate::a_safety_context::with_safety_context(
        "pure-facts",
        &[("src/lib.rs", "pub fn sample() {}\n")],
        |context| {
            let err = baseline_from_facts(
                context,
                "sha_dirty_456",
                Some((3, "complexity sample src/lib.rs:1 was=2 now=8")),
                false,
            )
            .unwrap_err();
            assert!(err.to_string().contains("3 gating finding(s)"));
            assert!(err.to_string().contains("--allow-dirty"));

            let snap = baseline_from_facts(
                context,
                "sha_dirty_456",
                Some((3, "complexity sample src/lib.rs:1 was=2 now=8")),
                true,
            )
            .unwrap();
            assert_eq!(snap.head_sha, "sha_dirty_456");
            assert!(snap.dirty);
            assert_eq!(snap.absorbed, 3);

            let error =
                baseline_from_facts(context, "abc", Some((2, "cc target")), false).unwrap_err();
            assert!(error.to_string().contains("2 gating finding(s)"));
            assert!(error.to_string().contains("cc target"));
            let dirty = baseline_from_facts(context, "abc", Some((2, "cc target")), true).unwrap();
            assert!(dirty.dirty);
            assert_eq!(dirty.absorbed, 2);
            let clean = baseline_from_facts(context, "abc", None, false).unwrap();
            assert!(!clean.dirty);
            assert_eq!(clean.absorbed, 0);
        },
    );
}

#[test]
fn test_cochange_printers_and_formatting() {
    let mut out = Vec::new();
    let partners = [
        CoPartner {
            file_path: "src/b.rs".to_string(),
            together: 5,
            deg: 0.85,
            deg_rev: 0.80,
            recur: 2,
            surprising: true,
            dep_capable: true,
        },
        CoPartner {
            file_path: "src/c.rs".to_string(),
            together: 2,
            deg: 0.40,
            deg_rev: 0.30,
            recur: 1,
            surprising: false,
            dep_capable: false,
        },
    ];
    print_cochange_partners("src/a.rs", &partners, &mut out).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.contains("<f path=\"src/a.rs\" partners=\"2\">"));
    assert!(text.contains("<partner path=\"src/b.rs\" together=\"5\" deg=\"0.85\" deg_rev=\"0.80\" recur=\"2\" surprising=\"1\" />"));
    assert!(text.contains("<partner path=\"src/c.rs\" together=\"2\" deg=\"0.40\" deg_rev=\"0.30\" recur=\"1\" dep_capable=\"0\" />"));

    with_cochange_history(|context, commits| {
        let mut pairs_xml = Vec::new();
        execute_cochange_with_options(
            None,
            CochangeOptions::default(),
            commits,
            context,
            &mut pairs_xml,
        )
        .unwrap();
        let text = std::str::from_utf8(&pairs_xml).unwrap();
        assert!(text.contains("<cochange schema=\"ripwire-rs.cochange/v1\" pairs=\"2\""));
        assert!(text.contains("<pair a=\"src/one.rs\""));

        let mut groups_xml = Vec::new();
        let options = CochangeOptions {
            cochange_groups: true,
            ..Default::default()
        };
        execute_cochange_with_options(None, options, commits, context, &mut groups_xml).unwrap();
        let text = std::str::from_utf8(&groups_xml).unwrap();
        assert!(text.contains("<cochange-groups schema=\"ripwire-rs.cochange-groups/v1\""));
        assert!(text.contains("<group core="));
    });
}

#[test]
fn test_execute_cochange_and_hotspots_with_injected_history() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let files = [
        (
            "src/a.rs",
            Lang::Rust,
            b"pub fn a(x: bool) { if x {} }\n".as_slice(),
        ),
        (
            "src/b.rs",
            Lang::Rust,
            b"pub fn b(y: bool) { if y {} }\n".as_slice(),
        ),
    ];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let s_ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let commits: Vec<_> = (0..4)
        .map(|index| CommitInfo {
            id: format!("commit{index}"),
            timestamp: 1_700_000_000 - index * 3_000_000,
            author: "Ada".to_string(),
            changed_files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
        })
        .collect();

    let mut out = Vec::new();
    execute_hotspots_with(
        Some("2023-01-01"),
        &commits,
        &s_ctx,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    let rows = |bytes: &[u8]| {
        roxmltree::Document::parse(std::str::from_utf8(bytes).unwrap())
            .unwrap()
            .descendants()
            .filter(|node| node.has_tag_name("f"))
            .count()
    };
    assert_eq!(rows(&out), 2);
    let mut paged = Vec::new();
    let page = PageRequest {
        limit: Some(1),
        offset: None,
        budget: None,
    };
    execute_hotspots_with(Some("2023-01-01"), &commits, &s_ctx, page, &mut paged).unwrap();
    assert_eq!(rows(&paged), 1);
    let root = roxmltree::Document::parse(std::str::from_utf8(&paged).unwrap()).unwrap();
    let attrs = root.root_element();
    assert_eq!(attrs.attribute("shown"), Some("1"));
    assert_eq!(attrs.attribute("limit"), Some("1"));
    assert_eq!(attrs.attribute("capped"), Some("1"));
    let text = std::str::from_utf8(&out).unwrap();
    let hotspots = roxmltree::Document::parse(text).unwrap();
    for path in ["src/a.rs", "src/b.rs"] {
        let row = hotspots
            .descendants()
            .find(|node| node.attribute("p") == Some(path))
            .unwrap();
        assert_eq!(row.attribute("churn"), Some("4"));
    }

    out.clear();
    let options = CochangeOptions {
        since: Some("2023-01-01"),
        ..Default::default()
    };
    execute_cochange_with_options(Some("src/a.rs"), options, &commits, &s_ctx, &mut out).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    let partners = roxmltree::Document::parse(text).unwrap();
    let paths: Vec<_> = partners
        .descendants()
        .filter(|node| node.has_tag_name("partner"))
        .map(|node| node.attribute("path").unwrap())
        .collect();
    assert_eq!(paths, ["src/b.rs"]);
}

#[test]
fn test_execute_edit_check_and_affected_consumer_facts() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let files = [(
        "src/lib.rs",
        Lang::Rust,
        b"pub fn target_sym() {}\nfn caller() { target_sym(); }\n".as_slice(),
    )];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let s_ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let mut out = Vec::new();
    let req = EditCheckRequest {
        sym_name: "target_sym",
        head_contract: None,
        page: None,
        at_stamp: Some("head_commit_123"),
    };
    write_edit_check_from_facts(&req, &s_ctx, &mut out).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.contains("<edit-check schema=\"ripwire-rs.edit-check/v1\""));
    assert!(text.contains("sym=\"target_sym\""));
    assert!(text.contains("callers=\"1\""));
    assert!(text.contains("at=\"head_commit_123\""));

    out.clear();
    let sources: [(&str, &[u8]); 1] = [(
        "src/lib.rs",
        b"pub fn target_sym() {}\nfn caller() { target_sym(); }\n",
    )];
    let report = affected_from_sources("src/lib.rs", &s_ctx, &sources);
    report.write_xml(&mut out).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.contains("<affected schema=\"ripwire-rs.affected/v1\""));
    assert!(report.tests.is_empty());
}

#[test]
fn test_situational_report_consumer_facts() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let files = [(
        "src/lib.rs",
        Lang::Rust,
        b"pub fn core_item() {}\n".as_slice(),
    )];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let s_ctx = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let changed_files = vec!["src/lib.rs".to_string()];
    let report = SituationalReport {
        root: "root",
        files: &changed_files,
        commits: None,
        context: s_ctx,
        page: PageRequest::default(),
    };

    let mut out = Vec::new();
    report.write_xml(&mut out).unwrap();
    let xml = std::str::from_utf8(&out).unwrap();
    assert!(xml.contains("<situ schema=\"ripwire-rs.situ/v1\""));
    assert!(xml.contains("<summary files=\"1\""));
    assert!(xml.contains("<changed path=\"src/lib.rs\"/>"));
}

fn with_cochange_history(check: impl FnOnce(&SafetyContext<'_>, &[CommitInfo])) {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let files = [
        ("src/one.rs", Lang::Rust, b"pub fn one() {}\n".as_slice()),
        ("src/two.rs", Lang::Rust, b"pub fn two() {}\n".as_slice()),
        (
            "src/three.rs",
            Lang::Rust,
            b"pub fn three() {}\n".as_slice(),
        ),
    ];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let context = SafetyContext {
        root: "root",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    let commit = |index: i64, changed: [&str; 2]| CommitInfo {
        id: format!("commit{index}"),
        timestamp: 1_700_000_000 - index * 3_000_000,
        author: "Ada".to_string(),
        changed_files: changed.iter().map(|f| f.to_string()).collect(),
    };
    let mut commits: Vec<CommitInfo> = (0..4)
        .map(|index| commit(index, ["src/one.rs", "src/two.rs"]))
        .collect();
    commits.extend((4..8).map(|index| commit(index, ["src/one.rs", "src/three.rs"])));
    check(&context, &commits);
}

#[test]
fn cochange_pairs_budget_fits_rows() {
    with_cochange_history(|context, commits| {
        let render = |page: PageRequest| {
            let mut out = Vec::new();
            let options = CochangeOptions {
                page,
                ..Default::default()
            };
            execute_cochange_with_options(None, options, commits, context, &mut out).unwrap();
            String::from_utf8(out).unwrap()
        };
        let roomy = render(PageRequest {
            limit: None,
            offset: None,
            budget: Some(usize::MAX),
        });
        assert!(roomy.contains("shown=\"2\""));
        let tight = render(PageRequest {
            limit: None,
            offset: None,
            budget: Some(1),
        });
        assert!(tight.contains("shown=\"1\""));
        assert!(tight.contains("has_more=\"1\""));
    });
}
