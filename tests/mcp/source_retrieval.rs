use ripwire_rs::Interner;
use ripwire_rs::query::detail::{BodyRequest, execute_body_query};
use ripwire_rs::query::retrieval::*;
use serde_json::json;
#[test]
fn task_scope_precedes_budget_and_details_use_supplied_bytes() {
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let a = b"pub fn parse_alpha() { let marker = 1; }\n";
    let b = b"pub fn parse_beta() { let marker = 2; }\n";
    let fixtures = [("alpha.rs", a.as_slice()), ("beta.rs", b.as_slice())];
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[
            ("alpha.rs", ripwire_rs::Lang::Rust, a.as_slice()),
            ("beta.rs", ripwire_rs::Lang::Rust, b.as_slice()),
        ],
        &registry,
        &interner,
    );
    let result = call_for_task_with_options(
        &ingest,
        &interner,
        &[],
        "parse",
        ForTaskOptions {
            sources: Some(&fixtures),
            in_scope: Some("beta"),
            budget_tokens: Some(1),
            detail: Some(1),
            sections: Some("compose,lego"),
            ..Default::default()
        },
    );
    assert_eq!(result["count"], 1);
    assert_eq!(result["symbols"][0]["path"], "beta.rs");
    assert!(
        result["symbols"][0]["body"]
            .as_str()
            .unwrap()
            .contains("marker = 2")
    );
    assert_eq!(result["sections"], json!(["compose", "lego"]));
    let invalid = call_for_task_with_options(
        &ingest,
        &interner,
        &[],
        "parse",
        ForTaskOptions {
            sections: Some("unknown"),
            ..Default::default()
        },
    );
    assert!(
        invalid["error"]
            .as_str()
            .unwrap()
            .contains("unknown section")
    );
    let signatures = call_for_task_with_options(
        &ingest,
        &interner,
        &[],
        "parse",
        ForTaskOptions {
            sources: Some(&fixtures),
            detail: Some(9),
            auto_bodies: true,
            signatures_only: true,
            ..Default::default()
        },
    );
    assert_eq!(signatures["detail"], 0);
    assert!(
        signatures["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("body").is_none())
    );
    let request = BodyRequest {
        root: "/nonexistent-root",
        symbols: "parse_beta",
        path_hint: None,
        start_line: None,
        end_line: None,
        outline: false,
        compress: false,
    };
    let mut output = Vec::new();
    execute_body_query(&request, &ingest, &interner, &fixtures, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("ripwire-rs.expand/v1"));
    assert!(xml.contains("marker = 2"));
    let missing: [(&str, &[u8]); 0] = [];
    let error =
        execute_body_query(&request, &ingest, &interner, &missing, &mut Vec::new()).unwrap_err();
    assert!(error.to_string().contains("cannot read file"));
}

#[test]
fn grep_and_ast_use_only_injected_sources() {
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let bytes = b"pub fn injected_marker() {}\n";
    let fixtures = [
        ("virtual.rs", bytes.as_slice()),
        ("notes.txt", b"injected_marker\n".as_slice()),
    ];
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[("virtual.rs", ripwire_rs::Lang::Rust, bytes.as_slice())],
        &registry,
        &interner,
    );
    let corpus = vec!["virtual.rs".to_string(), "notes.txt".to_string()];
    let request = GrepRequest {
        corpus_paths: &corpus,
        path_filter: Some("virtual"),
        ..GrepRequest::new("/nonexistent-root", "injected_marker", &fixtures)
    };
    let grep = call_grep(&ingest, &interner, &request);
    assert_eq!(grep["hits"].as_array().unwrap().len(), 1);
    assert!(grep["unindexed"].as_array().unwrap().is_empty());
    let ast = call_ast_match(
        &fixtures,
        &ingest,
        &interner,
        "(function_item name: (identifier) @name)",
    )
    .unwrap();
    assert_eq!(ast["count"], 1);
    assert_eq!(ast["hits"][0]["text"], "injected_marker");
    assert!(call_ast_match(&fixtures, &ingest, &interner, "(not_a_rust_node) @name").is_err());
    let request = BodyRequest {
        root: "/nonexistent-root",
        symbols: "injected_marker",
        path_hint: None,
        start_line: None,
        end_line: None,
        outline: false,
        compress: false,
    };
    let mut output = Vec::new();
    execute_body_query(&request, &ingest, &interner, &fixtures, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("shown=\"1\""));
    assert!(xml.contains("injected_marker"));
    let request = BodyRequest {
        symbols: "missing",
        ..request
    };
    assert!(execute_body_query(&request, &ingest, &interner, &fixtures, &mut Vec::new()).is_err());
}

#[test]
fn body_report_preserves_duplicate_matches_and_relative_range_metadata() {
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let bytes = b"pub fn repeated() {\n    first();\n    second();\n}\n";
    let fixtures = [("one.rs", bytes.as_slice()), ("two.rs", bytes.as_slice())];
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[
            ("one.rs", ripwire_rs::Lang::Rust, bytes.as_slice()),
            ("two.rs", ripwire_rs::Lang::Rust, bytes.as_slice()),
        ],
        &registry,
        &interner,
    );
    let request = BodyRequest {
        root: "/nonexistent-root",
        symbols: "repeated",
        path_hint: None,
        start_line: None,
        end_line: None,
        outline: false,
        compress: false,
    };
    let mut output = Vec::new();
    execute_body_query(&request, &ingest, &interner, &fixtures, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("shown=\"2\""));
    assert!(xml.contains("p=\"one.rs\""));
    assert!(xml.contains("p=\"two.rs\""));
    let request = BodyRequest {
        path_hint: Some("two.rs"),
        start_line: Some(2),
        end_line: Some(2),
        ..request
    };
    let mut output = Vec::new();
    execute_body_query(&request, &ingest, &interner, &fixtures, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("shown=\"1\""));
    assert!(xml.contains("total_lines=\"4\" start_line=\"2\" end_line=\"2\""));
    assert!(xml.contains("l=\"1\" p=\"two.rs\""));
    assert!(xml.contains("first();"));
    assert!(!xml.contains("second();"));
    let request = BodyRequest {
        start_line: Some(0),
        ..request
    };
    assert!(
        execute_body_query(&request, &ingest, &interner, &fixtures, &mut Vec::new())
            .unwrap_err()
            .to_string()
            .contains("invalid body line range")
    );
    let request = BodyRequest {
        path_hint: Some("absent"),
        start_line: None,
        end_line: None,
        ..request
    };
    assert!(
        execute_body_query(&request, &ingest, &interner, &fixtures, &mut Vec::new())
            .unwrap_err()
            .to_string()
            .contains("not found")
    );
}

#[test]
fn task_detail_preserves_all_acquired_bodies_in_canonical_xml() {
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let a = b"pub fn parse_alpha() { let alpha_marker = 1; let text = \"]]>\"; }\n";
    let b = b"pub fn parse_beta() { let beta_marker = 2; }\n";
    let fixtures = [("alpha.rs", a.as_slice()), ("beta.rs", b.as_slice())];
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[
            ("alpha.rs", ripwire_rs::Lang::Rust, a.as_slice()),
            ("beta.rs", ripwire_rs::Lang::Rust, b.as_slice()),
        ],
        &registry,
        &interner,
    );
    for detail in [0, 1, 2] {
        let result = call_for_task_with_options(
            &ingest,
            &interner,
            &[],
            "parse",
            ForTaskOptions {
                root: "/nonexistent-root",
                sources: Some(&fixtures),
                detail: Some(detail),
                ..Default::default()
            },
        );
        assert_eq!(result["detail"], detail);
        let mut output = Vec::new();
        ripwire_rs::serialize::serialize_for_xml(&result, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();
        assert!(xml.contains("schema=\"ripwire-rs.for/v1\""));
        assert!(xml.contains("root=\"/nonexistent-root\""));
        assert!(xml.contains("task=\"parse\""));
        assert!(!xml.contains(" at=\""));
        assert!(xml.contains(&format!("bodies=\"{detail}\"")));
        assert!(xml.contains(&format!("<bodies shown=\"{detail}\" capped=\"0\">")));
        assert_eq!(xml.matches("<b t=").count(), detail);
        if detail == 2 {
            assert!(xml.contains("let alpha_marker = 1;"));
            assert!(xml.contains("let beta_marker = 2;"));
            assert!(xml.contains("]]]]><![CDATA[>"));
        }
    }
    let mut result = call_for_task_with_options(
        &ingest,
        &interner,
        &[],
        "parse",
        ForTaskOptions {
            sources: Some(&fixtures),
            detail: Some(2),
            ..Default::default()
        },
    );
    result.as_object_mut().unwrap().remove("root");
    result["at"] = json!("provided&stamp");
    let mut output = Vec::new();
    ripwire_rs::serialize::serialize_for_xml(&result, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("root=\".\""));
    assert!(xml.contains("at=\"provided&amp;stamp\""));
}

#[test]
fn connect_radius_metadata_matches_bounded_graph_search() {
    use ripwire_rs::query::graph::{DEFAULT_CONNECT_RADIUS, execute_connect_query_with_radius};
    let registry = ripwire_rs::LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let bytes = b"pub fn first() { second(); }\npub fn second() { third(); }\npub fn third() { last(); }\npub fn last() {}\n";
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[("chain.rs", ripwire_rs::Lang::Rust, bytes.as_slice())],
        &registry,
        &interner,
    );
    let graph = ripwire_rs::graph::Graph::build(&ingest, &interner);
    let mut output = Vec::new();
    execute_connect_query_with_radius(
        ".",
        "first,last",
        DEFAULT_CONNECT_RADIUS,
        &ingest,
        &graph,
        &interner,
        &mut output,
    )
    .unwrap();
    let xml = String::from_utf8(output).unwrap();
    assert!(xml.contains("root=\".\""));
    assert!(xml.contains("radius=\"2\""));
    assert!(!xml.contains("<e "));
    for radius in [0, 2, 3] {
        let mut output = Vec::new();
        execute_connect_query_with_radius(
            "/nonexistent-root",
            "first,last",
            radius,
            &ingest,
            &graph,
            &interner,
            &mut output,
        )
        .unwrap();
        let xml = String::from_utf8(output).unwrap();
        assert!(xml.contains("root=\"/nonexistent-root\""));
        assert!(xml.contains(&format!("radius=\"{radius}\"")));
        assert_eq!(xml.matches("<e ").count(), if radius == 3 { 3 } else { 0 });
    }
}
