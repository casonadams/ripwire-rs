use ripwire_rs::ingest::unreadable_rel_paths;
use std::io;

fn denied() -> io::Error {
    io::Error::from(io::ErrorKind::PermissionDenied)
}

#[test]
fn read_error_is_reported_by_rel_path() {
    let reads: Vec<(&str, Result<Vec<u8>, io::Error>)> = vec![
        ("src/a.rs", Ok(b"fn a() {}".to_vec())),
        ("src/locked.rs", Err(denied())),
    ];
    assert_eq!(unreadable_rel_paths(reads), vec!["src/locked.rs"]);
}

#[test]
fn successful_read_of_binary_bytes_is_not_a_failure() {
    let reads: Vec<(&str, Result<Vec<u8>, io::Error>)> =
        vec![("bin/tool.dat", Ok(vec![0, 159, 146, 150, 0]))];
    assert!(unreadable_rel_paths(reads).is_empty());
}

#[test]
fn no_read_errors_yields_no_unreadable_files() {
    let reads: Vec<(&str, Result<(), io::Error>)> = vec![("a.rs", Ok(())), ("b.rs", Ok(()))];
    assert!(unreadable_rel_paths(reads).is_empty());
}

#[test]
fn unreadable_paths_are_sorted_and_distinct_whatever_the_input_order() {
    let forward = vec![
        ("z.rs", Err(denied())),
        ("a.rs", Err(denied())),
        ("m.rs", Ok(())),
        ("a.rs", Err(denied())),
    ];
    let reversed = vec![
        ("a.rs", Err(denied())),
        ("m.rs", Ok(())),
        ("a.rs", Err(denied())),
        ("z.rs", Err(denied())),
    ];
    assert_eq!(unreadable_rel_paths(forward), vec!["a.rs", "z.rs"]);
    assert_eq!(unreadable_rel_paths(reversed), vec!["a.rs", "z.rs"]);
}
