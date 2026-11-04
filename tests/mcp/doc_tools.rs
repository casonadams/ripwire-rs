use ripwire_rs::git::history::CommitInfo;
use ripwire_rs::languages::c::flags::{
    conditional_uses, extract_cmake_option, extract_settings_gate,
};
use ripwire_rs::languages::common::flags::FlagFact;
use ripwire_rs::query::doc_tools::{
    WhereisFacts, call_owners, collect_file_wanted_tokens, filter_present_in_sources_drift,
    mentions_from_sources, scan_doc_text_for_drift, whereis_from_sources_with_facts,
};
use ripwire_rs::{
    FileInfo, IngestResult, Interner, Lang, SymKind, Symbol, serialize_doc_drift_xml,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

#[test]
fn test_mentions_empty() {
    let ingest = IngestResult::new();
    let interner = Interner::new();
    let sources: [(&str, &[u8]); 0] = [];
    let res = mentions_from_sources(&sources, &ingest, &interner, "nonexistent");
    assert_eq!(res["total_mentions"], 0);
}

#[test]
fn test_owners_attribution() {
    let commits = vec![
        CommitInfo {
            id: "c1".to_string(),
            timestamp: 1000,
            author: "alice@example.com".to_string(),
            changed_files: vec!["src/lib.rs".to_string()],
        },
        CommitInfo {
            id: "c2".to_string(),
            timestamp: 2000,
            author: "bob@example.com".to_string(),
            changed_files: vec!["src/lib.rs".to_string()],
        },
    ];
    let res = call_owners(Some(&commits), None, None, None);
    assert_eq!(res["files_analyzed"], 1);
    let files = res["files"].as_array().unwrap();
    assert_eq!(files[0]["authors"], 2);
}

#[test]
fn test_env_identifier_extraction_table() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("a.c", "getenv(\"BAR\")", &["BAR"]),
        ("a.cpp", "std::getenv(\"CPP1\")", &["CPP1"]),
        ("a.c", "getenv( \"INSP\" )", &["INSP"]),
        ("a.py", "x = getenv (\"GSP\")", &["GSP"]),
        ("a.py", "os.getenv(\"BAZ\")", &["BAZ"]),
        ("a.py", "os.environ.get(\"QUX\", \"d\")", &["QUX"]),
        ("a.py", "os.environ.get (\"EGS\")", &["EGS"]),
        (
            "a.py",
            "dsn = os.environ[\"DATABASE_URL\"]",
            &["DATABASE_URL"],
        ),
        ("a.py", "f = os.environ[ \"SPC\" ]", &["SPC"]),
        ("a.py", "e = environ[\"BARE\"]", &["BARE"]),
        ("a.py", "x = os.environ[\"lower_case\"]", &["lower_case"]),
        (
            "a.py",
            "h = os.environ[\"X1\"] + os.environ[\"X2\"]",
            &["X1", "X2"],
        ),
        (
            "a.py",
            "a = os.environ[\"DUP\"] + os.environ[\"DUP\"]",
            &["DUP"],
        ),
        (
            "a.py",
            "_os.environ[\"COMMIT_SHA\"] = \"cafebabe\"",
            &["COMMIT_SHA"],
        ),
        (
            "a.py",
            "c = \"it's\" and os.getenv(\"AFTERAPOS\")",
            &["AFTERAPOS"],
        ),
        (
            "a.ts",
            "  * docs (injected via `process.env.DOCS_EMBED`)",
            &[],
        ),
        ("a.ts", "/* process.env.BLOCK */", &[]),
        (
            "a.ts",
            "define: { \"process.env.APP_BUNDLED\": JSON.stringify(\"true\") },",
            &[],
        ),
        ("a.js", "process.env.VAR_123", &["VAR_123"]),
        ("a.js", "process.env.A + process.env.B", &["A", "B"]),
        ("a.js", "const a = process.env[\"PDQ\"];", &["PDQ"]),
        ("a.js", "const b = process.env['PSQ'];", &["PSQ"]),
        ("a.js", "const b = process.env[`PBT`];", &["PBT"]),
        ("a.js", "const a = process.env[ \"PSPC\" ];", &["PSPC"]),
        ("a.rs", "let a = std::env::var(\"FOO\");", &[]),
        ("a.py", "b = os.environ['SQ']", &[]),
        ("a.py", "c = os.getenv('GSQ')", &[]),
        ("a.py", "d = os.environ.get('EGSQ')", &[]),
        ("a.py", "i = os.environ  [\"SP2\"]", &[]),
        ("a.py", "c = myenviron[\"MYENV\"]", &[]),
        ("a.py", "g = env[\"PLAINENV\"]", &[]),
        ("a.py", "b = mygetenv(\"MYG\")", &[]),
        ("a.py", "e = os.environ[KEY]", &[]),
        ("a.py", "f = os.environ[\"A-B\"]", &[]),
        ("a.py", "i = os.environ[\"9START\"]", &[]),
        ("a.py", "j = os.environ[\"\"]", &[]),
        ("a.py", "# os.getenv(\"CMT\")", &[]),
        ("a.py", "c = 'os.environ[\"INSQ\"]'", &[]),
        (
            "a.sh",
            "python3 -c 'import os; print(os.environ[\"SHPY\"])'",
            &[],
        ),
        ("a.sh", "echo ${SHV} ${SHD:-x} $SHP", &[]),
        ("a.js", "const b = \"process.env.INSTR\";", &[]),
        ("a.js", "// process.env.CMT", &[]),
        ("a.js", "const c = process.env[name];", &[]),
        ("a.js", "const b = process.env [ \"PSP\" ];", &[]),
        ("a.js", "const c = myprocess.env.MYP;", &[]),
        ("a.js", "const d = Deno.env.get(\"DENO\");", &[]),
        ("a.rs", "let x = 42;", &[]),
    ];
    for (path, line, want) in cases {
        let lang = ripwire_rs::Lang::from_path(std::path::Path::new(path));
        let names: Vec<String> = lang
            .semantics()
            .flag_facts(&[line], path)
            .into_iter()
            .filter_map(|fact| match fact {
                FlagFact::Env { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(names, *want, "{path}: {line}");
    }
}

#[test]
fn test_gate_extractors() {
    let gate = |src: &[&str]| {
        extract_settings_gate(src, 0).map(|g| (g.name, g.default, g.dark, g.define_idx))
    };
    assert_eq!(
        gate(&["#ifndef DARK_GATE", "#define DARK_GATE 0", "int x;"]),
        Some(("DARK_GATE".into(), "0".into(), true, 1))
    );
    assert_eq!(
        gate(&["#ifndef LIT_GATE", "", "#define LIT_GATE 1"]),
        Some(("LIT_GATE".into(), "1".into(), false, 2))
    );
    assert_eq!(gate(&["#ifndef GUARD", "#define GUARD"]), None);
    assert_eq!(
        gate(&["#ifndef MY_HEADER_H", "#define MY_HEADER_H 1"]),
        Some(("MY_HEADER_H".into(), "1".into(), false, 1))
    );
    assert_eq!(
        gate(&["#ifdef SOME_FEATURE", "#define SOME_FEATURE 1"]),
        None
    );
    assert_eq!(gate(&["#ifndef A", "#define B 1"]), None);
    let lines = [
        "#if defined(RW_SIMD) && RW_SIMD",
        "a;",
        "b;",
        "#endif",
        "#ifndef RW_SIMD",
        "#endif",
        "#ifdef RW_TRACE",
        "#elif 1",
        "#endif",
    ];
    let uses: Vec<(String, usize, usize)> = conditional_uses(&lines)
        .into_iter()
        .map(|u| (u.name, u.line_idx, u.loc))
        .collect();
    assert_eq!(
        uses,
        [
            ("RW_SIMD".to_string(), 0, 2),
            ("RW_SIMD".to_string(), 0, 2),
            ("RW_TRACE".to_string(), 6, 1),
        ]
    );
    assert_eq!(
        extract_cmake_option("option(ENABLE_TESTS \"doc\" ON)", "CMakeLists.txt"),
        Some(("ENABLE_TESTS".into(), "ON".into(), false))
    );
    assert_eq!(
        extract_cmake_option("option(ENABLE_TESTS \"doc\" ON)", "main.rs"),
        None
    );
}

#[test]
fn test_whereis_reports_indexed_definition() {
    let mut ingest = IngestResult::new();
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/main.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let sym = Symbol::new(
        0,
        interner.intern("main"),
        interner.intern("c_main"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    ingest.files.push(f0);
    ingest.symbols.push(sym);

    let sources = [("src/main.rs", b"fn main() {}\n".as_slice())];
    let whereis_res = whereis_from_sources_with_facts(
        &sources,
        &ingest,
        &interner,
        "main",
        &WhereisFacts::default(),
    );
    assert_eq!(whereis_res["git"], false);
    assert!(whereis_res["on_head"].is_null());
    assert_eq!(whereis_res["hits"][0]["kind"], "def");
    assert_eq!(whereis_res["hits"][0]["line"], 1);
}

#[test]
fn test_whereis_outside_git_discloses_no_git_and_omits_head_claims() {
    let mut ingest = IngestResult::new();
    let interner = Interner::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/main.rs"),
        lang: Lang::Rust,
        byte_size: 100,
        line_count: 1,
    });
    ingest.symbols.push(Symbol::new(
        0,
        interner.intern("main"),
        interner.intern("main"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    ));
    let sources = [("src/main.rs", b"fn main() {}\n".as_slice())];
    let val = whereis_from_sources_with_facts(
        &sources,
        &ingest,
        &interner,
        "main",
        &WhereisFacts::default(),
    );
    let mut out = Vec::new();
    ripwire_rs::serialize::serialize_whereis_xml(&val, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains(" git=\"0\""));
    assert!(!xml.contains("on-head="));
    assert!(!xml.contains("complete="));
    assert!(!xml.contains(" at=\""));
    assert!(!xml.contains(" ref=\""));
    assert!(xml.contains("kind=\"def\""));
}

#[test]
fn test_whereis_from_sources_with_facts_branch_and_history() {
    let mut ingest = IngestResult::new();
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/main.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    ingest.files.push(f0);

    let sources = [("src/main.rs", b"// file without symbol\n".as_slice())];
    let facts = WhereisFacts {
        history: Some(json!({
            "probed": 1,
            "head": "a1b2c3d",
            "fate": {
                "v": "removed",
                "commit": "deadbeef",
                "date": "2026-10-18",
                "subject": "deprecate and remove legacy_target"
            }
        })),
        head_date: Some("2026-10-21".to_string()),
        at_stamp: Some("a1b2c3d".to_string()),
        branch_rows: vec![json!({
            "ref": "feature/branch-x",
            "tip": "feedface",
            "date": "2026-10-19",
            "symbol": "legacy_target",
            "kind": "def",
            "file": "src/main.rs",
            "path": "src/main.rs",
            "line": 4,
            "text": "pub fn legacy_target() {}",
            "on_head": false,
        })],
        refs_scanned: 3,
    };

    let res =
        whereis_from_sources_with_facts(&sources, &ingest, &interner, "legacy_target", &facts);
    assert_eq!(res["symbol"], "legacy_target");
    assert_eq!(res["on_head"], false);
    assert_eq!(res["refs_scanned"], 3);
    assert_eq!(res["head_date"], "2026-10-21");
    assert_eq!(res["at"], "a1b2c3d");
    assert_eq!(res["hits"].as_array().unwrap().len(), 1);
    assert_eq!(res["hits"][0]["ref"], "feature/branch-x");
    assert_eq!(res["hits"][0]["kind"], "def");
    assert_eq!(res["hits"][0]["line"], 4);
    assert_eq!(res["history"]["fate"]["v"], "removed");
    assert_eq!(res["history"]["fate"]["commit"], "deadbeef");
}

#[test]
fn test_doc_drift_backtick_and_line_anchors() {
    let mut file_map = HashMap::new();
    file_map.insert("src/lib.rs", 20);
    let mut sym_set = HashSet::new();
    sym_set.insert("existingFn");

    let mut ingest = IngestResult::new();
    let interner = Interner::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/lib.rs"),
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 100,
    });
    let mut sym = Symbol::new(
        0,
        interner.intern("existingFn"),
        interner.intern("src/lib.rs::existingFn"),
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    sym.loc = 3;
    ingest.symbols.push(sym);

    let mut drift = Vec::new();
    let counts = scan_doc_text_for_drift(
        "See `existingFn` at src/lib.rs:3 and `deletedFunction` at src/lib.rs:50 (`--flag` `<s t=\"fn\">` `multi word`)\n```rust\n`ignoredInFence`\n```",
        "README.md",
        &file_map,
        &sym_set,
        &ingest,
        &interner,
        &mut drift,
    );
    assert_eq!(counts.total, 4);
    assert_eq!(counts.checked, 4);
    assert_eq!(drift.len(), 3);
    assert!(
        drift
            .iter()
            .any(|d| d["why"] == "line-moved" && d["tgt"] == "src/lib.rs:10")
    );
    assert!(drift.iter().any(|d| d["referenced"] == "src/lib.rs:50"));
    assert!(drift.iter().any(|d| d["referenced"] == "deletedFunction"));
    let sources = [("src/lib.rs", b"fn existingFn() {}\n".as_slice())];
    filter_present_in_sources_drift(&sources, &ingest, &interner, &[], &mut drift);
    assert_eq!(drift.len(), 3);
    let mut wanted = HashSet::new();
    wanted.insert("deletedFunction".to_string());
    let mut wants_first = [false; 256];
    wants_first[b'd' as usize] = true;
    let found =
        collect_file_wanted_tokens(b"fn helper() { deletedFunction(); }", &wants_first, &wanted);
    assert_eq!(found, vec!["deletedFunction".to_string()]);
    let mut xml_buf = Vec::new();
    serialize_doc_drift_xml(
        &json!({
            "clean_docs": 2,
            "dated_docs": 1,
            "prose_docs": 1,
            "corpus": 10,
            "anchors": 4,
            "checked": 4,
            "unchecked": 0,
            "drift": drift
        }),
        &mut xml_buf,
    )
    .unwrap();
    let xml_str = String::from_utf8(xml_buf).unwrap();
    assert!(xml_str.contains("deletedFunction"));
    assert!(xml_str.contains("prose=\"1\" corpus=\"10\""));
}
