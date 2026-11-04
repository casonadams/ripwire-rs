use ripwire_rs::git::merge_scout::worktree_bytes;
use std::io::{self, ErrorKind};

#[test]
fn test_worktree_bytes_deleted_file_reads_as_empty() {
    let gone = Err(io::Error::new(ErrorKind::NotFound, "gone"));
    assert_eq!(worktree_bytes("src/a.rs", gone), Ok(Vec::new()));
}

#[test]
fn test_worktree_bytes_unreadable_file_is_an_error_naming_path() {
    let denied = Err(io::Error::new(ErrorKind::PermissionDenied, "denied"));
    let err = worktree_bytes("src/a.rs", denied).unwrap_err();
    assert!(err.starts_with("src/a.rs: "), "{err}");
}

#[test]
fn test_worktree_bytes_reads_content() {
    assert_eq!(
        worktree_bytes("src/a.rs", Ok(b"fn a() {}".to_vec())),
        Ok(b"fn a() {}".to_vec())
    );
}
