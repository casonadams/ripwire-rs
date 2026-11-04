use ripwire_rs::git::remote::{is_remote_git_url, remote_cache_dir};
use std::path::Path;

#[test]
fn test_remote_root_classification() {
    let cases = [
        ("https://github.com/org/repo", true),
        ("http://host/org/repo.git", true),
        ("ssh://git@host/org/repo.git", true),
        ("git://host/org/repo", true),
        ("git@github.com:org/repo.git", true),
        ("deploy@host:org/repo.git", true),
        (".", false),
        ("src", false),
        ("../repo.git", false),
        ("/srv/mirrors/repo.git", false),
        ("C:\\work\\repo", false),
    ];
    for (root, want) in cases {
        assert_eq!(is_remote_git_url(root), want, "{root}");
    }
}

#[test]
fn test_remote_cache_dir_is_stable_per_url_under_base() {
    let base = Path::new("/cache-base");
    let a = remote_cache_dir(base, "https://github.com/org/repo");
    assert_eq!(a, remote_cache_dir(base, "https://github.com/org/repo"));
    assert_ne!(a, remote_cache_dir(base, "https://github.com/org/other"));
    assert!(a.starts_with(base.join("ripwire-rs")));
}
