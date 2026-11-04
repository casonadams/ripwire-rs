use crate::nonlocal_support::state_report;
use ripwire_rs::Lang;

/// Asserts that reads of immutable bindings in `files` are not reported as mutable state.
pub fn assert_reads_are_not_state(lang: Lang, files: &[(&str, &str)]) {
    let report = state_report(lang, files);
    assert!(
        report.is_empty(),
        "immutable {lang:?} reads reported as state: {:?}",
        report.iter().map(|f| &f.sym_name).collect::<Vec<_>>()
    );
}
