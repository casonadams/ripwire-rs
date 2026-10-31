use ripwire_rs::query::defs::{DefsRequest, MAX_DEFS_PER_NAME, call_defs, split_defs_names};
use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, find_symbols_by_name,
    ingest_in_memory_files,
};
use serde_json::Value;

fn corpus(interner: &Interner, files: &[(&str, Lang, &[u8])]) -> (IngestResult, Graph) {
    let registry = LanguageRegistry::new().unwrap();
    let ingest = ingest_in_memory_files(files, &registry, interner);
    let graph = Graph::build(&ingest, interner);
    (ingest, graph)
}

fn defs_for(names: &str, ingest: &IngestResult, graph: &Graph, interner: &Interner) -> Value {
    let names = split_defs_names(names);
    let request = DefsRequest {
        root: ".",
        names: &names,
    };
    call_defs(&request, ingest, graph, interner)
}

#[test]
fn every_sel_binds_exactly_its_own_definition() {
    let interner = Interner::new();
    let (ingest, graph) = corpus(
        &interner,
        &[
            (
                "src/a.rs",
                Lang::Rust,
                b"struct Alpha; impl Alpha { fn build(&self) {} }",
            ),
            (
                "src/b.rs",
                Lang::Rust,
                b"struct Beta; impl Beta { fn build(&self) {} }",
            ),
            ("src/c.rs", Lang::Rust, b"fn build() {}"),
        ],
    );
    let val = defs_for("build", &ingest, &graph, &interner);
    let rows = val["defs"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let sel = row["sel"].as_str().expect("ambiguous name carries a sel");
        let bound = find_symbols_by_name(sel, &ingest, &interner);
        assert_eq!(bound.len(), 1, "sel {sel} must bind one definition");
        let sym = ingest.sym(bound[0]).unwrap();
        assert_eq!(sym.line as u64, row["line"].as_u64().unwrap());
    }
}

#[test]
fn unique_name_has_no_sel_and_unknown_name_is_missing() {
    let interner = Interner::new();
    let (ingest, graph) = corpus(&interner, &[("src/a.rs", Lang::Rust, b"fn only_one() {}")]);
    let val = defs_for("only_one,ghost", &ingest, &graph, &interner);
    let rows = val["defs"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["sel"].is_null());
    assert_eq!(val["missing"][0], "ghost");
}

#[test]
fn rows_past_the_per_name_cap_are_counted_not_listed() {
    let interner = Interner::new();
    let sources: Vec<(String, String)> = (0..MAX_DEFS_PER_NAME + 3)
        .map(|i| (format!("src/m{i:02}.rs"), "fn dup() {}".to_string()))
        .collect();
    let files: Vec<(&str, Lang, &[u8])> = sources
        .iter()
        .map(|(p, c)| (p.as_str(), Lang::Rust, c.as_bytes()))
        .collect();
    let (ingest, graph) = corpus(&interner, &files);
    let val = defs_for("dup", &ingest, &graph, &interner);
    assert_eq!(val["shown"], MAX_DEFS_PER_NAME);
    assert_eq!(val["total"], MAX_DEFS_PER_NAME + 3);
}

#[test]
fn xml_marks_test_code_and_caps_once_on_the_root() {
    let interner = Interner::new();
    let (ingest, graph) = corpus(
        &interner,
        &[
            (
                "src/lib.rs",
                Lang::Rust,
                b"fn helper() {} fn caller() { helper(); }",
            ),
            ("tests/t.rs", Lang::Rust, b"fn helper() {}"),
        ],
    );
    let val = defs_for("helper", &ingest, &graph, &interner);
    let mut out = Vec::new();
    ripwire_rs::serialize::serialize_defs_xml(&val, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert_eq!(xml.matches("test=\"1\"").count(), 1);
    assert!(xml.contains(" in=\"1\""));
    assert!(!xml.contains("capped"));
}
