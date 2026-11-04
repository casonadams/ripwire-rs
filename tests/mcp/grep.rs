use crate::uses_support::uses_json;
use ripwire_rs::query::grep::{TierCounts, enclosing_metadata, tier_attrs};
use ripwire_rs::serialize::serialize_grep_xml;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};
use serde_json::{Value, json};

type ReadCase = (
    &'static str,
    Lang,
    &'static [u8],
    &'static [u32],
    &'static [&'static str],
);

fn counts(code: usize, comment: usize, literal: usize) -> TierCounts {
    TierCounts {
        code,
        comment,
        literal,
    }
}

#[test]
fn tier_disclosure_follows_the_share_of_code_hits() {
    let attrs = tier_attrs(counts(5, 2, 2), 3, false);
    assert_eq!(attrs["suppressed_comment"], json!(2));
    assert!(!attrs.contains_key("suppressed_string"));
    assert_eq!(attrs["tier"], json!("code+string"));
    assert_eq!(attrs["string_hits"], json!(2));
    assert_eq!(attrs["tier_parsed"], json!(3));

    let attrs = tier_attrs(counts(8, 0, 1), 1, false);
    assert_eq!(attrs["suppressed_string"], json!(1));
    assert!(!attrs.contains_key("tier"));
    assert!(!attrs.contains_key("suppressed_comment"));

    let attrs = tier_attrs(counts(0, 1, 1), 1, false);
    assert_eq!(attrs["tier"], json!("comment+string"));
    assert!(!attrs.contains_key("suppressed_comment"));
    assert!(!attrs.contains_key("string_hits"));

    assert!(tier_attrs(counts(4, 0, 0), 2, false).is_empty());
    assert!(tier_attrs(counts(4, 3, 3), 2, true).is_empty());
}

fn semantic_read_report(path: &str, lang: Lang, source: &[u8]) -> (Value, String) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&[(path, lang, source)], &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let symbol = ingest
        .symbols
        .iter()
        .find(|symbol| interner.resolve(&symbol.name) == "LIMIT")
        .unwrap();
    assert!(graph.callers(symbol.id).next().is_none());
    let hits = vec![json!({
        "path": path, "line": symbol.line, "text": "LIMIT", "enclosing": "LIMIT", "enc_id": 1
    })];
    let enc = enclosing_metadata(&ingest, &interner, Some(&graph), None, &hits);
    let mut xml = Vec::new();
    serialize_grep_xml(
        &json!({
            "pattern": "LIMIT", "verb": "grep", "literal": true, "hits": hits, "enc": enc
        }),
        &mut xml,
    )
    .unwrap();
    (
        uses_json(&ingest, &interner, "LIMIT"),
        String::from_utf8(xml).unwrap(),
    )
}

fn assert_read_sites(uses: &Value, lines: &[u32]) {
    let sites = uses["uses"].as_array().unwrap();
    assert_eq!(sites.len(), lines.len(), "{uses}");
    for (site, line) in sites.iter().zip(lines) {
        assert_eq!(site["role"], "read", "{uses}");
        assert_eq!(site["line"], *line, "{uses}");
    }
}

#[test]
fn constant_reads_bind_only_to_the_repository_definition() {
    let cases: [ReadCase; 6] = [
        (
            "src/sample.py",
            Lang::Python,
            b"LIMIT = 10\n\ndef first():\n    text = 'LIMIT'\n    return LIMIT\n\ndef second():\n    return LIMIT\n\ndef parameter(LIMIT):\n    return LIMIT\n\ndef local():\n    LIMIT = 3\n    return LIMIT\n\ndef write():\n    global LIMIT\n    LIMIT = 4\n\ndef keyword():\n    return object(LIMIT=2)\n# LIMIT\n",
            &[5, 8],
            &[
                "reads=\"2\" reads_at=\"src/sample.py:5 src/sample.py:8\"",
                "next=\"--uses=LIMIT\"",
            ],
        ),
        (
            "src/sample.rs",
            Lang::Rust,
            b"const LIMIT: u32 = 10;\nfn first() -> u32 { LIMIT }\nfn second() -> u32 { LIMIT }\nfn parameter(LIMIT: u32) -> u32 { LIMIT }\nfn local() -> u32 { let LIMIT = 3; LIMIT }\nfn write() { let mut LIMIT = 0; LIMIT = 4; let text = \"LIMIT\"; }\n// LIMIT\n",
            &[2, 3],
            &["reads=\"2\" reads_at=\"src/sample.rs:2 src/sample.rs:3\""],
        ),
        (
            "src/sample.rs",
            Lang::Rust,
            b"const LIMIT: u32 = 10;\nfn sample() -> u32 {\n    let before = LIMIT;\n    let LIMIT = LIMIT + 1;\n    LIMIT + before\n}\n",
            &[3, 4],
            &[],
        ),
        (
            "src/sample.js",
            Lang::JavaScript,
            b"const LIMIT = 10;\nfunction first() { return { LIMIT }; }\nfunction nested() { if (true) { var LIMIT = 3; } return LIMIT; }\nfunction block() { { const LIMIT = 3; } return LIMIT; }\nfunction pattern({LIMIT}) { return LIMIT; }\n",
            &[2, 4],
            &[],
        ),
        (
            "src/sample.py",
            Lang::Python,
            b"LIMIT = 10\nclass Sample:\n    LIMIT = 3\n    def sample(self):\n        return LIMIT\n",
            &[5],
            &[],
        ),
        (
            "src/sample.py",
            Lang::Python,
            b"LIMIT = 10\ndef sample(value=LIMIT):\n    LIMIT = 3\n    return LIMIT\n",
            &[2],
            &[],
        ),
    ];
    for (path, lang, source, lines, needles) in cases {
        let (uses, xml) = semantic_read_report(path, lang, source);
        assert_read_sites(&uses, lines);
        for needle in needles {
            assert!(xml.contains(needle), "{xml}");
        }
    }
}

#[test]
fn test_javascript_and_typescript_variable_reads_ignore_targets_and_shadowing() {
    let source = b"const LIMIT = 10;\nfunction first() { return LIMIT; }\nfunction second() { return { value: LIMIT }; }\nfunction parameter(LIMIT) { return LIMIT; }\nfunction local() { const LIMIT = 3; return LIMIT; }\nfunction write() { LIMIT = 4; return { LIMIT: 'LIMIT' }; }\n// LIMIT\n";
    for (path, lang) in [
        ("src/sample.js", Lang::JavaScript),
        ("src/sample.ts", Lang::TypeScript),
    ] {
        let (uses, xml) = semantic_read_report(path, lang, source);
        let sites = uses["uses"].as_array().unwrap();
        let reads: Vec<Value> = sites
            .iter()
            .filter(|u| u["role"] == "read")
            .cloned()
            .collect();
        assert_read_sites(&json!({ "uses": reads }), &[2, 3]);
        assert!(
            sites.iter().any(|u| u["role"] == "write" && u["line"] == 6),
            "{uses}"
        );
        assert!(xml.contains("reads=\"2\""), "{xml}");
    }
}

#[test]
fn test_grep_reads_do_not_cross_dialect_groups() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let rust = b"fn first() -> u32 { let LAYOUT = 3; LAYOUT }\nfn second() -> u32 { LAYOUT }\n";
    let ts = b"const LAYOUT = 10;\nfunction show() { return LAYOUT; }\n";
    let ingest = ingest_in_memory_files(
        &[
            ("src/app.rs", Lang::Rust, rust.as_slice()),
            ("src/app.ts", Lang::TypeScript, ts.as_slice()),
        ],
        &registry,
        &interner,
    );
    let hits = vec![
        json!({"path": "src/app.ts", "line": 1, "text": "LAYOUT", "enclosing": "LAYOUT", "enc_id": 1}),
    ];
    let enc = enclosing_metadata(&ingest, &interner, None, None, &hits);
    assert_eq!(enc["LAYOUT"]["reads"], 1, "{enc:?}");
    assert_eq!(
        enc["LAYOUT"]["reads_at"],
        json!(["src/app.ts:2"]),
        "{enc:?}"
    );
}

#[test]
fn test_callable_values_do_not_add_call_edges() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let source =
        b"def helper():\n    pass\ndef store():\n    return helper\ndef invoke():\n    helper()\n";
    let ingest = ingest_in_memory_files(
        &[("src/sample.py", Lang::Python, source.as_slice())],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let helper = ingest
        .symbols
        .iter()
        .find(|symbol| interner.resolve(&symbol.name) == "helper")
        .unwrap();
    assert_eq!(graph.callers(helper.id).count(), 1);
    let uses = uses_json(&ingest, &interner, "helper");
    let sites = uses["uses"].as_array().unwrap();
    assert_eq!(sites.len(), 2, "{uses}");
    assert_eq!(sites[0]["role"], "read");
    assert_eq!(sites[1]["role"], "call");
}

#[test]
fn test_python_local_import_shadows_repository_name() {
    let source = b"LIMIT = 10\ndef first():\n    return LIMIT\ndef imported():\n    from sample import LIMIT\n    return LIMIT\n";
    let (uses, _) = semantic_read_report("src/sample.py", Lang::Python, source);
    let reads: Vec<Value> = uses["uses"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|site| site["role"] == "read")
        .cloned()
        .collect();
    assert_eq!(reads.len(), 1, "{uses}");
    assert_eq!(reads[0]["line"], 3);
}

#[test]
fn oversized_grep_context_is_clamped_to_the_available_source_lines() {
    let source = b"fn first() {}\nfn needle() {}\nfn last() {}\n".as_slice();
    let sources = [("src/lib.rs", source)];
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest =
        ingest_in_memory_files(&[("src/lib.rs", Lang::Rust, source)], &registry, &interner);
    let request = ripwire_rs::query::retrieval::GrepRequest {
        before: usize::MAX,
        after: usize::MAX,
        ..ripwire_rs::query::retrieval::GrepRequest::new(":memory:", "needle", &sources)
    };
    let report = ripwire_rs::query::grep::call_grep(&ingest, &interner, &request);
    assert_eq!(report["context"], true);
    assert_eq!(report["hits"][0]["line"], 2);
    assert_eq!(report["hits"][0]["before"], json!(["fn first() {}"]));
    assert_eq!(report["hits"][0]["after"], json!(["fn last() {}"]));
}
