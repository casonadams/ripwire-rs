use ripwire_rs::languages::c::flags::extract_cmake_option;
use ripwire_rs::query::doc_tools::*;
use ripwire_rs::query::safety::CochangeOptions;
use ripwire_rs::query::safety::*;
use ripwire_rs::query::task_route::*;
use ripwire_rs::*;
use std::collections::HashSet;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_report_keeps_incompatible_callers_outside_unflagged_page() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"pub fn target(a: i32, b: i32) -> i32 { a + b }\nfn caller() { target(1); }\n"
                    .as_slice(),
            )],
            &registry,
            &interner,
        );
        let graph = Graph::build(&ingest, &interner);
        let context = SafetyContext {
            root: ":memory:",
            ingest: &ingest,
            graph: &graph,
            interner: &interner,
        };
        let baseline = ripwire_rs::gate::contract::SignatureContract {
            name: "target".into(),
            params: 1,
            is_public: true,
        };
        let mut out = Vec::new();
        write_edit_check_from_facts(
            &EditCheckRequest {
                sym_name: "target",
                head_contract: Some(&baseline),
                page: Some((0, 0)),
                at_stamp: Some("abc:date"),
            },
            &context,
            &mut out,
        )
        .unwrap();
        let xml = String::from_utf8(out).unwrap();
        assert!(xml.contains("status=\"contract-change\""));
        assert!(xml.contains("incompatible=\"1\""));
        assert!(xml.contains("n=\"caller\""));
        assert!(xml.contains("at=\"abc:date\""));
        assert!(xml.contains("shown_unflagged=\"0\""));
    }

    #[test]
    fn baseline_policy_requires_explicit_absorption_and_preserves_head_stamp() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[("src/lib.rs", Lang::Rust, b"pub fn target() {}\n".as_slice())],
            &registry,
            &interner,
        );
        let graph = Graph::build(&ingest, &interner);
        let context = SafetyContext {
            root: ":memory:",
            ingest: &ingest,
            graph: &graph,
            interner: &interner,
        };
        let error =
            baseline_from_facts(&context, "abc", Some((2, "cc target")), false).unwrap_err();
        assert!(error.to_string().contains("2 gating finding(s)"));
        assert!(error.to_string().contains("cc target"));
        let dirty = baseline_from_facts(&context, "abc", Some((2, "cc target")), true).unwrap();
        assert!(dirty.dirty);
        assert_eq!(dirty.absorbed, 2);
        let clean = baseline_from_facts(&context, "abc", None, false).unwrap();
        assert!(!clean.dirty);
        assert_eq!(clean.absorbed, 0);
    }
}

#[cfg(test)]
mod affected_tests {
    use super::*;

    #[test]
    fn affected_report_enriches_real_test_sources_without_filesystem() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let source = b"def test_behavior():\n    assert True\n\nif __name__ == '__main__':\n    test_behavior()\n";
        let ingest = ingest_in_memory_files(
            &[("tests/test_behavior.py", Lang::Python, source.as_slice())],
            &registry,
            &interner,
        );
        let graph = Graph::build(&ingest, &interner);
        let context = SafetyContext {
            root: ":memory:",
            ingest: &ingest,
            graph: &graph,
            interner: &interner,
        };
        let sources: &[(&str, &[u8])] = &[("tests/test_behavior.py", source)];
        let report = affected_from_sources("tests/test_behavior.py", &context, sources);
        assert_eq!(report.tests.len(), 1);
        assert!(report.tests[0].is_direct_change);
        assert!(
            report.tests[0]
                .run
                .as_deref()
                .unwrap()
                .contains("tests/test_behavior.py")
        );
        let missing: &[(&str, &[u8])] = &[];
        let without_source = affected_from_sources("tests/test_behavior.py", &context, missing);
        assert_eq!(without_source.tests.len(), 1);
        assert!(without_source.tests[0].run.is_none());
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn shared_cochange_query_mines_supplied_history_and_applies_recurrence_filter() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[
                ("src/a.rs", Lang::Rust, b"pub fn alpha() {}\n".as_slice()),
                ("src/b.rs", Lang::Rust, b"pub fn beta() {}\n".as_slice()),
            ],
            &registry,
            &interner,
        );
        let graph = Graph::build(&ingest, &interner);
        let context = SafetyContext {
            root: ":memory:",
            ingest: &ingest,
            graph: &graph,
            interner: &interner,
        };
        let commits: Vec<_> = (0..6)
            .map(|idx| ripwire_rs::git::history::CommitInfo {
                id: format!("c{idx}"),
                timestamp: 1_700_000_000 - idx * 60 * 86400,
                author: "Ada".into(),
                changed_files: vec!["src/a.rs".into(), "src/b.rs".into()],
            })
            .collect();
        let mut out = Vec::new();
        execute_cochange_with_options(
            Some("src/a.rs"),
            CochangeOptions::default(),
            &commits,
            &context,
            &mut out,
        )
        .unwrap();
        let xml = String::from_utf8(out).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let partner = doc
            .descendants()
            .find(|n| n.has_tag_name("partner"))
            .expect("partner node");
        assert_eq!(partner.attribute("path"), Some("src/b.rs"));
        assert_eq!(partner.attribute("together"), Some("6"));
        let mut filtered = Vec::new();
        execute_cochange_with_options(
            Some("src/a.rs"),
            CochangeOptions {
                cochange_recur: Some(4),
                ..Default::default()
            },
            &commits,
            &context,
            &mut filtered,
        )
        .unwrap();
        let filt_xml = String::from_utf8(filtered).unwrap();
        let filt_doc = roxmltree::Document::parse(&filt_xml).unwrap();
        let root = filt_doc
            .descendants()
            .find(|n| n.has_tag_name("f"))
            .expect("f node");
        assert_eq!(root.attribute("partners"), Some("0"));
        assert!(
            filt_doc
                .descendants()
                .find(|n| n.has_tag_name("partner"))
                .is_none()
        );
        let situ = SituationalReport {
            root: ":memory:",
            files: &["src/a.rs".to_string()],
            commits: Some(&commits),
            context,
            page: ripwire_rs::serialize::PageRequest::default(),
        };
        let situ_xml = situ.to_xml_string();
        let situ_doc = roxmltree::Document::parse(&situ_xml).unwrap();
        let changed = situ_doc
            .descendants()
            .find(|n| n.has_tag_name("changed"))
            .expect("changed node");
        assert_eq!(changed.attribute("path"), Some("src/a.rs"));
        let partner = situ_doc
            .descendants()
            .find(|n| n.has_tag_name("partner"))
            .expect("partner node");
        let partner_path = partner.attribute("path").expect("partner path");
        assert!(partner_path.starts_with("src/b.rs"));
    }
}
#[cfg(test)]
mod doc_tests {
    use super::*;
    use ripwire_rs::{Lang, LanguageRegistry, ingest_in_memory_files};

    #[test]
    fn docs_and_source_facts_drive_drift_mentions_and_locations() {
        let source = b"pub fn current_api() {}\nfn caller() { current_api(); }\n// hidden_token is intentionally not an indexed definition\n";
        let docs =
            b"Use `current_api` and `current_api`; replace `removed_api()` and `hidden_token()`.\n";
        let dated = b"2020-01-01\nUse `historical_api()`.\n";
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[
                ("src/lib.rs", Lang::Rust, source.as_slice()),
                ("README.md", Lang::Markdown, docs.as_slice()),
                ("CHANGELOG.md", Lang::Markdown, dated.as_slice()),
            ],
            &registry,
            &interner,
        );
        let sources: &[(&str, &[u8])] = &[
            ("src/lib.rs", source),
            ("README.md", docs),
            ("CHANGELOG.md", dated),
        ];
        let mentions = mentions_from_sources(sources, &ingest, &interner, "current_api");
        assert_eq!(mentions["total_mentions"], 2);
        assert_eq!(mentions["docs"][0]["file"], "README.md");
        let drift = doc_drift_from_sources(":memory:", sources, &ingest, &interner, &[]);
        assert_eq!(drift["docs_scanned"], 2);
        assert_eq!(drift["dated_docs"], 1);
        let missing: Vec<_> = drift["drift"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["referenced"].as_str())
            .collect();
        assert!(missing.contains(&"removed_api"));
        assert!(!missing.contains(&"hidden_token"));
        assert!(!missing.contains(&"historical_api"));
        let locations = ripwire_rs::query::branch_tools::whereis_from_sources_with_facts(
            sources,
            &ingest,
            &interner,
            "current_api",
            &ripwire_rs::query::branch_tools::WhereisFacts::default(),
        );
        assert_eq!(locations["git"], false);
        assert!(locations["on_head"].is_null());
        assert_eq!(locations["hits"][0]["kind"], "def");
        assert_eq!(locations["hits"][0]["line"], 1);
        assert!(
            locations["hits"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["kind"] == "ref" && row["line"] == 2)
        );
    }

    #[test]
    fn supplied_flags_skip_prose_and_preserve_read_sites() {
        let source =
            b"import os\nvalue = os.getenv(\"FEATURE_X\")\nother = os.environ.get(\"FEATURE_X\")\n";
        let docs = b"`os.getenv(\"DOCUMENTATION_ONLY\")`\n";
        let cmake_lists = b"option(BUILD_TESTS \"Build tests\" ON)\n";
        let cmake_opts = b"option(DARK_FEATURE \"Dark feature\" OFF)\n";
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[
                ("app.py", Lang::Python, source.as_slice()),
                ("README.md", Lang::Markdown, docs.as_slice()),
            ],
            &registry,
            &interner,
        );
        let sources: &[(&str, &[u8])] = &[
            ("app.py", source),
            ("README.md", docs),
            ("CMakeLists.txt", cmake_lists),
            ("cmake/options.cmake", cmake_opts),
        ];
        let report = flags_from_sources(":memory:", sources, &ingest, &interner);
        assert_eq!(report["env_count"], 1);
        let flags = report["flags"].as_array().unwrap();
        let feat_x = flags
            .iter()
            .find(|f| f["name"] == "FEATURE_X")
            .expect("FEATURE_X flag");
        assert_eq!(feat_x["reads_count"], 2);
        assert_eq!(feat_x["reads"][1]["line"], 3);
        assert_eq!(report["cmake_count"], 2);
        let build_tests = flags
            .iter()
            .find(|f| f["name"] == "BUILD_TESTS")
            .expect("BUILD_TESTS flag");
        assert_eq!(build_tests["kind"], "cmake");
        assert_eq!(build_tests["default"], "ON");
        let dark = flags
            .iter()
            .find(|f| f["name"] == "DARK_FEATURE")
            .expect("DARK_FEATURE flag");
        assert_eq!(dark["kind"], "cmake");
        assert_eq!(dark["default"], "OFF");
        assert_eq!(dark["dark"], 1);

        // CMake option extraction unit tests
        assert_eq!(
            extract_cmake_option("option(FEATURE_ON \"doc\" ON)", "CMakeLists.txt"),
            Some(("FEATURE_ON".into(), "ON".into(), false))
        );
        assert_eq!(
            extract_cmake_option("option(FEATURE_OFF \"doc\" OFF)", "cmake/options.cmake"),
            Some(("FEATURE_OFF".into(), "OFF".into(), true))
        );
        assert_eq!(
            extract_cmake_option("option(FEATURE_NO_ARG \"doc\")", "CMakeLists.txt"),
            Some(("FEATURE_NO_ARG".into(), "OFF".into(), true))
        );
        assert_eq!(
            extract_cmake_option("option(FEATURE_ON \"doc\" ON)", "not_cmake.rs"),
            None
        );
        assert_eq!(
            extract_cmake_option("set(FEATURE_ON ON)", "CMakeLists.txt"),
            None
        );
        assert_eq!(
            extract_cmake_option("option( \"doc\" ON)", "CMakeLists.txt"),
            None
        );
    }
}
#[cfg(test)]
mod branch_tests {
    use super::*;

    #[test]
    fn ownership_uses_recency_and_excludes_deleted_or_out_of_scope_paths() {
        let commits = [
            ripwire_rs::git::history::CommitInfo {
                id: "new".into(),
                timestamp: 180 * 86400,
                author: "Ada".into(),
                changed_files: vec!["src/lib.rs".into(), "deleted.rs".into()],
            },
            ripwire_rs::git::history::CommitInfo {
                id: "old".into(),
                timestamp: 0,
                author: "Grace".into(),
                changed_files: vec!["src/lib.rs".into(), "docs/guide.md".into()],
            },
        ];
        let known = HashSet::from(["src/lib.rs".to_string(), "docs/guide.md".to_string()]);
        let report = call_owners(Some(&commits), Some("src/"), Some("new:date"), Some(&known));
        assert_eq!(report["files_analyzed"], 1);
        assert_eq!(report["files"][0]["path"], "src/lib.rs");
        assert_eq!(report["files"][0]["top"], "Ada");
        assert_eq!(report["files"][0]["authors"], 2);
        assert_eq!(report["files"][0]["share"], 0.67);
        assert_eq!(report["at"], "new:date");
        assert!(call_owners(None, None, None, None).get("error").is_some());
    }
}
#[cfg(test)]
mod route_tests {
    use super::*;

    fn route(task: &str, dirty: bool) -> String {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"pub fn parse_request() {}\n".as_slice(),
            )],
            &registry,
            &interner,
        );
        let mut out = Vec::new();
        write_help_task_with_facts(task, ":memory:", &ingest, &interner, true, dirty, &mut out)
            .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn worktree_evidence_changes_pre_push_recommendation() {
        let dirty = route("before I push", true);
        let clean = route("before I push", false);
        assert!(dirty.contains("recommended=\"--situ\""));
        assert!(dirty.contains("dirty=\"1\""));
        assert!(!clean.contains("recommended=\"--situ\""));
        assert!(clean.contains("dirty=\"0\""));
    }

    #[test]
    fn indexed_symbol_contract_and_system_event_have_distinct_policies() {
        let contract = route("I edited parse_request signature", false);
        assert!(contract.contains("recommended=\"--edit-check\""));
        assert!(contract.contains("resolved_symbols=\"1\""));
        assert!(contract.contains("parse_request"));
        let event = route(
            "<system-reminder>who calls parse_request</system-reminder>",
            true,
        );
        assert!(event.contains("status=\"abstain\""));
        assert!(event.contains("resolved_symbols=\"0\""));
        assert!(!event.contains("<choice"));
    }

    #[test]
    fn direct_task_routes_prioritize_closed_claims_quotes_and_connections() {
        let claim = route("calls(parse_request,other)", false);
        assert!(claim.contains("recommended=\"--verify\""));
        assert!(claim.contains("intent=\"verify-claim\""));
        let quoted = route("find \"needle_exact\"", false);
        assert!(quoted.contains("recommended=\"--grep\""));
        assert!(quoted.contains("intent=\"exact-grep\""));
        assert!(quoted.contains("needle_exact"));

        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"pub fn a() {}\npub fn b() {}\npub fn c() {}\n".as_slice(),
            )],
            &registry,
            &interner,
        );
        let mut out = Vec::new();
        write_help_task_with_facts(
            "connect a, b",
            ":memory:",
            &ingest,
            &interner,
            true,
            false,
            &mut out,
        )
        .unwrap();
        let conn2 = String::from_utf8(out).unwrap();
        assert!(conn2.contains("recommended=\"--connect\""));
        assert!(conn2.contains("intent=\"connect-symbols\""));

        let callers = route("who calls parse_request", false);
        assert!(callers.contains("recommended=\"--callers\""));

        let expand = route("how does parse_request work", false);
        assert!(expand.contains("recommended=\"--expand\""));

        let lexical = route("tell me about parse_request", false);
        assert!(!lexical.contains("intent=\"find-callers\""));
    }

    #[test]
    fn identifier_shape_resolves_backticks_calls_camel_and_cues() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[(
                "src/lib.rs",
                Lang::Rust,
                b"pub fn backticked() {}\npub fn called() {}\npub fn camelCase() {}\npub fn cued() {}\npub fn plain() {}\n".as_slice(),
            )],
            &registry,
            &interner,
        );
        let mut out1 = Vec::new();
        write_help_task_with_facts(
            "look at `backticked`",
            ":memory:",
            &ingest,
            &interner,
            true,
            false,
            &mut out1,
        )
        .unwrap();
        assert!(String::from_utf8(out1).unwrap().contains("backticked"));

        let mut out2 = Vec::new();
        write_help_task_with_facts(
            "run called() here",
            ":memory:",
            &ingest,
            &interner,
            true,
            false,
            &mut out2,
        )
        .unwrap();
        assert!(String::from_utf8(out2).unwrap().contains("called"));

        let mut out3 = Vec::new();
        write_help_task_with_facts(
            "examine camelCase now",
            ":memory:",
            &ingest,
            &interner,
            true,
            false,
            &mut out3,
        )
        .unwrap();
        assert!(String::from_utf8(out3).unwrap().contains("camelCase"));

        for cue in [
            "function cued",
            "method cued",
            "symbol cued",
            "calls cued",
            "of cued",
            "understand cued",
        ] {
            let mut out = Vec::new();
            write_help_task_with_facts(
                &format!("inspect {cue}"),
                ":memory:",
                &ingest,
                &interner,
                true,
                false,
                &mut out,
            )
            .unwrap();
            assert!(
                String::from_utf8(out).unwrap().contains("cued"),
                "failed for cue {cue}"
            );
        }

        let mut out_plain = Vec::new();
        write_help_task_with_facts(
            "just plain text",
            ":memory:",
            &ingest,
            &interner,
            true,
            false,
            &mut out_plain,
        )
        .unwrap();
        assert!(
            String::from_utf8(out_plain)
                .unwrap()
                .contains("resolved_symbols=\"0\"")
        );
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use ripwire_rs::query::graph::call_from_trace;

    #[test]
    fn trace_frame_matching_resolves_symbols_at_file_lines_and_token_names() {
        let interner = Interner::new();
        let registry = LanguageRegistry::new().unwrap();
        let ingest = ingest_in_memory_files(
            &[
                (
                    "src/lib.rs",
                    Lang::Rust,
                    b"pub fn handle_event() {}\n".as_slice(),
                ),
                (
                    "src/util.rs",
                    Lang::Rust,
                    b"pub fn format_error() {}\n".as_slice(),
                ),
            ],
            &registry,
            &interner,
        );
        let trace = "Traceback (most recent call last):\n  File \"src/lib.rs\", line 1, in handle_event\n    handle_event()\n  in format_error\n  at unknown_source:999\n";
        let report = call_from_trace(&ingest, &interner, trace);
        let suspects = report["suspects"].as_array().unwrap();
        assert!(
            suspects.iter().any(|s| s["name"] == "handle_event"
                && s["file"] == "src/lib.rs"
                && s["line"] == 1)
        );
        assert!(
            suspects
                .iter()
                .any(|s| s["name"] == "format_error" && s["file"] == "src/util.rs")
        );
        assert!(!suspects.iter().any(|s| s["file"] == "unknown_source"));
    }
}
