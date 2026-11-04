use ripwire_rs::commands::doctor::{
    DoctorCheck, DoctorReport, binary_path_row, classify_fsmonitor, index_artifact,
    index_cache_row, index_source, is_cache_blob_name, open_error_state, probe_cache_artifact,
};
use std::path::{Path, PathBuf};

fn sample_report() -> DoctorReport {
    DoctorReport {
        at: Ok(Some("abc123def+dirty".to_string())),
        built_from: "0.1.0".to_string(),
        checks: vec![
            DoctorCheck::new("grammars", true)
                .attr("loaded", 26)
                .attr("expected", 26),
            DoctorCheck::new("cache-dir", false).attr("dir", "/tmp/a&b\"c"),
            DoctorCheck::new("layout", true),
        ],
    }
}

#[test]
fn test_doctor_report_with_legend_is_well_formed_xml() {
    let mut out = Vec::new();
    sample_report().write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.starts_with("<!--"), "the legend is part of the report");
    let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{e}: {xml}"));
    assert_eq!(doc.root_element().tag_name().name(), "doctor");
}

fn report_outside_git(checks: Vec<DoctorCheck>) -> DoctorReport {
    DoctorReport {
        at: Ok(None),
        built_from: "x".to_string(),
        checks,
    }
}

fn render_doctor(report: &DoctorReport) -> String {
    let mut out = Vec::new();
    report.write_xml(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn test_doctor_report_omits_at_outside_git() {
    let xml = render_doctor(&report_outside_git(vec![DoctorCheck::new(
        "binary-path",
        true,
    )]));
    let header =
        "<doctor schema=\"ripwire-rs.doctor/v1\" checks=\"1\" passed=\"1\" built_from=\"x\">";
    assert!(xml.contains(header), "{xml}");
    assert!(!xml.contains(" at=\""), "{xml}");
    assert!(!xml.contains("n=\"layout\""), "{xml}");
}

#[test]
fn test_doctor_passed_counts_ok_rows() {
    let report = sample_report();
    assert_eq!(report.passed(), 2);
    assert_eq!(report.checks.len(), 3);
}

#[test]
fn test_doctor_xml_rows_and_counts() {
    let mut out = Vec::new();
    sample_report().write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.starts_with("<!-- ripwire-rs doctor"));
    assert!(xml.contains(
        "<doctor schema=\"ripwire-rs.doctor/v1\" checks=\"3\" passed=\"2\" at=\"abc123def+dirty\" built_from=\"0.1.0\">"
    ));
    assert!(xml.contains("<c n=\"grammars\" ok=\"1\" loaded=\"26\" expected=\"26\"/>"));
    assert!(xml.contains("<c n=\"cache-dir\" ok=\"0\" dir=\"/tmp/a&amp;b&quot;c\"/>"));
    assert!(xml.contains("<c n=\"layout\" ok=\"1\"/>"));
    assert!(xml.trim_end().ends_with("</doctor>"));
    assert_eq!(xml.matches("<c n=\"").count(), 3);
}

#[test]
fn test_index_source_follows_no_cache_then_cache_flag() {
    let cache = std::path::Path::new("/tmp/explicit.json");
    assert_eq!(index_source(false, None), "auto");
    assert_eq!(index_source(false, Some(cache)), "cache-flag");
    assert_eq!(index_source(true, None), "disabled");
    assert_eq!(index_source(true, Some(cache)), "disabled");
}

#[test]
fn test_doctor_empty_report_passes_vacuously() {
    let report = report_outside_git(Vec::new());
    let mut out = Vec::new();
    report.write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("checks=\"0\" passed=\"0\""));
}

#[test]
fn test_binary_path_row_found_different_file_prints_version() {
    let check = binary_path_row(
        Some(Path::new("/build/ripwire-rs")),
        Some(Path::new("/usr/local/bin/ripwire-rs")),
        false,
        Some("ripwire-rs 0.6.3".to_string()),
    );
    let mut out = Vec::new();
    check.write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("ok=\"1\""));
    assert!(xml.contains("which=\"/usr/local/bin/ripwire-rs\""));
    assert!(xml.contains("which_version=\"ripwire-rs 0.6.3\""));
    assert!(xml.contains("on_path=\"1\" same_file=\"0\""));
    assert!(!xml.contains("hint="));
}

#[test]
fn test_binary_path_row_same_file_omits_version_even_when_supplied() {
    let check = binary_path_row(
        Some(Path::new("/bin/ripwire-rs")),
        Some(Path::new("/bin/ripwire-rs")),
        true,
        Some("ripwire-rs 0.6.3".to_string()),
    );
    let mut out = Vec::new();
    check.write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(!xml.contains("which_version="));
    assert!(xml.contains("same_file=\"1\""));
}

#[test]
fn test_binary_path_row_not_found_fails_and_exports_path_hint() {
    let check = binary_path_row(Some(Path::new("/opt/bin/ripwire-rs")), None, false, None);
    let mut out = Vec::new();
    check.write_xml(&mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("ok=\"0\""));
    assert!(xml.contains("on_path=\"0\""));
    assert!(xml.contains("hint=\"export PATH=&quot;/opt/bin:$PATH&quot;\""));
}

#[test]
fn test_index_artifact_chooses_cache_file_only_when_cache_is_enabled() {
    let auto = PathBuf::from("/tmp/auto.json");
    let cache = Path::new("/tmp/explicit.json");
    assert_eq!(
        index_artifact(None, false, auto.clone()),
        ("auto", auto.clone())
    );
    assert_eq!(
        index_artifact(Some(cache), false, auto.clone()),
        ("cache-flag", cache.to_path_buf())
    );
    assert_eq!(
        index_artifact(Some(cache), true, auto.clone()),
        ("disabled", auto)
    );
}

#[test]
fn test_open_error_state_distinguishes_absent_from_unreadable() {
    assert_eq!(open_error_state(std::io::ErrorKind::NotFound), "absent");
    assert_eq!(
        open_error_state(std::io::ErrorKind::PermissionDenied),
        "unreadable"
    );
}

#[test]
fn test_index_cache_row_corrupt_or_unreadable_fails_the_row() {
    let path = Path::new("/tmp/a.json");
    assert!(index_cache_row("auto", path, "ok").ok);
    assert!(index_cache_row("auto", path, "absent").ok);
    assert!(!index_cache_row("auto", path, "corrupt").ok);
    assert!(!index_cache_row("auto", path, "unreadable").ok);
}

#[test]
fn test_classify_fsmonitor() {
    assert_eq!(classify_fsmonitor(None), "unset");
    assert_eq!(classify_fsmonitor(Some("true")), "builtin");
    assert_eq!(classify_fsmonitor(Some("Yes")), "builtin");
    assert_eq!(classify_fsmonitor(Some("false")), "off");
    assert_eq!(classify_fsmonitor(Some("")), "off");
    assert_eq!(
        classify_fsmonitor(Some(".git/hooks/fsmonitor-watchman")),
        "hook"
    );
}

#[test]
fn test_is_cache_blob_name() {
    assert!(is_cache_blob_name("ripwire-rs-cache-1a2b.json"));
    assert!(!is_cache_blob_name("ripwire-rs-cache-1a2b.tmp"));
    assert!(!is_cache_blob_name("ripwire-rs-cache-.json"));
    assert!(!is_cache_blob_name("other.json"));
}

#[test]
fn test_probe_cache_artifact() {
    let current = format!(
        "{{\"version\":{},\"validation_key\":\"k\"}}",
        ripwire_rs::cache::CACHE_VERSION
    );
    assert_eq!(probe_cache_artifact(current.as_bytes()), "ok");
    let stale = format!("{{\"version\":{}}}", ripwire_rs::cache::CACHE_VERSION + 1);
    assert_eq!(probe_cache_artifact(stale.as_bytes()), "cache-version");
    assert_eq!(probe_cache_artifact(&b"not json"[..]), "corrupt");
}
