//! Test, benchmark, and fixture path predicates shared by ingest, ranking, gating, and serialization.
use crate::domain::Lang;
use crate::languages::has_dir_component;
use std::path::Path;

/// Check if a path corresponds to a test file by directory convention or filename.
#[must_use]
pub fn is_test_path<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    if has_dir_component(p, |d| {
        matches!(d, "test" | "tests" | "spec" | "specs" | "testdata")
    }) {
        return true;
    }
    let sem = Lang::from_path(p).semantics();
    if !sem.supports_test_partners() {
        return false;
    }
    p.file_name()
        .and_then(|s| s.to_str())
        .and_then(|n| n.rsplit_once('.'))
        .is_some_and(|(stem, _)| sem.test_partner_subject(stem, false).is_some())
}

/// Check if a path is a test, benchmark, or fixture path for ranking and module naming.
#[must_use]
pub fn is_test_or_bench_path<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    if is_test_path(p)
        || has_dir_component(p, |d| {
            matches!(d, "bench" | "benches" | "fixtures" | "__tests__")
                || d.ends_with("_tests")
                || d.ends_with("_test")
        })
    {
        return true;
    }
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    Lang::from_path(p).semantics().is_test_module_stem(stem)
}
