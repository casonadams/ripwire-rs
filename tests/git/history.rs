use ripwire_rs::git::history::{
    CommitInfo, SinceScope, WalkNode, civil_from_days, compute_file_churn, days_from_civil,
    filter_commits_by_scope, months_before_epoch, parse_since_scope, walk_metas,
    walk_repository_history,
};
use ripwire_rs::serialize::git_at_attr;
use std::collections::HashMap;
use std::convert::Infallible;

#[test]
fn test_civil_calendar_conversions() {
    let (y, m, d) = civil_from_days(0);
    assert_eq!((y, m, d), (1970, 1, 1));
    assert_eq!(days_from_civil(1970, 1, 1), 0);

    let d2026 = days_from_civil(2026, 10, 11);
    let (y2, m2, d2) = civil_from_days(d2026);
    assert_eq!((y2, m2, d2), (2026, 10, 11));

    let epoch = 1775000000;
    let past = months_before_epoch(epoch, 12);
    assert!(past < epoch);
    assert_eq!(epoch - past, 365 * 86400);
}

#[test]
fn test_since_scope_parsing() {
    let anchor = 1775000000;
    assert_eq!(parse_since_scope(None, anchor), SinceScope::AllHistory);
    assert_eq!(parse_since_scope(Some(""), anchor), SinceScope::AllHistory);
    assert_eq!(
        parse_since_scope(Some("today"), anchor),
        SinceScope::SinceDate(anchor)
    );
    assert_eq!(
        parse_since_scope(Some("yesterday"), anchor),
        SinceScope::SinceDate(anchor - 86400)
    );
    assert_eq!(
        parse_since_scope(Some("2 weeks ago"), anchor),
        SinceScope::SinceDate(anchor - 14 * 86400)
    );
    assert_eq!(
        parse_since_scope(Some("10 days ago"), anchor),
        SinceScope::SinceDate(anchor - 10 * 86400)
    );

    let sha = "e0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9";
    assert_eq!(
        parse_since_scope(Some(sha), anchor),
        SinceScope::SinceRevision(sha.to_string())
    );

    let iso = "2026-01-01";
    match parse_since_scope(Some(iso), anchor) {
        SinceScope::SinceDate(d) => assert!(d > 0),
        _ => panic!("expected SinceDate"),
    }
}

#[test]
fn test_commit_filtering_and_churn() {
    let epoch = 1775000000;
    let commits = vec![
        CommitInfo {
            id: "c3".to_string(),
            timestamp: epoch,
            author: "alice@example.com".to_string(),
            changed_files: vec!["a.rs".to_string(), "b.rs".to_string()],
        },
        CommitInfo {
            id: "c2".to_string(),
            timestamp: epoch - 90 * 86400,
            author: "alice@example.com".to_string(),
            changed_files: vec!["a.rs".to_string()],
        },
        CommitInfo {
            id: "c1".to_string(),
            timestamp: epoch - 180 * 86400,
            author: "bob@example.com".to_string(),
            changed_files: vec!["b.rs".to_string()],
        },
        CommitInfo {
            id: "bomb".to_string(),
            timestamp: epoch - 100,
            author: "dev@example.com".to_string(),
            changed_files: (0..150).map(|i| format!("file_{i}.rs")).collect(),
        },
    ];
    let filtered = filter_commits_by_scope(&commits, &SinceScope::SinceDate(epoch - 90 * 86400));
    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[0].id, "c3");
    assert_eq!(filtered[1].id, "c2");

    let rev_filtered =
        filter_commits_by_scope(&commits, &SinceScope::SinceRevision("c2".to_string()));
    assert_eq!(rev_filtered.len(), 1);
    assert_eq!(rev_filtered[0].id, "c3");

    let churn = compute_file_churn(commits.iter(), epoch, 100);
    assert_eq!(churn.len(), 2);
    assert_eq!(churn["a.rs"].commits, 2);
    assert_eq!(churn["b.rs"].commits, 2);
    assert_eq!(churn["a.rs"].last_commit_epoch, epoch);
    assert_eq!(churn["b.rs"].last_commit_epoch, epoch);
    assert!((churn["a.rs"].decayed_weight - 1.5).abs() < 0.05);
    assert!(!churn.contains_key("file_0.rs"));
}

#[test]
fn test_walk_repository_history_nonexistent() {
    let res = walk_repository_history("/nonexistent_ripwire_path_test");
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(format!("{err}").contains("failed to open repository"));
}

type Graph = HashMap<gix::ObjectId, WalkNode>;

fn oid(n: u32) -> gix::ObjectId {
    gix::ObjectId::from_hex(format!("{n:040x}").as_bytes()).unwrap()
}

/// Add commit `n` at `timestamp`; its followed parents are `parents`, and it is a merge with two.
fn add(graph: &mut Graph, n: u32, timestamp: i64, parents: &[u32]) {
    graph.insert(
        oid(n),
        WalkNode {
            timestamp,
            is_merge: parents.len() > 1,
            followed_parents: parents.iter().map(|&p| oid(p)).collect(),
        },
    );
}

fn walk(graph: &Graph, head: u32, cutoff: Option<i64>) -> Vec<gix::ObjectId> {
    walk_metas(
        oid(head),
        |id| Ok::<_, Infallible>(graph.get(&id).cloned()),
        cutoff,
    )
    .unwrap()
    .into_iter()
    .map(|m| m.id)
    .collect()
}

#[test]
fn test_walk_orders_commits_newest_first_through_parents() {
    let mut chain = Graph::new();
    add(&mut chain, 4, 400, &[3]);
    add(&mut chain, 3, 300, &[2]);
    add(&mut chain, 2, 200, &[1]);
    add(&mut chain, 1, 100, &[]);
    assert_eq!(walk(&chain, 4, None), vec![oid(4), oid(3), oid(2), oid(1)]);

    for parent_time in [100, 200] {
        let mut merge = Graph::new();
        add(&mut merge, 3, 300, &[1, 2]);
        add(&mut merge, 2, 200, &[]);
        add(&mut merge, 1, parent_time, &[]);
        assert_eq!(
            walk(&merge, 3, None),
            vec![oid(2), oid(1)],
            "parent at {parent_time}"
        );
    }

    let mut diamond = Graph::new();
    add(&mut diamond, 5, 500, &[3, 4]);
    add(&mut diamond, 4, 200, &[1]);
    add(&mut diamond, 3, 300, &[1]);
    add(&mut diamond, 1, 100, &[]);
    assert_eq!(walk(&diamond, 5, None), vec![oid(3), oid(4), oid(1)]);
}

#[test]
fn test_cutoff_is_inclusive_at_exact_commit_time() {
    let mut g = Graph::new();
    add(&mut g, 3, 300, &[2]);
    add(&mut g, 2, 200, &[1]);
    add(&mut g, 1, 100, &[]);
    assert_eq!(walk(&g, 3, Some(200)), vec![oid(3), oid(2)]);
    assert_eq!(walk(&g, 3, Some(201)), vec![oid(3)]);
    assert_eq!(walk(&g, 3, Some(100)), vec![oid(3), oid(2), oid(1)]);
}

#[test]
fn test_cutoff_newer_than_head_yields_nothing() {
    let mut g = Graph::new();
    add(&mut g, 3, 300, &[2]);
    add(&mut g, 2, 200, &[]);
    assert!(walk(&g, 3, Some(301)).is_empty());
}

#[test]
fn test_root_commit_is_reported() {
    let mut g = Graph::new();
    add(&mut g, 1, 100, &[]);
    assert_eq!(walk(&g, 1, None), vec![oid(1)]);
}

#[test]
fn test_only_followed_parents_are_walked() {
    let mut g = Graph::new();
    g.insert(
        oid(3),
        WalkNode {
            timestamp: 300,
            is_merge: true,
            followed_parents: vec![oid(1)],
        },
    );
    add(&mut g, 2, 200, &[]);
    add(&mut g, 1, 100, &[]);
    assert_eq!(walk(&g, 3, None), vec![oid(1)]);
}

#[test]
fn test_absent_parent_is_skipped_and_absent_head_is_empty() {
    let mut g = Graph::new();
    add(&mut g, 2, 200, &[9]);
    assert_eq!(walk(&g, 2, None), vec![oid(2)]);
    assert!(walk(&g, 7, None).is_empty());
}

#[test]
fn test_lookup_error_aborts_walk() {
    let mut g = Graph::new();
    add(&mut g, 2, 200, &[1]);
    add(&mut g, 1, 100, &[]);
    let result = walk_metas(
        oid(2),
        |id| {
            if id == oid(1) {
                Err("boom")
            } else {
                Ok(g.get(&id).cloned())
            }
        },
        None,
    );
    assert_eq!(result.unwrap_err(), "boom");
}

#[test]
fn test_git_at_attr_labels_stamp_absence_and_unavailable_status() {
    assert_eq!(
        git_at_attr(Ok(Some("abc123def+dirty".to_string()))),
        " at=\"abc123def+dirty\""
    );
    assert_eq!(git_at_attr(Ok(None)), "");
    assert_eq!(git_at_attr(Ok(Some(String::new()))), "");
    assert_eq!(
        git_at_attr(Err("index unreadable".to_string())),
        " at_unavailable=\"index unreadable\""
    );
}
