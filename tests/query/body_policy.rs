use ripwire_rs::query::retrieval::{ForTaskOptions, call_for_task_with_options};
use ripwire_rs::*;
use serde_json::Value;

const SESSION_TS: &[u8] = b"/**\n * Keeps identity stable.\n */\nexport function useFreshSession(): () => string {\n  return getToken;\n}\nexport function sessionHelper(): string {\n  return useFreshSession()();\n}\n";

const POLICY_A_TS: &[u8] = b"export function pick(a: string): string {\n  return \"text\";\n}\n";
const POLICY_B_TS: &[u8] = b"export function pick(a: number): number {\n  return a + 1;\n}\n";

fn run<const N: usize>(
    files: [(&str, &[u8]); N],
    task: &str,
    options: ForTaskOptions<'_>,
) -> Value {
    let inputs: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, source)| (*path, Lang::TypeScript, *source))
        .collect();
    let (ingest, graph, interner) = crate::support::index::index_files(&inputs);
    let ranks = graph.page_rank(&PageRankConfig::default());
    call_for_task_with_options(
        &ingest,
        &interner,
        &ranks.ranks,
        task,
        ForTaskOptions {
            sources: Some(&files),
            ..options
        },
    )
}

fn body_of(value: &Value, name: &str) -> Option<String> {
    value["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .and_then(|row| row["body"].as_str().map(str::to_string))
}

#[test]
fn exact_name_lookup_serves_the_definition_with_its_doc() {
    let value = run(
        [("src/module.ts", SESSION_TS)],
        "useFreshSession",
        ForTaskOptions::default(),
    );
    assert_eq!(value["bundle"], "auto");
    assert_eq!(value["detail"], 1);
    assert_eq!(value["reason"], Value::Null);
    let body = body_of(&value, "useFreshSession").expect("body");
    assert_eq!(
        body,
        "/**\n * Keeps identity stable.\n */\nexport function useFreshSession(): () => string {\n  return getToken;\n}"
    );
}

#[test]
fn withheld_bodies_name_their_reason_and_an_executable_follow_up() {
    let cases = [
        (
            ForTaskOptions {
                signatures_only: true,
                ..Default::default()
            },
            "signatures-only",
        ),
        (
            ForTaskOptions {
                budget_tokens: Some(1),
                ..Default::default()
            },
            "budget",
        ),
    ];
    for (options, reason) in cases {
        let value = run([("src/module.ts", SESSION_TS)], "useFreshSession", options);
        assert!(body_of(&value, "useFreshSession").is_none());
        assert_eq!(value["reason"], reason);
        assert_eq!(value["follow"], "--expand=src/module.ts:useFreshSession");
    }
}

#[test]
fn conceptual_route_stays_compact_and_discloses_the_expand_route() {
    let value = run(
        [("src/module.ts", SESSION_TS)],
        "fresh session token",
        ForTaskOptions::default(),
    );
    assert_eq!(value["bundle"], "compact");
    assert!(body_of(&value, "sessionHelper").is_none());
    assert_eq!(value["reason"], "compact-route");
    assert!(
        value["follow"]
            .as_str()
            .unwrap()
            .starts_with("--expand=src/module.ts:")
    );
}

#[test]
fn explicit_detail_still_serves_the_top_bodies() {
    let value = run(
        [("src/module.ts", SESSION_TS)],
        "fresh session token",
        ForTaskOptions {
            detail: Some(1),
            ..Default::default()
        },
    );
    let rows = value["symbols"].as_array().unwrap();
    assert!(rows[0]["body"].is_string());
    assert!(rows[1..].iter().all(|row| row["body"].is_null()));
    assert_eq!(value["reason"], Value::Null);
}

#[test]
fn ambiguous_exact_names_each_serve_their_own_definition() {
    let value = run(
        [("src/a.ts", POLICY_A_TS), ("src/b.ts", POLICY_B_TS)],
        "pick",
        ForTaskOptions::default(),
    );
    let rows = value["symbols"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let body = row["body"].as_str().expect("body");
        match row["path"].as_str().unwrap() {
            "src/a.ts" => {
                assert!(body.contains("return \"text\";"), "{body}");
                assert!(!body.contains("a + 1"), "{body}");
            }
            "src/b.ts" => {
                assert!(body.contains("return a + 1;"), "{body}");
                assert!(!body.contains("\"text\""), "{body}");
            }
            other => panic!("unexpected path {other}"),
        }
    }
}
