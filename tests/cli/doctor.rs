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
fn binary_path_row_reports_location_version_and_hint() {
    type Case<'a> = (
        Option<&'a str>,
        Option<&'a str>,
        bool,
        Option<&'a str>,
        &'a [&'a str],
        &'a [&'a str],
    );
    let cases: [Case; 3] = [
        (
            Some("/build/ripwire-rs"),
            Some("/usr/local/bin/ripwire-rs"),
            false,
            Some("ripwire-rs 0.6.3"),
            &[
                "ok=\"1\"",
                "which=\"/usr/local/bin/ripwire-rs\"",
                "which_version=\"ripwire-rs 0.6.3\"",
                "on_path=\"1\" same_file=\"0\"",
            ],
            &["hint="],
        ),
        (
            Some("/bin/ripwire-rs"),
            Some("/bin/ripwire-rs"),
            true,
            Some("ripwire-rs 0.6.3"),
            &["same_file=\"1\""],
            &["which_version="],
        ),
        (
            Some("/opt/bin/ripwire-rs"),
            None,
            false,
            None,
            &[
                "ok=\"0\"",
                "on_path=\"0\"",
                "hint=\"export PATH=&quot;/opt/bin:$PATH&quot;\"",
            ],
            &[],
        ),
    ];
    for (running, on_path, same_file, version, present, absent) in cases {
        let check = binary_path_row(
            running.map(Path::new),
            on_path.map(Path::new),
            same_file,
            version.map(str::to_owned),
        );
        let mut out = Vec::new();
        check.write_xml(&mut out).unwrap();
        let xml = String::from_utf8(out).unwrap();
        for needle in present {
            assert!(xml.contains(needle), "missing {needle}: {xml}");
        }
        for needle in absent {
            assert!(!xml.contains(needle), "unexpected {needle}: {xml}");
        }
    }
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
    let inputs = [
        None,
        Some("true"),
        Some("Yes"),
        Some("false"),
        Some(""),
        Some(".git/hooks/fsmonitor-watchman"),
    ];
    let got: Vec<&str> = inputs.into_iter().map(classify_fsmonitor).collect();
    assert_eq!(got, ["unset", "builtin", "builtin", "off", "off", "hook"]);
}

#[test]
fn test_is_cache_blob_name() {
    let cases = [
        ("ripwire-rs-cache-1a2b.bin", true),
        ("ripwire-rs-cache-1a2b.tmp", false),
        ("ripwire-rs-cache-.bin", false),
        ("other.bin", false),
        ("ripwire-rs-cache-1a2b.json", false),
    ];
    for (name, expected) in cases {
        assert_eq!(is_cache_blob_name(name), expected, "{name}");
    }
}

#[test]
fn test_probe_cache_artifact() {
    let payload = ripwire_rs::cache::CachePayload {
        validation_key: "k".into(),
        interner_strings: Vec::new(),
        ingest: ripwire_rs::IngestResult::default(),
        graph: ripwire_rs::Graph::default(),
        pr: ripwire_rs::PageRankResult {
            iterations: 0,
            converged: true,
            ranks: Vec::new(),
        },
    };
    let current = ripwire_rs::cache::encode_payload(&payload).unwrap();
    assert_eq!(probe_cache_artifact(current.as_slice()), "ok");
    let mut stale = current.clone();
    stale[4..8].copy_from_slice(&(ripwire_rs::cache::CACHE_VERSION + 1).to_le_bytes());
    assert_eq!(probe_cache_artifact(stale.as_slice()), "cache-version");
    assert_eq!(probe_cache_artifact(&b"not a blob"[..]), "corrupt");
    assert_eq!(
        probe_cache_artifact(&br#"{"version":38,"validation_key":"k"}"#[..]),
        "cache-version"
    );
}
