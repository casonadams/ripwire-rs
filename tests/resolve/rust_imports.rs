use crate::a_callees::callees_described;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

/// A callee described by the path of its file.
fn callee_file(ingest: &ripwire_rs::IngestResult, interner: &Interner, id: u32) -> String {
    ingest
        .sym_file_path(&ingest.symbols[id as usize], interner)
        .to_string()
}

const MODULES: [(&str, Lang, &[u8]); 2] = [
    (
        "src/commands/ast_match.rs",
        Lang::Rust,
        b"pub fn execute() {}\n",
    ),
    ("src/other.rs", Lang::Rust, b"pub fn execute() {}\n"),
];

#[test]
fn import_bindings_decide_which_module_a_qualified_call_reaches() {
    let cases: [(&[u8], &[&str]); 3] = [
        (
            b"use ripwire_rs::commands::{ast_match, at};\nfn probe() { ast_match::execute(); }\n",
            &["src/commands/ast_match.rs"],
        ),
        (
            b"use ripwire_rs::commands::{ast_match as am};\nfn probe() { am::execute(); }\n",
            &["src/commands/ast_match.rs"],
        ),
        (
            b"use ripwire_rs::commands::at;\nfn probe() { ast_match::execute(); }\n",
            &[],
        ),
    ];
    for (source, expected) in cases {
        let files = [
            ("tests/cli/retrieval.rs", Lang::Rust, source),
            MODULES[0],
            MODULES[1],
        ];
        assert_eq!(callees_described(&files, "probe", callee_file), expected);
    }
}

#[test]
fn test_module_path_call_binds_the_module_function_not_a_trait_method_of_that_name() {
    let files: [(&str, Lang, &[u8]); 3] = [
        (
            "src/commands/mod.rs",
            Lang::Rust,
            b"pub trait Command { fn run(&self, root: &str) -> Option<u8>; }\npub fn run(root: &str) -> Option<u8> { None }\n",
        ),
        (
            "src/commands/alpha.rs",
            Lang::Rust,
            b"struct Alpha;\nimpl crate::commands::Command for Alpha { fn run(&self, root: &str) -> Option<u8> { None } }\n",
        ),
        (
            "src/cli/dispatch.rs",
            Lang::Rust,
            b"pub fn probe(root: &str) { crate::commands::run(root); }\n",
        ),
    ];
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&files, &reg, &interner);
    let graph = Graph::build(&ingest, &interner);
    let probe = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "probe")
        .unwrap()
        .id;
    let callees: Vec<(u32, bool)> = graph
        .callees(probe)
        .map(|(id, _)| {
            let sym = &ingest.symbols[id as usize];
            (sym.line, sym.scope.is_some())
        })
        .collect();
    assert_eq!(
        callees,
        [(2, false)],
        "only the free `run` in src/commands/mod.rs"
    );
}
