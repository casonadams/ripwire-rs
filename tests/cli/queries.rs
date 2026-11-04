use ripwire_rs::commands::common::edit::{resolve_edit_spec, resolve_edit_spec_with_reader};
use ripwire_rs::commands::common::ladder::execute_expand;
use ripwire_rs::commands::doc_drift::{DOC_DRIFT, DocDriftArgs};
use ripwire_rs::commands::flags_report::{FLAGS, FlagsArgs};
use ripwire_rs::commands::flip::{
    FLIP, FlipArgs, FlipReach, flag_entries, flip_reach, render_flip_xml,
};
use ripwire_rs::commands::handoff::build_handoff_verified_rows;
use ripwire_rs::commands::mentions::{MENTIONS, MentionsArgs};
use ripwire_rs::commands::notes::{NOTES, NotesArgs, render_notes_xml};
use ripwire_rs::commands::whereis::{WHEREIS, WhereisArgs};
use ripwire_rs::query::detail::skeletonize_body;
use ripwire_rs::query::doc_tools::{
    WhereisFacts, flags_from_sources, whereis_from_sources_with_facts,
};
use ripwire_rs::serialize::{PageRequest, serialize_whereis_xml};
use ripwire_rs::{IngestResult, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

fn render_whereis_xml<W: std::io::Write>(
    val: &serde_json::Value,
    listing: Option<&str>,
    full_text: bool,
    out: &mut W,
) -> std::io::Result<()> {
    let mut val = val.clone();
    if let Some(listing) = listing {
        val["listing"] = listing.into();
    }
    if full_text {
        val["full_text"] = true.into();
    }
    serialize_whereis_xml(&val, out)
}

#[test]
fn test_skeletonize_body() {
    let body = "fn run() {\n    if x {\n        let a = 1;\n        let b = 2;\n    }\n}";
    let skel = skeletonize_body(body);
    assert!(skel.contains("..."));
    assert!(!skel.contains("let a = 1"));
    let interner = Interner::new();
    let ingest = IngestResult::new();
    let mut buf = Vec::new();
    assert!(execute_expand(".", "missing", false, false, &ingest, &interner, &mut buf).is_err());
    assert!(execute_expand(".", "missing", true, false, &ingest, &interner, &mut buf).is_err());
}

#[test]
fn test_handoff_xml_branches() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let fixtures: &[(&str, Lang, &[u8])] = &[("src/lib.rs", Lang::Rust, b"pub fn overview() {}\n")];
    let ingest = ingest_in_memory_files(fixtures, &reg, &interner);
    let empty_rows = build_handoff_verified_rows(
        &[],
        &ingest,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(empty_rows.is_empty());
    let ch_rows = build_handoff_verified_rows(
        &["src/lib.rs".to_string()],
        &ingest,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert!(ch_rows.contains("<s n=\"overview\"/>"));
    let unindexed = build_handoff_verified_rows(
        &["docs/notes.txt".to_string()],
        &ingest,
        &interner,
        ripwire_rs::serialize::PageRequest::default(),
    );
    assert_eq!(unindexed, "<f p=\"docs/notes.txt\"/>");
    assert!(!unindexed.contains("main"));
}

#[test]
fn test_handoff_blast_and_tests_follow_the_changed_symbols() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            ("src/lib.rs", Lang::Rust, b"pub fn alpha() -> u32 { 1 }\n"),
            (
                "src/caller.rs",
                Lang::Rust,
                b"pub fn use_alpha() -> u32 { alpha() }\n",
            ),
            (
                "tests/lib_test.rs",
                Lang::Rust,
                b"fn test_alpha() { alpha(); }\n",
            ),
            ("src/unrelated.rs", Lang::Rust, b"pub fn idle() {}\n"),
        ],
        &reg,
        &interner,
    );
    let graph = ripwire_rs::Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let changed = ["src/lib.rs".to_string()];
    assert_eq!(
        ripwire_rs::commands::handoff::handoff_blast_files(
            &changed, &ingest, &graph, &pr.ranks, &interner
        ),
        2
    );
    assert_eq!(
        ripwire_rs::commands::handoff::handoff_affected_tests(&changed, &ingest, &graph, &interner),
        1
    );

    let idle = ["src/unrelated.rs".to_string()];
    assert_eq!(
        ripwire_rs::commands::handoff::handoff_blast_files(
            &idle, &ingest, &graph, &pr.ranks, &interner
        ),
        0
    );
    assert_eq!(
        ripwire_rs::commands::handoff::handoff_affected_tests(&idle, &ingest, &graph, &interner),
        0
    );
    assert_eq!(
        ripwire_rs::commands::handoff::handoff_blast_files(
            &[],
            &ingest,
            &graph,
            &pr.ranks,
            &interner
        ),
        0
    );
}

#[test]
fn test_resolve_edit_spec_all_branches() {
    // Inline spec with colon
    let (sym, payload) = resolve_edit_spec("compute:let y = 2;", None).unwrap();
    assert_eq!(sym, "compute");
    assert_eq!(payload, "let y = 2;");

    // Read from stdin when payload_override is "-"
    let stdin_cur = std::io::Cursor::new(b"stdin payload content");
    let (sym2, payload2) = resolve_edit_spec_with_reader("compute", Some("-"), stdin_cur).unwrap();
    assert_eq!(sym2, "compute");
    assert_eq!(payload2, "stdin payload content");

    // Literal string fallback when payload_override is not a readable file
    let stdin_cur2 = std::io::Cursor::new(b"");
    let (sym3, payload3) =
        resolve_edit_spec_with_reader("compute", Some("inline_override_text"), stdin_cur2).unwrap();
    assert_eq!(sym3, "compute");
    assert_eq!(payload3, "inline_override_text");

    // None payload
    let stdin_cur3 = std::io::Cursor::new(b"");
    let (sym4, payload4) = resolve_edit_spec_with_reader("compute", None, stdin_cur3).unwrap();
    assert_eq!(sym4, "compute");
    assert_eq!(payload4, "");
}

#[test]
fn test_render_notes_xml() {
    let mut buf = Vec::new();
    render_notes_xml("note alpha\n\nnote beta\n", &mut buf).unwrap();
    let out = std::str::from_utf8(&buf).unwrap();
    assert!(out.contains("ripwire-rs.notes/v1"));
    assert!(out.contains("<note text=\"note alpha\"/>"));
    assert!(out.contains("<note text=\"note beta\"/>"));
}

#[test]
fn test_render_flip_xml() {
    let flags_val = serde_json::json!({
        "flags": [
            { "name": "gate_deploy", "kind": "compile", "path": "src/deploy.rs", "line": 42, "reads_count": 1, "reads": [{ "path": "src/deploy.rs", "line": 42 }] },
            { "name": "other_flag", "kind": "compile", "path": "src/other.rs", "line": 10, "reads_count": 1, "reads": [{ "path": "src/other.rs", "line": 10 }] }
        ]
    });
    let entries = flag_entries("gate_deploy", &flags_val).unwrap();
    let reach = FlipReach::default();
    let mut buf = Vec::new();
    render_flip_xml(
        "gate_deploy",
        ".",
        &entries,
        &reach,
        PageRequest::default(),
        &mut buf,
    )
    .unwrap();
    let out = std::str::from_utf8(&buf).unwrap();
    assert!(out.contains("ripwire-rs.flip/v2"));
    assert!(out.contains(
        "<site p=\"src/deploy.rs\" l=\"42\" gate=\"gate_deploy\" kind=\"compile\" reads=\"1\"/>"
    ));
    assert!(out.contains("<reach readers=\"0\" unscoped_reads=\"0\" files=\"0\">"));
    assert!(!out.contains("other_flag"));
}

#[test]
fn test_flag_entries_rejects_empty_and_unknown_gates() {
    let flags_val = serde_json::json!({
        "flags": [ { "name": "FEATURE_X" }, { "name": "FEATURE_XY" } ]
    });
    assert!(flag_entries("", &flags_val).is_err());
    assert!(flag_entries("   ", &flags_val).is_err());
    assert!(flag_entries("ZZZ_NOPE", &flags_val).is_err());
    // Substring matches are not gates: "FEATURE" names no flag in the table.
    assert!(flag_entries("FEATURE", &flags_val).is_err());
    let exact = flag_entries("FEATURE_X", &flags_val).unwrap();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0]["name"], "FEATURE_X");
}

#[test]
fn test_flip_reach_follows_readers_to_callers_and_tests() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let app: &[u8] = b"import os\ndef gate_check():\n    return os.getenv(\"FEATURE_X\")\ndef serve():\n    return gate_check()\ndef unrelated():\n    pass\n";
    let test: &[u8] = b"from app import serve\ndef test_serve():\n    serve()\n";
    let ingest = ingest_in_memory_files(
        &[
            ("src/app.py", Lang::Python, app),
            ("tests/test_app.py", Lang::Python, test),
        ],
        &reg,
        &interner,
    );
    let graph = ripwire_rs::Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&ripwire_rs::PageRankConfig::default());
    let sources: &[(&str, &[u8])] = &[("src/app.py", app), ("tests/test_app.py", test)];
    let flags = flags_from_sources(":memory:", sources, &ingest, &interner);
    let entries = flag_entries("FEATURE_X", &flags).unwrap();
    let reach = flip_reach(&entries, &ingest, &graph, &pr.ranks, &interner);
    let names: Vec<&str> = reach.readers.iter().map(|(_, _, n)| n.as_str()).collect();
    assert_eq!(names, vec!["gate_check"]);
    assert_eq!(reach.unscoped_reads, 0);
    let callers: Vec<&str> = reach.callers.iter().map(|c| c.name.as_str()).collect();
    assert!(callers.contains(&"serve"), "callers were {callers:?}");
    assert!(reach.tests.iter().any(|t| t.path == "tests/test_app.py"));

    let unread = serde_json::json!({ "flags": [] });
    assert!(flag_entries("FEATURE_Y", &unread).is_err());
    let none = FlipReach::default();
    assert!(none.callers.is_empty() && none.readers.is_empty());
}

#[test]
fn test_execute_doc_search_query_all_branches() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let file_bytes = b"pub fn overview() {}\n";
    let fixtures: &[(&str, Lang, &[u8])] = &[("src/lib.rs", Lang::Rust, file_bytes)];
    let ingest = ingest_in_memory_files(fixtures, &reg, &interner);
    let sources: &[(&str, &[u8])] = &[("src/lib.rs", file_bytes)];

    // Whereis pure renderer with acquired-facts fixtures (zero I/O)
    let facts = WhereisFacts {
        history: Some(serde_json::json!({
            "probed": 1,
            "head": "abc1234",
            "fate": {
                "v": "removed",
                "commit": "def5678",
                "date": "2026-10-20",
                "subject": "remove overview symbol"
            }
        })),
        head_date: Some("2026-10-21".to_string()),
        at_stamp: Some("abc1234".to_string()),
        branch_rows: vec![serde_json::json!({
            "ref": "feature-branch",
            "tip": "feat123",
            "date": "2026-10-20",
            "symbol": "overview",
            "kind": "def",
            "file": "src/lib.rs",
            "path": "src/lib.rs",
            "line": 1,
            "text": "pub fn overview() {}",
            "on_head": false,
        })],
        refs_scanned: 1,
    };
    let whereis_val =
        whereis_from_sources_with_facts(&sources, &ingest, &interner, "overview", &facts);
    let mut buf = Vec::new();
    render_whereis_xml(&whereis_val, None, false, &mut buf).unwrap();
    let whereis_xml = std::str::from_utf8(&buf).unwrap();
    assert!(whereis_xml.contains("ripwire-rs.whereis/v1"));
    assert!(whereis_xml.contains("feature-branch"));
    assert!(whereis_xml.contains("fate sym=\"overview\" v=\"removed\""));
    assert!((DOC_DRIFT.selected)(&DocDriftArgs { doc_drift: true }));
    assert!((FLAGS.selected)(&FlagsArgs { flags: true }));
    assert!((MENTIONS.selected)(&MentionsArgs {
        mentions: Some("overview".to_string()),
    }));
    assert!((WHEREIS.selected)(&WhereisArgs {
        whereis: Some("overview".to_string()),
        ..Default::default()
    }));
    assert!(!(DOC_DRIFT.selected)(&DocDriftArgs::default()));
    assert!(!(FLAGS.selected)(&FlagsArgs::default()));
    assert!(!(MENTIONS.selected)(&MentionsArgs::default()));
    assert!(!(WHEREIS.selected)(&WhereisArgs::default()));
}

#[test]
fn test_render_whereis_xml_branch_and_history_consumer() {
    let facts = WhereisFacts {
        history: Some(serde_json::json!({
            "probed": 1,
            "head": "c0ffee",
            "fate": {
                "v": "removed",
                "commit": "1234567",
                "date": "2026-10-19",
                "subject": "remove target symbol"
            }
        })),
        head_date: Some("2026-10-21".to_string()),
        at_stamp: Some("c0ffee".to_string()),
        branch_rows: vec![serde_json::json!({
            "ref": "alt-branch",
            "tip": "7654321",
            "date": "2026-10-20",
            "symbol": "target",
            "kind": "ref",
            "file": "src/alt.rs",
            "path": "src/alt.rs",
            "line": 10,
            "text": "call target();",
            "on_head": false,
        })],
        refs_scanned: 2,
    };
    let ingest = IngestResult::new();
    let interner = Interner::new();
    let sources: [(&str, &[u8]); 0] = [];
    let val = whereis_from_sources_with_facts(&sources, &ingest, &interner, "target", &facts);
    let mut buf = Vec::new();
    render_whereis_xml(&val, Some("all"), true, &mut buf).unwrap();
    let out = std::str::from_utf8(&buf).unwrap();
    assert!(out.contains("ripwire-rs.whereis/v1"));
    assert!(out.contains("alt-branch"));
    assert!(out.contains("call target();"));
    assert!(out.contains("fate sym=\"target\" v=\"removed\""));
    assert!(out.contains("head=\"c0ffee\""));
}

#[test]
fn test_session_plugins_render_in_memory_content() {
    let mut buf = Vec::new();
    render_notes_xml("pinned in-memory note", &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("pinned in-memory note"));

    buf.clear();
    let flags_val = serde_json::json!({ "flags": [] });
    assert!(flag_entries("nonexistent_gate", &flags_val).is_err());
    assert!(String::from_utf8_lossy(&buf).is_empty());

    assert!((NOTES.selected)(&NotesArgs { notes: true }));
    assert!((FLIP.selected)(&FlipArgs {
        flip: Some("nonexistent_gate".to_string()),
    }));
    assert!(!(NOTES.selected)(&NotesArgs::default()));
    assert!(!(FLIP.selected)(&FlipArgs::default()));
}
