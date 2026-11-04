use ripwire_rs::query::retrieval::{
    ForTaskOptions, call_exemplar_in_root, call_for_task_with_options, call_lego,
    collect_query_file_hits, compute_adaptive_cut, parse_for_sections,
};
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, LanguageRegistry, PageRankConfig, RefRole,
    Reference, SymKind, Symbol, ingest_in_memory_files, serialize_exemplar_xml, serialize_for_xml,
};
use serde_json::{Value, json};

#[test]
fn test_parse_for_sections_is_a_closed_set() {
    assert_eq!(parse_for_sections("lego").unwrap(), vec!["lego"]);
    assert_eq!(
        parse_for_sections("compose, lego").unwrap(),
        vec!["compose", "lego"]
    );
    assert!(
        parse_for_sections("lego,lego")
            .unwrap_err()
            .contains("named twice")
    );
    assert!(
        parse_for_sections("legos")
            .unwrap_err()
            .contains("unknown section")
    );
    assert!(parse_for_sections("").is_err());
}

#[test]
fn test_for_sections_expand_available_results() {
    let mut val = json!({
        "task": "parse flags",
        "symbols": [],
        "lego": [{
            "name": "VerbGroup",
            "path": "src/cli/args.rs",
            "line": 30,
            "implementors": [
                { "name": "TopNavArgs", "path": "src/cli/args.rs", "line": 122 },
                { "name": "TopDetailArgs", "path": "src/cli/args.rs", "line": 142 }
            ]
        }]
    });
    let render = |v: &Value| {
        let mut out = Vec::new();
        serialize_for_xml(v, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };
    let stub = render(&val);
    assert!(stub.contains(
        "<lego total=\"1\" shown=\"0\" capped=\"1\" next=\"--for=&apos;parse flags&apos; --sections=lego\"/>"
    ));
    assert!(!stub.contains("<iface "));

    val["sections"] = json!(["lego"]);
    let full = render(&val);
    assert!(full.contains("<lego><iface n=\"VerbGroup\" p=\"src/cli/args.rs:30\" implementors=\"2\"><impl n=\"TopNavArgs\" p=\"src/cli/args.rs:122\"/>"));
    assert!(!full.contains("<lego total="));

    val["lego"] = json!([]);
    assert!(!render(&val).contains("<lego"));
    let mut cval = json!({
        "task": "parse flags",
        "symbols": [],
        "compose": [{ "name": "graph", "owner": "NavContext", "rel": "uses" }],
        "hops": [{ "name": "run", "path": "src/main.rs", "line": 10, "total": 1, "calls": [{ "name": "init", "line": 2 }] }]
    });
    let cstub = render(&cval);
    assert!(cstub.contains("<compose total=\"1\" shown=\"0\" capped=\"1\""));
    assert!(cstub.contains("<hops shown=\"1\" capped=\"0\"><h l=\"10\" p=\"src/main.rs\" n=\"run\"><calls total=\"1\"><c n=\"init\" l=\"2\"/></calls></h></hops>"));
    cval["sections"] = json!(["compose"]);
    let cfull = render(&cval);
    assert!(
        cfull.contains(
            "<compose><field name=\"graph\" owner=\"NavContext\" rel=\"uses\"/></compose>"
        )
    );
}

#[test]
fn test_call_exemplar_roles() {
    let interner = Interner::new();
    let ingest = IngestResult::new();
    let sources = [(
        "src/lib.rs",
        &b"fn caller_fn() { callee_fn(); }\nfn callee_fn() {}\n"[..],
    )];
    for role in [
        "fn",
        "method",
        "class",
        "struct",
        "interface",
        "section",
        "other",
    ] {
        let res = call_exemplar_in_root(&sources, &ingest, &interner, &[], role, None);
        assert!(res.get("error").is_some());
    }
    let mut ing = IngestResult::new();
    ing.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/lib.rs"),
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 100,
    });
    let mut s0 = Symbol::new(
        0,
        interner.intern("caller_fn"),
        interner.intern("src/lib.rs::caller_fn"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    s0.end_byte = sources[0].1.len() as u32;
    ing.symbols.push(s0);
    ing.symbols.push(Symbol::new(
        1,
        interner.intern("callee_fn"),
        interner.intern("src/lib.rs::callee_fn"),
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    ));
    ing.references.push(Reference::new(
        0,
        interner.intern("callee_fn"),
        0,
        2,
        Lang::Rust,
        RefRole::Call,
    ));
    let g = Graph::build(&ing, &interner);
    let ex = call_exemplar_in_root(&sources, &ing, &interner, &[0.8, 0.2], "fn", Some(&g));
    assert_eq!(ex["calls"].as_array().unwrap().len(), 1);
    let mut xml_buf = Vec::new();
    serialize_exemplar_xml(".", &ex, &mut xml_buf).unwrap();
    assert!(
        String::from_utf8(xml_buf)
            .unwrap()
            .contains("<calls total=\"1\">")
    );
}

#[test]
fn test_call_for_task_with_detail() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let sym = Symbol::new(
        0,
        interner.intern("test_fn"),
        interner.intern("c_test_fn"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    ingest.symbols.push(sym);
    let res = call_for_task_with_options(
        &ingest,
        &interner,
        &[1.0],
        "test fn",
        ForTaskOptions {
            detail: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(res["count"], 1);
    assert_eq!(res["detail"], 1);

    for i in 1..6 {
        ingest.symbols.push(Symbol::new(
            i,
            interner.intern(&format!("session_secret_{i}")),
            interner.intern(&format!("c_{i}")),
            SymKind::Function,
            Lang::Rust,
            0,
            i + 1,
        ));
    }
    ingest.symbols.push(Symbol::new(
        6,
        interner.intern("lsp_manager"),
        interner.intern("c_lsp"),
        SymKind::Struct,
        Lang::Rust,
        0,
        10,
    ));
    let ranks = vec![1.0; ingest.symbols.len()];
    let div_res = call_for_task_with_options(
        &ingest,
        &interner,
        &ranks,
        "lsp manager session secret",
        ForTaskOptions::default(),
    );
    let top_names: Vec<&str> = div_res["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .take(5)
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(top_names.contains(&"lsp_manager"));
}

#[test]
fn test_compute_adaptive_cut() {
    assert_eq!(compute_adaptive_cut(&[10.0, 5.0], 5, 10), (2, 2, 0.0));
    let scores = [10.0, 9.0, 8.0, 2.0, 1.0];
    let (k, shown, gap) = compute_adaptive_cut(&scores, 1, 5);
    assert_eq!(k, 3);
    assert_eq!(shown, 3);
    assert!(gap >= 0.25);
    let no_drop = [10.0, 9.9, 9.8, 9.7];
    let (k_all, _, _) = compute_adaptive_cut(&no_drop, 1, 4);
    assert_eq!(k_all, 4);
}

#[test]
fn test_ast_match_fixture() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let code = b"pub struct Widget {}\npub fn build() {}";
    let ts_code = b"export const materializeUrl = async (rawPath: string): Promise<string> => {\n  const x = rawPath.trim();\n  return x;\n};\n";
    let mut ingest = ingest_in_memory_files(
        &[
            ("src/lib.rs", Lang::Rust, code),
            ("src/url.ts", Lang::TypeScript, ts_code),
        ],
        &reg,
        &interner,
    );
    let hits = collect_query_file_hits(
        &ingest.files[0],
        code,
        "(struct_item name: (type_identifier) @name)",
        &ingest,
        &interner,
    )
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["text"], "Widget");

    let arrow_sym = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "materializeUrl")
        .expect("materializeUrl");
    assert!(arrow_sym.has_body());
    assert!(
        !arrow_sym
            .extract_signature(ts_code, "materializeUrl")
            .contains("rawPath.trim()")
    );

    let next_id = ingest.symbols.len() as u32;
    ingest.symbols.push(
        Symbol::new(
            next_id,
            interner.intern("count"),
            interner.intern("src/lib.rs::Widget::count"),
            SymKind::Field,
            Lang::Rust,
            0,
            1,
        )
        .with_scope(interner.intern("Widget")),
    );
    ingest.references.push(Reference::new(
        0,
        interner.intern("build"),
        0,
        1,
        Lang::Rust,
        RefRole::Call,
    ));
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let sources = [("src/lib.rs", &code[..]), ("src/url.ts", &ts_code[..])];
    let res = call_for_task_with_options(
        &ingest,
        &interner,
        &pr.ranks,
        "Widget build",
        ForTaskOptions {
            root: ".",
            sources: Some(&sources),
            graph: Some(&graph),
            budget_tokens: None,
            detail: Some(1),
            adaptive: false,
            ..Default::default()
        },
    );
    assert!(!res["compose"].as_array().unwrap().is_empty());
    assert!(!res["hops"].as_array().unwrap().is_empty());
}

#[test]
fn camel_case_stem_scoring_and_lego_out_of_line_methods() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("packages/core/src/ast/loader.ts"),
        lang: Lang::TypeScript,
        line_count: 20,
        byte_size: 200,
    });
    ingest.files.push(FileInfo {
        id: 1,
        path: interner.intern("packages/core/src/controllers/tree-controller.ts"),
        lang: Lang::TypeScript,
        line_count: 20,
        byte_size: 200,
    });
    ingest.symbols.push(Symbol::new(
        0,
        interner.intern("discoverAndLoadASTNodes"),
        interner.intern("discoverAndLoadASTNodes"),
        SymKind::Function,
        Lang::TypeScript,
        0,
        1,
    ));
    ingest.symbols.push(Symbol::new(
        1,
        interner.intern("reloadTrees"),
        interner.intern("reloadTrees"),
        SymKind::Function,
        Lang::TypeScript,
        1,
        1,
    ));
    let res = call_for_task_with_options(
        &ingest,
        &interner,
        &[0.001, 0.001],
        "ast node cache tree discovery validation resolution",
        ForTaskOptions::default(),
    );
    let syms = res["symbols"].as_array().unwrap();
    assert_eq!(syms[0]["name"], "discoverAndLoadASTNodes");
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/infra/Diagnostics.h"),
        lang: Lang::Cpp,
        line_count: 50,
        byte_size: 500,
    });
    ingest.files.push(FileInfo {
        id: 1,
        path: interner.intern("src/infra/diagnostics.cpp"),
        lang: Lang::Cpp,
        line_count: 50,
        byte_size: 500,
    });
    let cls_spur = interner.intern("ConsoleLog");
    ingest.symbols.push(Symbol::new(
        0,
        cls_spur,
        interner.intern("src/infra/Diagnostics.h::ConsoleLog"),
        SymKind::Class,
        Lang::Cpp,
        0,
        10,
    ));
    ingest.symbols.push(
        Symbol::new(
            1,
            interner.intern("handlePanic"),
            interner.intern("src/infra/Diagnostics.h::ConsoleLog::handlePanic"),
            SymKind::Function,
            Lang::Cpp,
            0,
            15,
        )
        .with_scope(cls_spur),
    );
    ingest.symbols.push(
        Symbol::new(
            2,
            interner.intern("handleAssert"),
            interner.intern("src/infra/diagnostics.cpp::ConsoleLog::handleAssert"),
            SymKind::Method,
            Lang::Cpp,
            1,
            25,
        )
        .with_scope(cls_spur),
    );
    let lego = call_lego(&ingest, &interner, "ConsoleLog");
    let methods = lego["methods"].as_array().unwrap();
    assert_eq!(methods.len(), 2);
}

#[test]
fn rust_lego_lists_the_implemented_subject_and_default_contract() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let source = br#"
trait Processor {
    fn perform(&self);
    fn run(&self) { self.perform(); }
}
struct Worker;
impl Processor for Worker { fn perform(&self) {} }
trait Other { fn perform(&self); }
struct Unrelated;
impl Other for Unrelated { fn perform(&self) {} }
"#;
    let ingest = ingest_in_memory_files(
        &[("src/process.rs", Lang::Rust, source.as_slice())],
        &registry,
        &interner,
    );
    let result = call_lego(&ingest, &interner, "Processor");
    let names: Vec<_> = result["implementors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Worker"]);
    let mut methods: Vec<_> = result["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["name"].as_str().unwrap())
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, ["perform", "run"]);
}

#[test]
fn rust_lego_keeps_inline_trait_contracts_and_implementors_separate() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[(
            "src/lib.rs",
            Lang::Rust,
            br#"
mod alpha {
    pub trait Processor { fn perform(&self); }
    pub struct Worker;
    impl Processor for Worker { fn perform(&self) {} }
}
mod beta {
    pub trait Processor { fn perform(&self); fn extra(&self); }
    pub struct Worker;
    impl Processor for Worker { fn perform(&self) {} fn extra(&self) {} }
}
"#,
        )],
        &registry,
        &interner,
    );
    let result = call_lego(&ingest, &interner, "alpha::Processor");
    assert_eq!(result["count"], 1);
    assert_eq!(result["implementors"][0]["name"], "Worker");
    assert_eq!(result["methods"].as_array().unwrap().len(), 1);
    assert_eq!(result["methods"][0]["name"], "perform");
}

fn for_xml(code: &[u8], task: &str) -> (Value, String) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&[("src/lib.rs", Lang::Rust, code)], &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    let val = call_for_task_with_options(
        &ingest,
        &interner,
        &ranks,
        task,
        ForTaskOptions {
            graph: Some(&graph),
            ..Default::default()
        },
    );
    let mut out = Vec::new();
    serialize_for_xml(&val, &mut out).unwrap();
    (val, String::from_utf8(out).unwrap())
}

#[test]
fn name_exact_for_lists_direct_callers_with_the_total() {
    let (val, xml) = for_xml(
        b"fn target() {}\nfn first() { target(); }\nfn second() { target(); }\n",
        "target",
    );
    assert_eq!(val["callers"]["total"], 2);
    assert!(xml.contains("<callers of=\"target\" total=\"2\""), "{xml}");
    assert!(
        xml.contains("n=\"first\"") && xml.contains("n=\"second\""),
        "{xml}"
    );
}

#[test]
fn name_exact_for_omits_callers_when_nothing_calls_it_or_the_name_is_ambiguous() {
    let (val, xml) = for_xml(b"fn lonely() {}\n", "lonely");
    assert!(val["callers"].is_null(), "{xml}");
    assert!(!xml.contains("<callers"), "{xml}");

    let (val, xml) = for_xml(
        b"struct A;\nstruct B;\nimpl A { fn run(&self) {} }\nimpl B { fn run(&self) {} }\nfn go(a: A) { a.run(); }\n",
        "run",
    );
    assert!(val["callers"].is_null(), "{xml}");
}

/// The symbol names a conceptual `--for` returns for `task`, in rank order, over TypeScript files.
fn ranked_names(files: &[(&str, &str)], task: &str) -> Vec<String> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let typed: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|&(path, code)| (path, Lang::TypeScript, code.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&typed, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    let sources: Vec<(&str, &[u8])> = files
        .iter()
        .map(|&(path, code)| (path, code.as_bytes()))
        .collect();
    let val = call_for_task_with_options(
        &ingest,
        &interner,
        &ranks,
        task,
        ForTaskOptions {
            sources: Some(&sources.as_slice()),
            signatures_only: true,
            ..Default::default()
        },
    );
    val["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn query_evidence_from_comments_constants_and_whole_subtokens() {
    let code = "// Counts bank transfers: recurring billing can run off a bank account.\nexport function pickDefault(): number {\n  return 1;\n}\n\nexport function renderRecurringPage(): number {\n  return 2;\n}\n";
    let names = ranked_names(
        &[("src/billing.ts", code)],
        "recurring billing bank account",
    );
    assert_eq!(
        names.first().map(String::as_str),
        Some("pickDefault"),
        "{names:?}"
    );
    let code = "export const LIMITS = {\n  maxChangedFiles: 100,\n  maxFindingsPerAgent: 20,\n  reviewBudgetMs: 5000,\n};\n\nexport function describeThing(): number {\n  return 1;\n}\n";
    let names = ranked_names(
        &[("src/config.ts", code)],
        "hard caps on changed files findings review budget",
    );
    assert!(names.iter().any(|n| n == "LIMITS"), "{names:?}");
    let code = "export function loadAuthor(): number {\n  return 1;\n}\n\nexport function checkAuth(): number {\n  return 2;\n}\n";
    let names = ranked_names(&[("src/users.ts", code)], "check auth");
    assert_eq!(
        names.first().map(String::as_str),
        Some("checkAuth"),
        "{names:?}"
    );
    assert!(!names.iter().any(|n| n == "loadAuthor"), "{names:?}");
}

#[test]
fn a_long_body_does_not_outrank_a_short_one_by_mentioning_more_terms() {
    let filler: String = (0..120).map(|i| format!("  total += {i};\n")).collect();
    let long = format!(
        "export function runEverything(): number {{\n  let total = 0;\n  // queue the invoice and refund, then notify\n{filler}  return total;\n}}\n"
    );
    let short = "export function settleInvoice(): number {\n  return refundInvoice();\n}\nfunction refundInvoice(): number {\n  return 1;\n}\n";
    let names = ranked_names(
        &[("src/long.ts", long.as_str()), ("src/short.ts", short)],
        "settle invoice refund notify queue",
    );
    let pos = |name: &str| names.iter().position(|n| n == name);
    assert!(pos("settleInvoice") < pos("runEverything"), "{names:?}");
}

#[test]
fn a_ranked_signature_is_capped_so_a_wide_declaration_does_not_take_the_budget() {
    let params: String = (0..60).map(|i| format!("argument{i}: string, ")).collect();
    let code = format!("export function wide({params}): void {{\n  return;\n}}\n");
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[("src/table.ts", Lang::TypeScript, code.as_bytes())],
        &registry,
        &interner,
    );
    let sources: Vec<(&str, &[u8])> = vec![("src/table.ts", code.as_bytes())];
    let val = call_for_task_with_options(
        &ingest,
        &interner,
        &[1.0],
        "wide",
        ForTaskOptions {
            sources: Some(&sources.as_slice()),
            signatures_only: true,
            ..Default::default()
        },
    );
    let sig = val["symbols"][0]["sig"].as_str().unwrap();
    assert!(
        sig.len() <= 244 && sig.ends_with('…'),
        "{} bytes: {sig}",
        sig.len()
    );
}
