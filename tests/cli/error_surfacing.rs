use ripwire_rs::commands::common::edit::payload_or_inline;
use ripwire_rs::commands::from_trace::stdin_trace;
use ripwire_rs::commands::plan_lanes::read_brief;
use ripwire_rs::commands::plan_lint::read_plan;
use ripwire_rs::git::repo::CommitSummary;
use ripwire_rs::query::branch_tools::name_fate;
use std::io::{self, ErrorKind};

fn io_err(kind: ErrorKind) -> io::Error {
    io::Error::new(kind, "fixture")
}

#[test]
fn test_read_plan_missing_file_is_error_naming_path() {
    let err = read_plan("PLAN.md", Err(io_err(ErrorKind::NotFound))).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(err.to_string().contains("PLAN.md"));
    assert_eq!(
        read_plan("PLAN.md", Ok("# Plan\n".to_string())).unwrap(),
        "# Plan\n"
    );
}

#[test]
fn test_payload_or_inline_falls_back_only_when_missing() {
    let inline = payload_or_inline("{\"a\":1}", Err(io_err(ErrorKind::NotFound))).unwrap();
    assert_eq!(inline, "{\"a\":1}");
    let err = payload_or_inline("edit.json", Err(io_err(ErrorKind::PermissionDenied))).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    assert!(err.to_string().contains("edit.json"));
    assert_eq!(
        payload_or_inline("edit.json", Ok("body".to_string())).unwrap(),
        "body"
    );
}

#[test]
fn test_read_brief_error_is_not_an_empty_brief() {
    let lines = read_brief("lanes.txt", Ok("a b\nc d\n".to_string())).unwrap();
    assert_eq!(lines, vec!["a b", "c d"]);
    let err = read_brief("lanes.txt", Err(io_err(ErrorKind::PermissionDenied))).unwrap_err();
    assert!(err.to_string().contains("lanes.txt"));
}

#[test]
fn test_stdin_trace_read_failure_is_error() {
    let text = stdin_trace(&b"panic at src/a.rs:3\n"[..]).unwrap();
    assert_eq!(text, "panic at src/a.rs:3\n");

    struct Broken;
    impl io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io_err(ErrorKind::BrokenPipe))
        }
    }
    assert!(stdin_trace(Broken).is_err());
}

#[test]
fn test_name_fate_unreadable_history_is_unavailable_not_never() {
    let unavailable = name_fate(Err("history walk: broken".to_string()));
    assert_eq!(unavailable["v"], "unavailable");
    assert_eq!(unavailable["note"], "history walk: broken");

    let never = name_fate(Ok(None));
    assert_eq!(never["v"], "never");

    let commit = CommitSummary {
        short_id: "abc123def".to_string(),
        date: "2024-01-02".to_string(),
        subject: "remove helper".to_string(),
    };
    let removed = name_fate(Ok(Some(commit)));
    assert_eq!(removed["v"], "removed");
    assert_eq!(removed["commit"], "abc123def");
}
