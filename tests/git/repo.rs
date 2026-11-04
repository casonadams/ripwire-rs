use ripwire_rs::git::repo::{short_id, text_changes};

#[test]
fn test_text_changes_binary_content_is_left_out() {
    assert!(text_changes(b"a\0b\n", b"a\nb\n").is_none());
    assert!(text_changes(b"a\n", b"a\0b\n").is_none());
    let changes = text_changes(b"a\nb\n", b"a\nc\n").expect("text file has changed lines");
    assert_eq!(changes.ranges, vec![1..2]);
}

#[test]
fn test_blob_text_of_marks_binary_as_absent() {
    use ripwire_rs::git::repo::blob_text_of;

    assert_eq!(blob_text_of(b"a\nb\n"), Some("a\nb\n".to_string()));
    assert_eq!(blob_text_of(b"a\0b\n"), None);
}

#[test]
fn test_relative_prefix_is_none_outside_the_work_tree() {
    use ripwire_rs::git::repo::relative_prefix;
    use std::path::Path;

    assert_eq!(
        relative_prefix(Path::new("/w"), Path::new("/w/dir")),
        Some("dir/".to_string())
    );
    assert_eq!(
        relative_prefix(Path::new("/w"), Path::new("/w")),
        Some(String::new())
    );
    assert_eq!(relative_prefix(Path::new("/w"), Path::new("/x")), None);
}

#[test]
fn test_short_id_formatting_and_length() {
    let oid = gix::ObjectId::from_hex(b"0123456789abcdef0123456789abcdef01234567").unwrap();
    let sid = short_id(oid);
    assert_eq!(sid, "012345678");
    assert_eq!(sid.len(), 9);

    let zeros = gix::ObjectId::null(gix::hash::Kind::Sha1);
    let zero_sid = short_id(zeros);
    assert_eq!(zero_sid, "000000000");
}

#[test]
fn test_has_word_boundaries_and_identifiers() {
    use ripwire_rs::git::repo::has_word;

    assert!(has_word("hello world", "world"));
    assert!(has_word("world", "world"));
    assert!(has_word("(world);", "world"));
    assert!(has_word("hello world!", "world"));

    // Identifier characters: alphanumeric + '_'
    assert!(!has_word("hello_world", "world"));
    assert!(!has_word("world_hello", "world"));
    assert!(!has_word("worldwide", "world"));
    assert!(!has_word("myworld", "world"));
    assert!(!has_word("world1", "world"));

    // Empty cases
    assert!(!has_word("anything", ""));
    assert!(!has_word("", "world"));
}

fn blob(c: char) -> gix::ObjectId {
    gix::ObjectId::from_hex(c.to_string().repeat(40).as_bytes()).unwrap()
}

fn tree(entries: &[(&str, char)]) -> std::collections::BTreeMap<String, gix::ObjectId> {
    entries
        .iter()
        .map(|(path, c)| ((*path).to_string(), blob(*c)))
        .collect()
}

fn dirty(paths: &[&str]) -> std::collections::BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn test_changed_paths_compares_committed_dirty_and_untracked_files_against_base() {
    use ripwire_rs::git::repo::changed_paths;
    type Entries<'a> = &'a [(&'a str, char)];
    type Case<'a> = (
        Entries<'a>,
        Entries<'a>,
        &'a [&'a str],
        Entries<'a>,
        &'a [&'a str],
    );
    let cases: [Case; 5] = [
        (
            &[("src/a.rs", 'a'), ("src/b.rs", 'b')],
            &[("src/a.rs", 'c'), ("src/b.rs", 'b')],
            &[],
            &[],
            &["src/a.rs"],
        ),
        (&[("src/a.rs", 'a')], &[("src/a.rs", 'a')], &[], &[], &[]),
        (
            &[("src/a.rs", 'a')],
            &[("src/a.rs", 'a')],
            &["src/a.rs"],
            &[("src/a.rs", 'd')],
            &["src/a.rs"],
        ),
        (
            &[("src/a.rs", 'a')],
            &[("src/a.rs", 'c')],
            &["src/a.rs"],
            &[("src/a.rs", 'a')],
            &[],
        ),
        (
            &[("src/gone.rs", 'e')],
            &[("src/gone.rs", 'e')],
            &["src/gone.rs", "src/new.rs"],
            &[("src/new.rs", 'f')],
            &["src/gone.rs", "src/new.rs"],
        ),
    ];
    for (base, head, dirty_paths, worktree, expected) in cases {
        let worktree = tree(worktree);
        let changed = changed_paths(&tree(base), &tree(head), &dirty(dirty_paths), |path| {
            worktree.get(path).copied()
        });
        assert_eq!(changed, expected);
    }
}

#[test]
fn test_relative_prefix_is_dir_with_trailing_slash_or_empty_at_top() {
    use ripwire_rs::git::repo::relative_prefix;
    use std::path::Path;
    let work = Path::new("/work/tree");
    assert_eq!(relative_prefix(work, work).as_deref(), Some(""));
    assert_eq!(
        relative_prefix(work, Path::new("/work/tree/src/git")).as_deref(),
        Some("src/git/")
    );
    assert_eq!(relative_prefix(work, Path::new("/work/other")), None);
    assert_eq!(relative_prefix(work, Path::new("/work/treeish")), None);
}

#[test]
fn test_base_of_range_accepts_rev_or_rev_to_head_only() {
    use ripwire_rs::git::repo::base_of_range;
    let cases = [
        ("main", Ok("main")),
        ("main..HEAD", Ok("main")),
        ("main...HEAD", Ok("main")),
        ("main..", Ok("main")),
        ("main..feature", Err(())),
        ("..HEAD", Err(())),
    ];
    for (spec, want) in cases {
        assert_eq!(base_of_range(spec).map_err(|_| ()), want, "{spec}");
    }
}

/// (new-side changed ranges as `(start, end)`, line start offsets)
type SideChanges = (Vec<(u32, u32)>, Vec<u32>);

#[test]
fn test_new_side_changes_reports_new_lines_and_their_byte_starts() {
    use ripwire_rs::git::repo::new_side_changes;
    let cases: [(&str, &[u8], &[u8], SideChanges); 3] = [
        (
            "edit",
            b"a\nb\nc\n",
            b"a\nB\nc\n",
            (vec![(1, 2)], vec![0, 2, 4]),
        ),
        (
            "deletion",
            b"a\nb\nc\n",
            b"a\nc\n",
            (vec![(1, 1)], vec![0, 2]),
        ),
        ("unchanged", b"a\nb\n", b"a\nb\n", (vec![], vec![0, 2])),
    ];
    for (case, old, new, want) in cases {
        let got = new_side_changes(old, new);
        let ranges = got.ranges.iter().map(|r| (r.start, r.end)).collect();
        assert_eq!((ranges, got.line_starts), want, "{case}");
    }
}
