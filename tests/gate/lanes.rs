use ripwire_rs::gate::lanes::{
    LaneCorpus, compute_plan_lanes, compute_plan_lanes_with_brief, measure_partition_overlap,
    plan_task_partition,
};
use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, PageRankConfig, PageRankResult,
    ingest_in_memory_files,
};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

#[test]
fn test_plan_lanes_partition_and_brief() {
    let empty_ingest = IngestResult::new();
    let empty_interner = Interner::new();
    let empty_graph = Graph::build(&empty_ingest, &empty_interner);
    let empty_pr = empty_graph.page_rank(&PageRankConfig::default());
    let empty_val = compute_plan_lanes(
        LaneCorpus {
            root: ".",
            ingest: &empty_ingest,
            graph: &empty_graph,
            pr: &empty_pr,
            interner: &empty_interner,
            churn: Err("no git history"),
        },
        2,
        "refactor parser",
        Default::default(),
    );
    assert_eq!(empty_val["verb"], "plan-lanes");
    assert_eq!(empty_val["requested"], 2);

    let (ingest, interner, graph, pr) = lane_fixture();
    let val = compute_plan_lanes(
        LaneCorpus {
            root: ".",
            ingest: &ingest,
            graph: &graph,
            pr: &pr,
            interner: &interner,
            churn: Err("no git history"),
        },
        2,
        "login verify charge",
        Default::default(),
    );
    assert_eq!(val["source"], "partition");
    assert_eq!(val["carve"]["strategy"], "leiden-community");
    assert!(val["lanes"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(
        val["landing_order"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    );

    let brief_lines = vec![
        "login_user verify_token".to_string(),
        "verify_token charge_card".to_string(),
    ];
    let brief_val = brief_plan(&ingest, &interner, &graph, &pr, None, &brief_lines);
    assert_eq!(brief_val["source"], "brief");
    assert_eq!(brief_val["lane_count"], 2);
    assert_eq!(brief_val["pairs"].as_array().map(Vec::len), Some(1));

    let s1: BTreeSet<u32> = [0, 1, 2].into_iter().collect();
    let s2: BTreeSet<u32> = [2, 3].into_iter().collect();
    let core: BTreeSet<u32> = [0, 4].into_iter().collect();
    let ov = measure_partition_overlap(&[s1, s2], &core);
    assert_eq!(ov.shared_count, 1);
    assert_eq!(ov.union_count, 4);
    assert!((ov.core_leak - 0.5).abs() < 1e-6);
}

#[test]
fn test_plan_lanes_partition_plan_has_core_and_groups() {
    let (ingest, interner, graph, pr) = lane_fixture();
    let plan = plan_task_partition(
        "login verify charge",
        4,
        &ingest,
        &graph,
        &pr.ranks,
        &interner,
    );
    assert!(!plan.core_ids.is_empty());
    assert!(!plan.groups.is_empty());
}

#[test]
fn test_plan_lanes_churn_and_hotspot_rank_follow_history() {
    let (ingest, interner, graph, pr) = lane_fixture();
    let brief = ["login_user verify_token".to_string()];
    let churn: HashMap<String, u32> = [
        ("src/auth.rs".to_string(), 5),
        ("src/billing.rs".to_string(), 1),
    ]
    .into_iter()
    .collect();
    let hot = brief_plan(&ingest, &interner, &graph, &pr, Some(&churn), &brief);
    let auth = file_row(&hot, "src/auth.rs");
    assert_eq!(auth["churn"], 5);
    assert_eq!(auth["hotspot_rank"], 1);
    assert!(hot.get("churn_unavailable").is_none());

    let cold: HashMap<String, u32> = HashMap::new();
    let unseen = brief_plan(&ingest, &interner, &graph, &pr, Some(&cold), &brief);
    let auth = file_row(&unseen, "src/auth.rs");
    assert_eq!(auth["churn"], 0);
    assert!(auth["hotspot_rank"].is_null());

    let no_git = brief_plan(&ingest, &interner, &graph, &pr, None, &brief);
    let auth = file_row(&no_git, "src/auth.rs");
    assert!(auth.get("churn").is_none());
    assert!(auth.get("hotspot_rank").is_none());
    assert_eq!(no_git["churn_unavailable"], "no git history");
}

#[test]
fn test_plan_lanes_conflicts_and_touches_come_from_pairs() {
    let (ingest, interner, graph, pr) = lane_fixture();
    let overlapping = ["login_user".to_string(), "login_user".to_string()];
    let shared = brief_plan(&ingest, &interner, &graph, &pr, None, &overlapping);
    let pair_conflicts = shared["pairs"][0]["conflict_count"].as_u64().unwrap();
    assert!(pair_conflicts > 0);
    for lane in shared["lanes"].as_array().unwrap() {
        assert_eq!(
            lane["execution"]["signals"]["conflicts"].as_u64(),
            Some(pair_conflicts)
        );
    }

    let disjoint = ["login_user".to_string(), "charge_card".to_string()];
    let apart = brief_plan(&ingest, &interner, &graph, &pr, None, &disjoint);
    assert_eq!(apart["pairs"][0]["conflict_count"], 0);
    for lane in apart["lanes"].as_array().unwrap() {
        assert_eq!(lane["execution"]["signals"]["conflicts"], 0);
    }
}

#[test]
fn test_plan_lanes_ambiguity_counts_match_the_graph() {
    let (ingest, interner, graph, pr) = lane_fixture();
    assert!(
        graph.total_ambiguous() > 0,
        "fixture must contain an ambiguous call"
    );
    let val = brief_plan(
        &ingest,
        &interner,
        &graph,
        &pr,
        None,
        &["run_ambiguous".to_string()],
    );
    let lane = &val["lanes"][0];
    let amb_rows: u64 = lane["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["amb"].as_u64().unwrap())
        .sum();
    assert_eq!(amb_rows, 1);
    assert_eq!(
        lane["execution"]["signals"]["ambiguous_calls"].as_u64(),
        Some(amb_rows)
    );

    let quiet = brief_plan(
        &ingest,
        &interner,
        &graph,
        &pr,
        None,
        &["login_user".to_string()],
    );
    assert_eq!(
        quiet["lanes"][0]["execution"]["signals"]["ambiguous_calls"],
        0
    );
}

#[test]
fn test_plan_lanes_identity_fields_only_when_addressable() {
    let (ingest, interner, graph, pr) = lane_fixture();
    let val = brief_plan(
        &ingest,
        &interner,
        &graph,
        &pr,
        None,
        &["login_user".to_string()],
    );
    let sym = &val["lanes"][0]["symbols"][0];
    assert_eq!(sym["id_addressable"], false);
    assert!(sym.get("id_collides_with").is_none());
    assert_eq!(sym["overloads"], 1);
}

#[test]
fn test_plan_lanes_brief_module_span_counts_communities() {
    let (ingest, interner, graph, pr) = lane_fixture();
    let val = brief_plan(
        &ingest,
        &interner,
        &graph,
        &pr,
        None,
        &["login_user".to_string(), "charge_card".to_string()],
    );
    for lane in val["lanes"].as_array().unwrap() {
        assert_eq!(lane["execution"]["signals"]["module_span"], 1);
    }
}

/// Auth, billing, a test, and an ambiguous caller: `helper` is defined in two files, so the call in
/// `run_ambiguous` cannot bind to one definition.
fn lane_fixture() -> (IngestResult, Interner, Graph, PageRankResult) {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files: [(&str, Lang, &[u8]); 6] = [
        (
            "src/auth.rs",
            Lang::Rust,
            b"pub fn login_user(ok: bool) { if ok { verify_token(); } }\npub fn verify_token() {}\npub struct AuthConfig { pub ttl: u32 }\npub struct TokenPair { pub id: u32 }\n",
        ),
        (
            "src/billing.rs",
            Lang::Rust,
            b"pub fn charge_card() { record_ledger(); }\npub fn record_ledger() {}\npub fn refund_card() { charge_card(); }\n",
        ),
        (
            "tests/auth_test.rs",
            Lang::Rust,
            b"fn test_login() { login_user(); charge_card(); }\n",
        ),
        ("src/left.rs", Lang::Rust, b"pub fn helper() {}\n"),
        ("src/right.rs", Lang::Rust, b"pub fn helper() {}\n"),
        (
            "src/ambiguous.rs",
            Lang::Rust,
            b"pub fn run_ambiguous() { helper(); }\n",
        ),
    ];
    let ingest = ingest_in_memory_files(&files, &reg, &interner);
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    (ingest, interner, graph, pr)
}

fn brief_plan(
    ingest: &IngestResult,
    interner: &Interner,
    graph: &Graph,
    pr: &PageRankResult,
    churn: Option<&HashMap<String, u32>>,
    brief: &[String],
) -> Value {
    compute_plan_lanes_with_brief(
        LaneCorpus {
            root: ".",
            ingest,
            graph,
            pr,
            interner,
            churn: churn.ok_or("no git history"),
        },
        brief,
        Default::default(),
    )
}

fn file_row<'a>(val: &'a Value, path: &str) -> &'a Value {
    val["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|l| l["claims"]["files"].as_array().unwrap().iter())
        .find(|f| f["p"] == path)
        .unwrap_or_else(|| panic!("no claim row for {path}"))
}

#[test]
fn task_partition_surface_is_capped_by_symbol_count_not_token_budget() {
    let source: String = (0..24)
        .map(|i| format!("pub fn worker_stage_{i}() {{}}\n"))
        .collect();
    let files: [(&str, Lang, &[u8]); 1] = [("src/worker.rs", Lang::Rust, source.as_bytes())];
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&files, &reg, &interner);
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let plan = plan_task_partition("worker stage", 2, &ingest, &graph, &pr.ranks, &interner);
    assert!(plan.surface_count >= 20, "surface {}", plan.surface_count);
    assert!(!plan.groups.is_empty());
}
