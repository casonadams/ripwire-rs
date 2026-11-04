use ripwire_rs::domain::{
    FileInfo, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol,
};
use ripwire_rs::git::cochange::{
    CochangeOptions, StaticCouplingMap, check_pair_dependency_capability, cluster_cochange_groups,
    mine_cochange_for_file, mine_repo_cochange_pairs,
};
use ripwire_rs::git::history::CommitInfo;
use ripwire_rs::graph::Graph;
use ripwire_rs::query::safety::{SafetyContext, execute_cochange_with_options};

fn create_test_ingest(interner: &Interner) -> IngestResult {
    let mut ingest = IngestResult::new();

    let p0 = interner.intern("src/caller.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: p0,
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 1000,
    });

    let p1 = interner.intern("src/callee.rs");
    ingest.files.push(FileInfo {
        id: 1,
        path: p1,
        lang: Lang::Rust,
        line_count: 40,
        byte_size: 800,
    });

    let p2 = interner.intern("src/unrelated.rs");
    ingest.files.push(FileInfo {
        id: 2,
        path: p2,
        lang: Lang::Rust,
        line_count: 60,
        byte_size: 1200,
    });

    let p3 = interner.intern("docs/arch.md");
    ingest.files.push(FileInfo {
        id: 3,
        path: p3,
        lang: Lang::Markdown,
        line_count: 100,
        byte_size: 2000,
    });
    let s0_name = interner.intern("foo");
    ingest.symbols.push(Symbol::new(
        0,
        s0_name,
        s0_name,
        SymKind::Function,
        Lang::Rust,
        0,
        5,
    ));

    let s1_name = interner.intern("bar");
    ingest.symbols.push(Symbol::new(
        1,
        s1_name,
        s1_name,
        SymKind::Function,
        Lang::Rust,
        1,
        5,
    ));

    ingest
        .references
        .push(Reference::new(0, s1_name, 0, 8, Lang::Rust, RefRole::Call));

    ingest
}

#[test]
fn test_cochange_mining_and_surprise() {
    let commits = vec![
        CommitInfo {
            id: "c1".to_string(),
            timestamp: 100,
            author: "alice@example.com".to_string(),
            changed_files: vec![
                "src/ui.rs".to_string(),
                "src/db.rs".to_string(),
                "docs/readme.md".to_string(),
            ],
        },
        CommitInfo {
            id: "c2".to_string(),
            timestamp: 200,
            author: "alice@example.com".to_string(),
            changed_files: vec![
                "src/ui.rs".to_string(),
                "src/db.rs".to_string(),
                "docs/readme.md".to_string(),
            ],
        },
        CommitInfo {
            id: "c3".to_string(),
            timestamp: 300,
            author: "bob@example.com".to_string(),
            changed_files: vec![
                "src/ui.rs".to_string(),
                "src/db.rs".to_string(),
                "docs/readme.md".to_string(),
            ],
        },
    ];

    let opts = CochangeOptions {
        min_support: 3,
        sub_windows: 3,
        max_commit_files: 100,
    };

    let partners = mine_cochange_for_file("src/ui.rs", &commits, None, None, None, &opts);
    assert_eq!(partners.len(), 2);

    let db_partner = partners
        .iter()
        .find(|p| p.file_path == "src/db.rs")
        .unwrap();
    assert_eq!(db_partner.together, 3);
    assert_eq!(db_partner.deg, 1.0);
    assert_eq!(db_partner.recur, 3);
    assert!(db_partner.dep_capable);
    assert!(db_partner.surprising);

    let doc_partner = partners
        .iter()
        .find(|p| p.file_path == "docs/readme.md")
        .unwrap();
    assert_eq!(doc_partner.together, 3);
    assert!(!doc_partner.dep_capable);
    assert!(!doc_partner.surprising);

    let pairs = mine_repo_cochange_pairs(&commits, None, None, None, &opts);
    assert_eq!(pairs.len(), 3);
    let ui_db_pair = pairs
        .iter()
        .find(|p| p.file_a == "src/db.rs" && p.file_b == "src/ui.rs")
        .unwrap();
    assert_eq!(ui_db_pair.together, 3);
    assert!(ui_db_pair.surprising);
    let groups = cluster_cochange_groups(&pairs);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].partners.len(), 2);
}

#[test]
fn test_git_cochange_surprising_and_recurrence() {
    let interner = Interner::new();
    let ingest = create_test_ingest(&interner);
    let graph = Graph::build(&ingest, &interner);
    let coupling = StaticCouplingMap::build(&ingest, &interner, Some(&graph));

    assert!(coupling.is_coupled("src/caller.rs", "src/callee.rs"));
    assert!(coupling.is_coupled("src/callee.rs", "src/caller.rs"));
    assert!(!coupling.is_coupled("src/caller.rs", "src/unrelated.rs"));

    assert!(check_pair_dependency_capability(
        "src/caller.rs",
        "src/unrelated.rs",
        Some(&ingest),
        Some(&interner)
    ));
    assert!(!check_pair_dependency_capability(
        "src/caller.rs",
        "docs/arch.md",
        Some(&ingest),
        Some(&interner)
    ));

    let commits = vec![
        CommitInfo {
            id: "c0".to_string(),
            timestamp: 600,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "src/unrelated.rs".to_string(),
                "docs/arch.md".to_string(),
            ],
        },
        CommitInfo {
            id: "c1".to_string(),
            timestamp: 500,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "src/unrelated.rs".to_string(),
            ],
        },
        CommitInfo {
            id: "c2".to_string(),
            timestamp: 400,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "src/unrelated.rs".to_string(),
            ],
        },
        CommitInfo {
            id: "c3".to_string(),
            timestamp: 300,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "docs/arch.md".to_string(),
            ],
        },
        CommitInfo {
            id: "c4".to_string(),
            timestamp: 200,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "src/unrelated.rs".to_string(),
                "docs/arch.md".to_string(),
            ],
        },
        CommitInfo {
            id: "c5".to_string(),
            timestamp: 100,
            author: "dev@example.com".to_string(),
            changed_files: vec![
                "src/caller.rs".to_string(),
                "src/callee.rs".to_string(),
                "src/unrelated.rs".to_string(),
            ],
        },
    ];

    let opts = CochangeOptions {
        min_support: 3,
        sub_windows: 3,
        max_commit_files: 100,
    };

    let partners = mine_cochange_for_file(
        "src/caller.rs",
        &commits,
        Some(&coupling),
        Some(&ingest),
        Some(&interner),
        &opts,
    );

    let callee = partners
        .iter()
        .find(|p| p.file_path == "src/callee.rs")
        .expect("callee partner");
    assert_eq!(callee.together, 6);
    assert_eq!(callee.deg, 1.0);
    assert_eq!(callee.recur, 3);
    assert!(callee.dep_capable);
    assert!(!callee.surprising);

    let unrelated = partners
        .iter()
        .find(|p| p.file_path == "src/unrelated.rs")
        .expect("unrelated partner");
    assert_eq!(unrelated.together, 5);
    assert!((unrelated.deg - 0.833).abs() < 0.01);
    assert_eq!(unrelated.recur, 3);
    assert!(unrelated.dep_capable);
    assert!(unrelated.surprising);

    let arch_doc = partners
        .iter()
        .find(|p| p.file_path == "docs/arch.md")
        .expect("arch doc partner");
    assert_eq!(arch_doc.together, 3);
    assert!(!arch_doc.dep_capable);
    assert!(!arch_doc.surprising);

    let pairs = mine_repo_cochange_pairs(
        &commits,
        Some(&coupling),
        Some(&ingest),
        Some(&interner),
        &opts,
    );

    let pair_caller_unrelated = pairs
        .iter()
        .find(|p| {
            (p.file_a == "src/caller.rs" && p.file_b == "src/unrelated.rs")
                || (p.file_a == "src/unrelated.rs" && p.file_b == "src/caller.rs")
        })
        .expect("caller unrelated pair");
    assert_eq!(pair_caller_unrelated.together, 5);
    assert!(pair_caller_unrelated.surprising);

    let pair_caller_callee = pairs
        .iter()
        .find(|p| {
            (p.file_a == "src/caller.rs" && p.file_b == "src/callee.rs")
                || (p.file_a == "src/callee.rs" && p.file_b == "src/caller.rs")
        })
        .expect("caller callee pair");
    assert_eq!(pair_caller_callee.together, 6);
    assert!(!pair_caller_callee.surprising);
}

#[test]
fn test_cochange_query_xml_and_recurrence_filter() {
    let interner = Interner::new();
    let ingest = create_test_ingest(&interner);
    let graph = Graph::build(&ingest, &interner);
    let ctx = SafetyContext {
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };

    let commit = |id: &str, timestamp: i64| CommitInfo {
        id: id.to_string(),
        timestamp,
        author: "dev@example.com".to_string(),
        changed_files: vec!["src/caller.rs".to_string(), "src/unrelated.rs".to_string()],
    };
    let log = vec![commit("c1", 100), commit("c2", 200), commit("c3", 300)];

    let mut buf = Vec::new();
    execute_cochange_with_options(
        Some("src/caller.rs"),
        Default::default(),
        &log,
        &ctx,
        &mut buf,
    )
    .expect("execute cochange");

    let xml = String::from_utf8(buf).unwrap();
    assert!(xml.contains("<f path=\"src/caller.rs\""));
    assert!(xml.contains("<partner path=\"src/unrelated.rs\" together=\"3\""));
    assert!(xml.contains("surprising=\"1\""));
    let mut filtered = Vec::new();
    execute_cochange_with_options(
        Some("src/caller.rs"),
        ripwire_rs::query::safety::CochangeOptions {
            cochange_recur: Some(4),
            ..Default::default()
        },
        &log,
        &ctx,
        &mut filtered,
    )
    .expect("filter cochange partners by recurrence");
    let xml = String::from_utf8(filtered).unwrap();
    assert!(xml.contains("<f path=\"src/caller.rs\" partners=\"0\">"));
    assert!(!xml.contains("<partner "));
}
