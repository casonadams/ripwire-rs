use crate::uses_support::uses_json;
use ripwire_rs::commands::around::{AroundConfig, execute_around_query};
use ripwire_rs::commands::common::safety_queries::execute_safe_delete_query;
use ripwire_rs::gate::contract::{ContractStatus, SignatureContract, is_symbol_name_public};
use ripwire_rs::gate::edit_check::check_symbol_contract;
use ripwire_rs::ingest::{Capture, classify_capture, parse_file_content};
use ripwire_rs::query::graph::{
    DEFAULT_CONNECT_RADIUS, call_path_between, execute_connect_query_with_radius,
};
use ripwire_rs::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use ripwire_rs::query::retrieval::{ForTaskOptions, call_for_task_with_options, call_lego};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    Graph, Interner, Lang, LanguageRegistry, PageRankConfig, SliceContext, SliceFlowDir,
    SliceRequest, SymKind, ingest_in_memory_files, navigate_1hop, query_slice, serialize_lego_xml,
    serialize_uses_xml,
};

#[test]
fn test_classify_captures() {
    assert!(matches!(classify_capture("name"), Some(Capture::Name)));
    assert!(matches!(
        classify_capture("import.path"),
        Some(Capture::ImportPath)
    ));
    assert!(classify_capture("other").is_none());
    assert!(classify_capture("definition.unknown_tag").is_none());
    assert!(classify_capture("reference.unknown_tag").is_none());
    for tail in [
        "function",
        "cjsexport",
        "method",
        "protomethod",
        "class",
        "object",
        "specialization",
        "struct",
        "interface",
        "type",
        "var",
        "constant",
        "attribute",
        "property",
        "enummember",
        "macro",
        "testmacroblock",
        "module",
        "section",
        "yamlkey",
        "field",
    ] {
        assert!(classify_capture(&format!("definition.{tail}")).is_some());
    }
    for tail in [
        "read",
        "attribute",
        "write",
        "import",
        "extends",
        "implementation",
        "class",
        "macro",
        "type",
        "value",
        "through",
        "call",
        "bare",
        "operator",
    ] {
        assert!(classify_capture(&format!("reference.{tail}")).is_some());
    }
}

#[test]
fn test_all_scm_capture_vocabulary_and_patterns_valid() {
    let reg = LanguageRegistry::new().unwrap();
    for lang in ripwire_rs::languages::ALL_LANGS {
        let (_, query) = reg.get(lang).expect("registered language");
        for cap_name in query.capture_names() {
            assert!(
                classify_capture(cap_name).is_some(),
                "Lang {:?} tags.scm contains unrecognized capture '@{}'",
                lang,
                cap_name
            );
        }
    }
}
#[test]
fn test_specialized_language_handlers() {
    let reg = LanguageRegistry::new().unwrap();

    let astro_code =
        b"---\nimport Nav from './Nav.astro';\nfunction setup() { run(); }\n---\n<Nav />";
    let (ts_lang, query) = reg.get(Lang::TypeScript).unwrap();
    let astro_res = parse_file_content(
        astro_code,
        ts_lang,
        query,
        Lang::TypeScript,
        "src/Component.astro",
    );
    assert!(
        !astro_res.defs.is_empty(),
        "Astro should extract frontmatter defs"
    );
    assert_eq!(astro_res.defs[0].name, "setup");
    assert!(
        !astro_res.refs.is_empty(),
        "Astro should extract frontmatter refs"
    );
    assert!(astro_res.refs.iter().any(|r| r.callee_name == "run"));
}

#[test]
fn test_dart_end_to_end_parity_contracts() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let shapes_code = br#"
sealed class TaskOutcome {}
final class TaskCompleted extends TaskOutcome {}
final class TaskSkipped extends TaskOutcome {}
final class TaskMissing extends TaskOutcome {}
final class TaskAmbiguous extends TaskOutcome {}
final class TaskConflicted extends TaskOutcome {}
"#;
    let ctrl_code = br#"
class ApiController {
  void executeAction() {
    recordAudit();
  }
}
void recordAudit() {}
"#;
    let svc_code = br#"
import 'package:core/core.dart';

String normalizeInput(String raw) {
  return raw;
}

void dispatchBatch(String item) {
  final normalized = normalizeInput(item);
  sendPayload(normalized);
}

void sendPayload(String payload) {}

class WorkerService {
  Future<void> executeAction(String taskId, String mode) async {
    if (taskId.isEmpty) {
      return;
    }
    dispatchBatch(taskId);
  }
}
"#;
    let json_code = b"{\"name\": \"test\"}";
    let ingest = ingest_in_memory_files(
        &[
            ("src/domain/outcomes.dart", Lang::Dart, shapes_code),
            ("src/api/controller.dart", Lang::Dart, ctrl_code),
            ("src/service/worker.dart", Lang::Dart, svc_code),
            ("src/views/manifest.json", Lang::Json, json_code),
        ],
        &reg,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());

    let lego = call_lego(&ingest, &interner, "TaskOutcome");
    assert_eq!(lego["count"], 5);
    let mut lego_xml = Vec::new();
    serialize_lego_xml("lib", &lego, &mut lego_xml).unwrap();
    let lego_str = String::from_utf8(lego_xml).unwrap();
    assert!(lego_str.contains("implementors=\"5\""));
    assert!(lego_str.contains("p=\"src/domain/outcomes.dart:"));

    let uses = uses_json(&ingest, &interner, "TaskOutcome");
    assert_eq!(uses["count"], 5);

    let path_val = call_path_between(
        &ingest,
        &graph,
        &interner,
        "executeAction",
        "normalizeInput",
    );
    assert_eq!(path_val["reachable"], true);
    assert_eq!(path_val["hops"], 2);
    assert_eq!(path_val["from_defs"], 2);
    assert_eq!(path_val["to_defs"], 1);

    let has_div_cls = ingest
        .symbols
        .iter()
        .any(|s| s.kind == SymKind::Class && interner.resolve(&s.name) == "div");
    assert!(!has_div_cls);

    let slice_xml = query_slice(
        SliceRequest {
            root: ":memory:",
            target: "dispatchBatch:normalized",
            flow: SliceFlowDir::Fwd,
            max_depth: 8,
            at: None,
        },
        SliceContext {
            ingest: &ingest,
            registry: &reg,
            interner: &interner,
        },
        &[("src/service/worker.dart", svc_code.as_slice())],
    )
    .unwrap();
    assert!(slice_xml.contains("v=\"normalized\""));
    assert!(!slice_xml.contains("steps=\"0\""));

    let req_sym = ingest
        .symbols
        .iter()
        .find(|s| {
            interner.resolve(&s.name) == "executeAction"
                && s.scope.map(|sc| interner.resolve(&sc)) == Some("WorkerService")
        })
        .expect("WorkerService::executeAction");
    assert!(req_sym.sig_end_byte > req_sym.sig_start_byte);
    assert!(req_sym.sig_end_byte < req_sym.end_byte);
    let sig_slice = &svc_code[req_sym.sig_start_byte as usize..req_sym.sig_end_byte as usize];
    let sig_str = String::from_utf8_lossy(sig_slice);
    assert!(sig_str.contains("Future<void> executeAction(String taskId, String mode)"));

    let sources = [
        ("src/domain/outcomes.dart", &shapes_code[..]),
        ("src/api/controller.dart", &ctrl_code[..]),
        ("src/service/worker.dart", &svc_code[..]),
        ("src/views/manifest.json", &json_code[..]),
    ];
    let for_val = call_for_task_with_options(
        &ingest,
        &interner,
        &pr.ranks,
        "Worker executeAction",
        ForTaskOptions {
            root: ".",
            sources: Some(&sources),
            graph: Some(&graph),
            ..Default::default()
        },
    );
    assert!(for_val["count"].as_u64().unwrap_or(0) > 0);

    let mut conn_buf = Vec::new();
    execute_connect_query_with_radius(
        ".",
        "executeAction,normalizeInput",
        DEFAULT_CONNECT_RADIUS,
        &ingest,
        &graph,
        &interner,
        &mut conn_buf,
    )
    .unwrap();
    assert!(
        String::from_utf8(conn_buf)
            .unwrap()
            .contains("<edge from=\"executeAction\" to=\"normalizeInput\"/>")
    );

    let mut sd_buf = Vec::new();
    let facts = ripwire_rs::query::safety::SafetyContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    execute_safe_delete_query(
        &facts,
        "normalizeInput",
        ripwire_rs::serialize::PageRequest::default(),
        &mut sd_buf,
    )
    .unwrap();
    assert!(
        String::from_utf8(sd_buf)
            .unwrap()
            .contains("p=\"src/service/worker.dart:")
    );

    let mut around_buf = Vec::new();
    execute_around_query(
        ".",
        "dispatchBatch",
        AroundConfig {
            depth: 1,
            fanout: 32,
        },
        &ingest,
        &graph,
        &interner,
        &mut around_buf,
    )
    .unwrap();
    let around_str = String::from_utf8(around_buf).unwrap();
    assert!(around_str.contains("n=\"executeAction\""));
    assert!(around_str.contains("n=\"normalizeInput\""));

    let sub_sym = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "dispatchBatch")
        .unwrap();
    let head_contract = SignatureContract {
        name: "dispatchBatch".to_string(),
        params: sub_sym.params,
        is_public: is_symbol_name_public("dispatchBatch", Lang::Dart),
    };
    let ec_rep =
        check_symbol_contract(sub_sym.id, Some(&head_contract), &ingest, &graph, &interner)
            .unwrap();
    assert_eq!(ec_rep.status, ContractStatus::Unchanged);
}

#[test]
fn test_embedded_sql_table_lineage_across_languages() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();

    let sql_migration = br#"
CREATE TABLE mobile.user_records (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL
);
CREATE TABLE audit_events (
    id SERIAL PRIMARY KEY
);
"#;
    let dart_repo = br#"
class AccountRepository {
  AccountRepository(this.a, {required this.b});
  Future<void> createRecord(dynamic db) async {
    await db.query("INSERT INTO mobile.user_records (id, status) VALUES ($1, $2)");
  }
  Future<void> loadActive(dynamic db) async {
    await db.query("SELECT r.id FROM \"mobile\".\"user_records\" r JOIN audit_events a ON a.id = r.id WHERE r.status = 'active'");
  }
  Future<void> purgeOld(dynamic db, {int limit = 10, bool force = false}) async {
    await db.query("DELETE FROM ONLY mobile.user_records WHERE status = 'archived'");
    await db.query("UPDATE audit_events SET id = 0");
  }
  Future<void> runMaintenance(dynamic db) async {
    await purgeOld(db, limit: 50, force: true);
  }
}
"#;
    let caller_service = br#"
class MobileSwapService {
  void executeSwap() {
    AccountRepository(1, b: 2);
  }
}
"#;
    let ingest = ingest_in_memory_files(
        &[
            ("migrations/001_init.sql", Lang::Sql, sql_migration),
            ("lib/db/account_repo.dart", Lang::Dart, dart_repo),
            ("lib/mobile/swap_service.dart", Lang::Dart, caller_service),
        ],
        &reg,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let ranks = vec![1.0; ingest.symbols.len()];

    let uses_val = uses_json(&ingest, &interner, "user_records");
    assert_eq!(uses_val["count"], 3);
    let sites = uses_val["uses"].as_array().unwrap();
    assert!(
        sites
            .iter()
            .any(|s| s["role"] == "write" && s["enclosing"] == "createRecord")
    );
    assert!(
        sites
            .iter()
            .any(|s| s["role"] == "read" && s["enclosing"] == "loadActive")
    );
    assert!(
        sites
            .iter()
            .any(|s| s["role"] == "write" && s["enclosing"] == "purgeOld")
    );

    let table_sym = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "user_records")
        .expect("SQL table user_records");
    let nav = navigate_1hop(table_sym.id, &ingest, &graph, &ranks, &interner).unwrap();
    assert_eq!(nav.callers.len(), 3);

    let purge_sym = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "purgeOld")
        .expect("purgeOld method");
    assert_eq!(purge_sym.params, 3);
    let purge_nav = navigate_1hop(purge_sym.id, &ingest, &graph, &ranks, &interner).unwrap();
    assert_eq!(purge_nav.callers.len(), 1);
    assert_eq!(purge_nav.callers[0].name, "runMaintenance");

    let mut callers_buf = Vec::new();
    let ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &ranks,
        interner: &interner,
        page: PageRequest::default(),
        max_depth: None,
        pr_iters: None,
    };
    execute_nav_symbol_query_in_root(
        ".",
        "AccountRepository",
        NavMode::Callers,
        &ctx,
        &mut callers_buf,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&callers_buf).contains("n=\"executeSwap\""));
}

#[test]
fn test_embedded_sql_binds_tables_only_and_skips_ctes() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            ("a.py", Lang::Python, b"users = []\ntotals = {}\n"),
            (
                "b.py",
                Lang::Python,
                b"def load():\n    return \"SELECT id FROM users WHERE id = 1\"\ndef tally():\n    return \"WITH totals AS (SELECT 1 AS n) SELECT n FROM totals\"\n",
            ),
            ("schema.sql", Lang::Sql, b"CREATE TABLE users (id INT);\n"),
        ],
        &reg,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let ranks = vec![1.0; ingest.symbols.len()];
    let callers_of = |name: &str, lang: Lang| -> Vec<String> {
        let sym = ingest
            .symbols
            .iter()
            .find(|s| interner.resolve(&s.name) == name && s.lang == lang)
            .unwrap_or_else(|| panic!("{name} in {lang:?}"));
        let nav = navigate_1hop(sym.id, &ingest, &graph, &ranks, &interner).unwrap();
        nav.callers.iter().map(|c| c.name.clone()).collect()
    };
    assert_eq!(callers_of("users", Lang::Sql), ["load"]);
    assert!(callers_of("users", Lang::Python).is_empty());
    assert!(callers_of("totals", Lang::Python).is_empty());
}

#[test]
fn test_definition_lines_use_each_language_tags_query() {
    let cases: &[(&str, &[u8], &[u32])] = &[
        (
            "lib/widget.dart",
            b"int parseValue(String s) {\n  return int.parse(s);\n}\nvoid use() { parseValue('1'); }\n",
            &[1],
        ),
        ("app.py", b"x = parseValue\n\ndef parseValue():\n    pass\n", &[3]),
        ("notes.md", b"See parseValue.\n# parseValue\n", &[2]),
        ("blob.bin", b"parseValue", &[]),
    ];
    for &(path, src, want) in cases {
        assert_eq!(
            ripwire_rs::ingest::definition_lines(path, src, "parseValue"),
            want,
            "{path}"
        );
    }
}

#[test]
fn test_cpp_type_uses_and_ifdef_macro_dedup() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let cpp_src = br#"
#ifdef DEBUG
#define RW_ASSUME_(x) check(x)
#else
#define RW_ASSUME_(x) ((void)0)
#endif
struct RankedGraph { int nodes; };
RankedGraph rankGraph(const RankedGraph& g) { return g; }
"#;
    let ingest = ingest_in_memory_files(&[("src/graph.h", Lang::Cpp, cpp_src)], &reg, &interner);
    let macro_count = ingest
        .symbols
        .iter()
        .filter(|s| interner.resolve(&s.name) == "RW_ASSUME_")
        .count();
    assert_eq!(macro_count, 1);

    let mut uses_xml = Vec::new();
    serialize_uses_xml("RankedGraph", &ingest, &interner, &mut uses_xml).unwrap();
    let uses_str = String::from_utf8(uses_xml).unwrap();
    assert!(uses_str.contains("role=\"type\""));
    assert!(uses_str.contains("count=\"2\""));
}
