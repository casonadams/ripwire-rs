use ripwire_rs::serialize::serialize_grep_xml;
use serde_json::{Value, json};

fn render(val: &Value) -> String {
    let mut buf = Vec::new();
    serialize_grep_xml(val, &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

/// A grep report for pattern `p` with `fields` overriding the defaults.
fn grep(fields: Value) -> String {
    let mut report = json!({
        "pattern": "p", "verb": "grep", "literal": true,
        "enc": {"f": {"callers": 0, "floor": false, "cx": 1}},
    });
    report
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    render(&report)
}

fn hit(path: &str, line: u64, enclosing: &str) -> Value {
    json!({"path": path, "line": line, "text": "x", "enclosing": enclosing, "enc_id": 1})
}

/// `count` hits in `a.py` on lines 1..=count, all in enclosing `f`.
fn hits(count: u64) -> Value {
    Value::Array((1..=count).map(|l| hit("a.py", l, "f")).collect())
}

#[test]
fn test_enc_rows_disclose_read_sites_only_when_indexed() {
    let cases: [(Value, &[&str], &[&str]); 2] = [
        (
            json!({
                "pattern": "LIMIT",
                "hits": [hit("a.py", 1, "LIMIT")],
                "enc": {"LIMIT": {"callers": 0, "floor": true, "floor_next": "--uses=LIMIT", "cx": 0,
                                  "reads": 2, "reads_at": ["a.py:5", "a.py:8"]}},
            }),
            &[
                "<enc n=\"LIMIT\" callers=\"0\" callers_floor=\"1\" floor_next=\"--uses=LIMIT\" reads=\"2\" reads_at=\"a.py:5 a.py:8\"/>",
                "next=\"--uses=LIMIT\"",
            ],
            &[],
        ),
        (
            json!({
                "pattern": "f",
                "hits": [hit("a.py", 1, "f")],
                "enc": {"f": {"callers": 1, "floor": false, "cx": 2, "reads": 0, "reads_at": []}},
            }),
            &[
                "<enc n=\"f\" callers=\"1\" cx=\"2\"/>",
                "next=\"--at=a.py:1\"",
            ],
            &["reads"],
        ),
    ];
    for (fields, present, absent) in cases {
        let xml = grep(fields);
        for needle in present {
            assert!(xml.contains(needle), "missing {needle}: {xml}");
        }
        for needle in absent {
            assert!(!xml.contains(needle), "unexpected {needle}: {xml}");
        }
    }
}

#[test]
fn test_windows_disclose_the_shown_hits_under_limit_and_budget() {
    let cases: [(Value, &[&str], &[&str]); 4] = [
        (
            json!({"limit": 2, "offset": 1, "hits": hits(5)}),
            &[
                "hits=\"5\" shown=\"2\" capped=\"1\" total=\"5\" has_more=\"1\" next_offset=\"3\" offset=\"1\" limit=\"2\"",
                "next=\"--grep=p --offset=3 --legend=compact\"",
                "<hit l=\"2\"",
                "<hit l=\"3\"",
            ],
            &["complete=\"1\"", "<hit l=\"4\""],
        ),
        (
            json!({"budget": 1, "hits": hits(50)}),
            &[
                "hits=\"50\" shown=\"1\" capped=\"1\" total=\"50\" has_more=\"1\"",
                "hits_capped=\"1\"",
                "<hit l=\"1\"",
                "next=\"--grep=p --offset=1 --legend=compact\"",
            ],
            &["<hit l=\"2\""],
        ),
        (
            json!({"budget": 1000000, "hits": hits(5)}),
            &[
                "hits=\"5\" shown=\"5\" capped=\"0\" hits_capped=\"0\"",
                "complete=\"1\"",
            ],
            &[],
        ),
        (
            json!({"limit": 3, "budget": 1, "hits": hits(5)}),
            &[
                "hits=\"5\" shown=\"3\" capped=\"1\" total=\"5\" has_more=\"1\"",
                "<hit l=\"3\"",
            ],
            &["<hit l=\"4\""],
        ),
    ];
    for (fields, present, absent) in cases {
        let xml = grep(fields);
        for needle in present {
            assert!(xml.contains(needle), "missing {needle}: {xml}");
        }
        for needle in absent {
            assert!(!xml.contains(needle), "unexpected {needle}: {xml}");
        }
    }
}

#[test]
fn test_large_budget_counts_each_site_once() {
    let xml = grep(json!({"budget": 1000000, "hits": hits(5)}));
    assert_eq!(xml.matches("<hit ").count(), 1);
    assert_eq!(xml.matches("<at ").count(), 4);
}

#[test]
fn test_identical_sites_merge_into_one_hit_with_at_markers() {
    let xml = grep(json!({
        "hits": [hit("a.py", 5, "f"), hit("a.py", 5, "f"), hit("a.py", 5, "f")],
    }));
    assert!(xml.contains("<hit l=\"5\" in=\"f\" n=\"3\">"));
    assert_eq!(xml.matches("<at l=\"5\" in=\"f\"/>").count(), 2);
    assert!(xml.contains("hits=\"3\""));
}

#[test]
fn test_identical_text_runs_merge_across_lines_keeping_each_enclosing() {
    let same = |line: u64, enclosing: &str| json!({"path": "a.py", "line": line, "text": "row_bytes(x)", "enclosing": enclosing, "enc_id": 1});
    let xml = grep(json!({
        "hits": [same(5, "f"), same(9, "g"), hit("a.py", 12, "h"), same(20, "f")],
        "enc": {"f": {"callers": 0, "floor": false, "cx": 1}, "g": {"callers": 0, "floor": false, "cx": 1}},
    }));
    assert!(xml.contains("<hit l=\"5\" in=\"f\" n=\"2\">"), "{xml}");
    assert!(xml.contains("<at l=\"9\" in=\"g\"/>"), "{xml}");
    assert!(xml.contains("<hit l=\"12\" in=\"h\">"), "{xml}");
    assert!(xml.contains("<hit l=\"20\" in=\"f\">"), "{xml}");
}

#[test]
fn test_unindexed_hits_are_counted_apart_and_listed_in_trailing_element() {
    let xml = grep(json!({
        "hits": [hit("a.py", 1, "f")],
        "unindexed": [{"path": "n.txt", "line": 1, "text": "p"}],
        "unindexed_files_scanned": 3, "corpus_pruned_dirs": 2,
    }));
    assert!(xml.contains("hits=\"1\""));
    assert!(
        xml.contains("corpus_pruned_dirs=\"2\" unindexed_hits=\"1\" unindexed_files_scanned=\"3\"")
    );
    assert!(xml.contains("<unindexed count=\"1\" shown=\"1\" capped=\"0\">"));
    assert!(xml.contains("<f p=\"n.txt\">"));
    assert!(xml.contains("complete=\"1\""));
}

#[test]
fn test_suppressed_tiers_withdraw_the_complete_claim() {
    let xml = grep(json!({"suppressed_comment": 2, "hits": [hit("a.py", 1, "f")]}));
    assert!(xml.contains("suppressed_comment=\"2\""));
    assert!(!xml.contains("complete=\"1\""));
}
