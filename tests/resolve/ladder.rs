use ripwire_rs::{
    FileInfo, IngestResult, Interner, Lang, RefRole, Reference, ResolutionResult, SymKind, Symbol,
    resolve_all_references,
};

/// Resolves one call from `caller` (file 0) to `name`, with one candidate per entry of `candidate_files`.
fn resolve_call(paths: &[&str], name: &str, candidate_files: &[u32]) -> ResolutionResult {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    for (id, path) in paths.iter().enumerate() {
        ingest.files.push(FileInfo {
            id: id as u32,
            path: interner.intern(path),
            lang: Lang::Rust,
            line_count: 10,
            byte_size: 100,
        });
    }
    let name = interner.intern(name);
    let canon = interner.intern("canon");
    let caller = interner.intern("caller");
    ingest.symbols.push(Symbol::new(
        0,
        caller,
        canon,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    ));
    for (i, &file) in candidate_files.iter().enumerate() {
        ingest.symbols.push(Symbol::new(
            i as u32 + 1,
            name,
            canon,
            SymKind::Function,
            Lang::Rust,
            file,
            5,
        ));
    }
    ingest
        .references
        .push(Reference::new(0, name, 0, 2, Lang::Rust, RefRole::Call));
    resolve_all_references(&ingest, &interner)
}

#[test]
fn test_precedence_ladder_same_file() {
    let res = resolve_call(&["src/a.rs", "src/b.rs"], "target", &[0, 1]);
    assert_eq!(res.edge_accumulators.len(), 1);
    assert!(res.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res.edge_accumulators[&(0, 1)].conf_sum, 1.0);
    assert_eq!(res.edge_accumulators[&(0, 1)].n_ref, 1);
    assert_eq!(res.amb_out[0], 0);
}

#[test]
fn test_precedence_ladder_same_dir_and_unique_global() {
    let paths = ["pkg/a.rs", "pkg/b.rs", "other/c.rs"];
    let res = resolve_call(&paths, "shared", &[1, 2]);
    assert_eq!(res.edge_accumulators.len(), 1);
    assert!(res.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res.edge_accumulators[&(0, 1)].conf_sum, 0.5);

    let res_global = resolve_call(&paths, "shared", &[2]);
    assert_eq!(res_global.edge_accumulators.len(), 1);
    assert!(res_global.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res_global.edge_accumulators[&(0, 1)].conf_sum, 0.2);
}

#[test]
fn test_precedence_ladder_non_unique_global_dropped() {
    let res = resolve_call(&["src/a.rs", "dir1/b.rs", "dir2/c.rs"], "common", &[1, 2]);
    assert!(res.edge_accumulators.is_empty());
}

#[test]
fn test_discard_self_loops() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/a.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let name = interner.intern("recursive");
    let canon = interner.intern("canon");
    let sym = Symbol::new(0, name, canon, SymKind::Function, Lang::Rust, 0, 1);
    let call = Reference::new(0, name, 0, 2, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.push(f0);
    ingest.symbols.push(sym);
    ingest.references.push(call);

    let res = resolve_all_references(&ingest, &interner);
    assert!(res.edge_accumulators.is_empty());
    assert_eq!(res.amb_out[0], 0);
}

#[test]
fn test_std_member_gate_declines_unpinned_cross_class_calls() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/lib.rs"),
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 200,
    };
    let len_name = interner.intern("len");
    let buf_scope = interner.intern("CandBuffer");
    let other_scope = interner.intern("Other");
    let buf_len = Symbol::new(
        0,
        len_name,
        interner.intern("src/lib.rs::CandBuffer::len"),
        SymKind::Method,
        Lang::Rust,
        0,
        1,
    )
    .with_scope(buf_scope);
    let same_scope_caller = Symbol::new(
        1,
        interner.intern("push"),
        interner.intern("src/lib.rs::CandBuffer::push"),
        SymKind::Method,
        Lang::Rust,
        0,
        5,
    )
    .with_scope(buf_scope);
    let unrelated_caller = Symbol::new(
        2,
        interner.intern("work"),
        interner.intern("src/lib.rs::Other::work"),
        SymKind::Method,
        Lang::Rust,
        0,
        10,
    )
    .with_scope(other_scope);

    let call_same_scope = Reference::new(1, len_name, 0, 6, Lang::Rust, RefRole::Call);
    let call_unpinned = Reference::new(2, len_name, 0, 11, Lang::Rust, RefRole::Call);
    let call_qualified =
        Reference::new(2, len_name, 0, 12, Lang::Rust, RefRole::Call).with_qualifier(buf_scope);

    let mut ingest = IngestResult::new();
    ingest.files.push(f0);
    ingest
        .symbols
        .extend_from_slice(&[buf_len, same_scope_caller, unrelated_caller]);
    ingest
        .references
        .extend_from_slice(&[call_same_scope, call_unpinned]);

    let res = resolve_all_references(&ingest, &interner);
    assert!(res.edge_accumulators.contains_key(&(1, 0)));
    assert!(!res.edge_accumulators.contains_key(&(2, 0)));

    ingest.references.push(call_qualified);
    let res2 = resolve_all_references(&ingest, &interner);
    assert!(res2.edge_accumulators.contains_key(&(2, 0)));
}

#[test]
fn test_same_dir_starburst_declined_above_cap() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let name = interner.intern("common_helper");
    let canon = interner.intern("canon");
    for fid in 0..6u32 {
        ingest.files.push(FileInfo {
            id: fid,
            path: interner.intern(&format!("src/f{fid}.rs")),
            lang: Lang::Rust,
            line_count: 10,
            byte_size: 100,
        });
        if fid == 0 {
            ingest.symbols.push(Symbol::new(
                0,
                interner.intern("caller"),
                canon,
                SymKind::Function,
                Lang::Rust,
                0,
                1,
            ));
        } else {
            ingest.symbols.push(Symbol::new(
                fid,
                name,
                canon,
                SymKind::Function,
                Lang::Rust,
                fid,
                2,
            ));
        }
    }
    ingest
        .references
        .push(Reference::new(0, name, 0, 3, Lang::Rust, RefRole::Call));
    let res = resolve_all_references(&ingest, &interner);
    assert!(res.edge_accumulators.is_empty());
    assert_eq!(res.declined, 1);
    assert_eq!(res.unresolved, 0);
}

#[test]
fn test_std_qualifier_and_cross_file_scope_evidence() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/index.ts"),
        lang: Lang::TypeScript,
        line_count: 20,
        byte_size: 200,
    };
    let f1 = FileInfo {
        id: 1,
        path: interner.intern("src/caller_with_type.ts"),
        lang: Lang::TypeScript,
        line_count: 20,
        byte_size: 200,
    };
    let f2 = FileInfo {
        id: 2,
        path: interner.intern("src/unrelated_set_user.ts"),
        lang: Lang::TypeScript,
        line_count: 20,
        byte_size: 200,
    };
    let has_name = interner.intern("has");
    let is_arr_name = interner.intern("isArray");
    let idx_scope = interner.intern("SpanIndex");
    let arr_qual = interner.intern("Array");

    let anchor_has = Symbol::new(
        0,
        has_name,
        interner.intern("src/index.ts::SpanIndex::has"),
        SymKind::Method,
        Lang::TypeScript,
        0,
        5,
    )
    .with_scope(idx_scope);
    let local_is_arr = Symbol::new(
        1,
        is_arr_name,
        interner.intern("src/index.ts::isArray"),
        SymKind::Function,
        Lang::TypeScript,
        0,
        10,
    );
    let typed_caller = Symbol::new(
        2,
        interner.intern("verifySpans"),
        interner.intern("src/caller_with_type.ts::verifySpans"),
        SymKind::Function,
        Lang::TypeScript,
        1,
        1,
    );
    let untyped_caller = Symbol::new(
        3,
        interner.intern("isIgnoredPath"),
        interner.intern("src/unrelated_set_user.ts::isIgnoredPath"),
        SymKind::Function,
        Lang::TypeScript,
        2,
        1,
    );

    let mut ingest = IngestResult::new();
    ingest.files.extend_from_slice(&[f0, f1, f2]);
    ingest
        .symbols
        .extend_from_slice(&[anchor_has, local_is_arr, typed_caller, untyped_caller]);
    ingest.references.extend_from_slice(&[
        Reference::new(2, idx_scope, 1, 2, Lang::TypeScript, RefRole::Type),
        Reference::new(2, has_name, 1, 4, Lang::TypeScript, RefRole::Call),
        Reference::new(3, has_name, 2, 4, Lang::TypeScript, RefRole::Call),
        Reference::new(3, is_arr_name, 2, 5, Lang::TypeScript, RefRole::Call)
            .with_qualifier(arr_qual),
    ]);

    let res = resolve_all_references(&ingest, &interner);
    assert!(res.edge_accumulators.contains_key(&(2, 0)));
    assert!(!res.edge_accumulators.contains_key(&(3, 0)));
    assert!(!res.edge_accumulators.contains_key(&(3, 1)));
}

#[test]
fn test_interface_contract_fallback_resolves_polymorphic_calls() {
    let interner = Interner::new();
    let m_name = interner.intern("render");
    let tr_name = interner.intern("Widget");
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/api/trait_def.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    });
    ingest.symbols.push(Symbol::new(
        0,
        tr_name,
        interner.intern("src/api/trait_def.rs::Widget"),
        SymKind::Interface,
        Lang::Rust,
        0,
        1,
    ));
    ingest.symbols.push(
        Symbol::new(
            1,
            m_name,
            interner.intern("src/api/trait_def.rs::Widget::render"),
            SymKind::Method,
            Lang::Rust,
            0,
            2,
        )
        .with_scope(tr_name),
    );
    for i in 0..3u32 {
        let fid = i + 1;
        let nid = i + 2;
        let sc = interner.intern(&format!("Impl{i}"));
        ingest.files.push(FileInfo {
            id: fid,
            path: interner.intern(&format!("src/impls/impl{i}.rs")),
            lang: Lang::Rust,
            line_count: 10,
            byte_size: 100,
        });
        ingest.symbols.push(
            Symbol::new(
                nid,
                m_name,
                interner.intern(&format!("src/impls/impl{i}.rs::Impl{i}::render")),
                SymKind::Method,
                Lang::Rust,
                fid,
                1,
            )
            .with_scope(sc),
        );
    }
    ingest.files.push(FileInfo {
        id: 4,
        path: interner.intern("src/app/runner.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    });
    ingest.symbols.push(Symbol::new(
        5,
        interner.intern("run_all"),
        interner.intern("src/app/runner.rs::run_all"),
        SymKind::Function,
        Lang::Rust,
        4,
        1,
    ));
    let mut call = Reference::new(5, m_name, 4, 3, Lang::Rust, RefRole::Call);
    call.member_call = true;
    ingest.references.push(call);
    for sym in ingest.symbols.iter_mut().filter(|s| s.name == m_name) {
        sym.takes_self = true;
    }
    let res = resolve_all_references(&ingest, &interner);
    assert!(res.edge_accumulators.contains_key(&(5, 1)));
}

#[test]
fn test_triage_external_declined_and_unresolved_counts() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/main.rs"),
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 200,
    });
    ingest.files.push(FileInfo {
        id: 1,
        path: interner.intern("scripts/helper.py"),
        lang: Lang::Python,
        line_count: 10,
        byte_size: 100,
    });
    let caller = Symbol::new(
        0,
        interner.intern("caller"),
        interner.intern("src/main.rs::caller"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let py_only = Symbol::new(
        1,
        interner.intern("py_target"),
        interner.intern("scripts/helper.py::py_target"),
        SymKind::Function,
        Lang::Python,
        1,
        1,
    );
    let len_sym = Symbol::new(
        2,
        interner.intern("len"),
        interner.intern("src/main.rs::Other::len"),
        SymKind::Method,
        Lang::Rust,
        0,
        10,
    )
    .with_scope(interner.intern("Other"));
    ingest
        .symbols
        .extend_from_slice(&[caller, py_only, len_sym]);
    ingest.references.extend_from_slice(&[
        Reference::new(
            0,
            interner.intern("println"),
            0,
            2,
            Lang::Rust,
            RefRole::Call,
        ),
        Reference::new(
            0,
            interner.intern("py_target"),
            0,
            3,
            Lang::Rust,
            RefRole::Call,
        ),
        Reference::new(0, interner.intern("len"), 0, 4, Lang::Rust, RefRole::Call),
        Reference::new(
            0,
            interner.intern("local_var"),
            0,
            5,
            Lang::Rust,
            RefRole::Read,
        ),
        Reference::new(
            0,
            interner.intern("py_target"),
            0,
            6,
            Lang::Rust,
            RefRole::Read,
        ),
    ]);
    let res = resolve_all_references(&ingest, &interner);
    assert_eq!(res.external, 1);
    assert_eq!(res.unresolved, 1);
    assert_eq!(res.declined, 1);
}

/// A std member name shared with in-tree symbols (a `lines` variable, a `chain` fn in another file) is still the std
/// method on an untyped receiver: it is declined or external, never unresolved. A tree-sitter `Node` receiver's
/// `walk` is external too.
#[test]
fn test_std_member_names_shadowed_by_in_tree_symbols_are_not_unresolved() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    for (id, path) in [(0, "src/reader.rs"), (1, "src/chain.rs")] {
        ingest.files.push(FileInfo {
            id,
            path: interner.intern(path),
            lang: Lang::Rust,
            line_count: 20,
            byte_size: 200,
        });
    }
    let caller = Symbol::new(
        0,
        interner.intern("read_all"),
        interner.intern("src/reader.rs::read_all"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let lines_var = Symbol::new(
        1,
        interner.intern("lines"),
        interner.intern("src/reader.rs::lines"),
        SymKind::Var,
        Lang::Rust,
        0,
        1,
    );
    let chain_fn = Symbol::new(
        2,
        interner.intern("chain"),
        interner.intern("src/chain.rs::chain"),
        SymKind::Function,
        Lang::Rust,
        1,
        1,
    );
    let walk_fn = Symbol::new(
        3,
        interner.intern("walk"),
        interner.intern("src/chain.rs::walk"),
        SymKind::Function,
        Lang::Rust,
        1,
        2,
    );
    ingest
        .symbols
        .extend_from_slice(&[caller, lines_var, chain_fn, walk_fn]);
    let member_call = |name: &str, line: u32| {
        let mut call = Reference::new(0, interner.intern(name), 0, line, Lang::Rust, RefRole::Call);
        call.member_call = true;
        call
    };
    let node_walk = {
        let mut call = member_call("walk", 5);
        call.receiver_type = Some(interner.intern("Node"));
        call
    };
    ingest.references.extend_from_slice(&[
        member_call("lines", 2),
        member_call("chain", 3),
        member_call("skip", 4),
        node_walk,
    ]);
    let res = resolve_all_references(&ingest, &interner);
    assert_eq!(res.unresolved, 0);
    assert_eq!(res.external + res.declined, 4);
}

/// Each caller's name and the `scope::name` of every symbol it calls, for one in-memory file.
fn call_edges(path: &str, lang: Lang, source: &str) -> Vec<(String, String)> {
    use ripwire_rs::domain::CorpusView;
    let registry =
        ripwire_rs::LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ripwire_rs::ingest_in_memory_files(
        &[(path, lang, source.as_bytes())],
        &registry,
        &interner,
    );
    let graph = ripwire_rs::Graph::build(&ingest, &interner);
    let view = (&ingest, &interner);
    let label = |s: &Symbol| match s.scope {
        Some(sc) => format!("{}::{}", interner.resolve(&sc), view.sym_name(s)),
        None => view.sym_name(s).to_string(),
    };
    let mut edges: Vec<(String, String)> = ingest
        .symbols
        .iter()
        .flat_map(|callee| {
            graph
                .callers(callee.id)
                .map(|(caller, _)| (label(&ingest.symbols[caller as usize]), label(callee)))
                .collect::<Vec<_>>()
        })
        .collect();
    edges.sort();
    edges
}

/// For a corpus of in-memory files, `caller -> callee's file` for every call edge.
fn callee_files(files: &[(&str, Lang, &str)]) -> Vec<(String, String)> {
    use ripwire_rs::domain::CorpusView;
    let registry =
        ripwire_rs::LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, lang, src)| (*path, *lang, src.as_bytes()))
        .collect();
    let ingest = ripwire_rs::ingest_in_memory_files(&sources, &registry, &interner);
    let graph = ripwire_rs::Graph::build(&ingest, &interner);
    let view = (&ingest, &interner);
    let mut edges: Vec<(String, String)> = ingest
        .symbols
        .iter()
        .flat_map(|callee| {
            graph
                .callers(callee.id)
                .map(|(caller, _)| {
                    let caller = &ingest.symbols[caller as usize];
                    (
                        view.sym_name(caller).to_string(),
                        view.sym_path(callee).to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    edges.sort();
    edges
}

#[test]
fn test_a_bare_call_binds_to_the_imported_file_not_the_other_namesake() {
    let imported = [
        (
            "src/commands/common/mcp.rs",
            Lang::Rust,
            "pub fn mk(n: u8) -> u8 { n }\n",
        ),
        (
            "src/commands/map/tools.rs",
            Lang::Rust,
            "pub fn mk(n: u8) -> u8 { n + 1 }\n",
        ),
        (
            "src/commands/a.rs",
            Lang::Rust,
            "use super::common::mcp::mk;\npub fn tools() -> u8 { mk(1) }\n",
        ),
    ];
    assert_eq!(
        callee_files(&imported),
        vec![(
            "tools".to_string(),
            "src/commands/common/mcp.rs".to_string()
        )]
    );
    let local_shadows = [
        (
            "src/commands/common/mcp.rs",
            Lang::Rust,
            "pub fn mk(n: u8) -> u8 { n }\n",
        ),
        (
            "src/commands/a.rs",
            Lang::Rust,
            "use super::common::mcp::mk;\nfn mk(n: u8) -> u8 { n }\npub fn tools() -> u8 { mk(1) }\n",
        ),
    ];
    assert_eq!(
        callee_files(&local_shadows),
        vec![("tools".to_string(), "src/commands/a.rs".to_string())]
    );
}

#[test]
fn test_bare_calls_reach_module_functions_and_nested_defs_not_class_members() {
    let py = "class Client:\n    def load(self):\n        return 1\n\n    def run(self):\n        def helper():\n            return 2\n        return helper()\n\n\ndef load():\n    return 3\n\n\ndef use():\n    return load()\n";
    let ts = "class Client {\n  load(): number {\n    return 1;\n  }\n}\n\nfunction load(): number {\n  return 2;\n}\n\nexport function use(): number {\n  return load();\n}\n";
    let observed = (
        call_edges("src/client.py", Lang::Python, py),
        call_edges("src/client.ts", Lang::TypeScript, ts),
    );
    let edge = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        observed,
        (
            vec![edge("Client::run", "Client::helper"), edge("use", "load")],
            vec![edge("use", "load")],
        )
    );
}

/// One in-memory file and the exact `caller -> callee` edges it must produce.
struct EdgeCase {
    name: &'static str,
    path: &'static str,
    lang: Lang,
    source: &'static str,
    edges: &'static [(&'static str, &'static str)],
}

#[test]
fn single_file_call_edges_bind_by_receiver_type_and_path() {
    let cases = [
        EdgeCase {
            name: "method_named_like_a_container_method_needs_its_type_on_an_unknown_receiver",
            path: "src/jar.py",
            lang: Lang::Python,
            source: "class Jar:\n    def get(self, name):\n        return name\n\n    def lookup(self, name):\n        return self.get(name)\n\n\ndef read(headers):\n    return headers.get(\"Host\")\n",
            edges: &[("Jar::lookup", "Jar::get")],
        },
        EdgeCase {
            name: "property_reads_bind_to_the_property_but_plain_and_standard_names_need_evidence",
            path: "src/jar.py",
            lang: Lang::Python,
            source: "class Jar:\n    @property\n    def contents(self):\n        return 1\n\n    @property\n    def name(self):\n        return \"jar\"\n\n    def label(self):\n        return self.name\n\n\ndef inspect(jar, path):\n    return (jar.contents, jar.other, path.name)\n",
            edges: &[("Jar::label", "Jar::name"), ("inspect", "Jar::contents")],
        },
        EdgeCase {
            name: "receiver_typed_through_a_field_or_a_free_function_binds_one_owner",
            path: "src/lib.rs",
            lang: Lang::Rust,
            source: "struct Graph;\nimpl Graph {\n    fn gauges(&self) {}\n}\nstruct Other;\nimpl Other {\n    fn gauges(&self) {}\n}\nstruct Ctx<'a> {\n    graph: &'a Graph,\n}\nmod build {\n    pub fn make() -> super::Graph {\n        super::Graph\n    }\n}\nfn by_field(ctx: &Ctx) {\n    ctx.graph.gauges();\n}\nfn by_path() {\n    build::make().gauges();\n}\n",
            edges: &[
                ("by_field", "Graph::gauges"),
                ("by_path", "Graph::gauges"),
                ("by_path", "build::make"),
            ],
        },
        EdgeCase {
            name: "std_named_call_on_a_field_does_not_bind_to_the_callers_own_method",
            path: "src/lib.rs",
            lang: Lang::Rust,
            source: "struct Heap {\n    items: Vec<u8>,\n}\nimpl Heap {\n    fn is_empty(&self) -> bool {\n        true\n    }\n    fn drained(&self) -> bool {\n        self.items.is_empty()\n    }\n    fn idle(&self) -> bool {\n        self.is_empty()\n    }\n}\n",
            edges: &[("Heap::idle", "Heap::is_empty")],
        },
        EdgeCase {
            name: "path_qualified_call_binds_to_the_type_its_last_segment_names",
            path: "src/lib.rs",
            lang: Lang::Rust,
            source: "mod writer {\n    pub struct Minify;\n    impl Minify {\n        pub fn new() -> Self {\n            Minify\n        }\n    }\n    pub struct Plain;\n    impl Plain {\n        pub fn new() -> Self {\n            Plain\n        }\n    }\n}\n\nfn build() -> writer::Minify {\n    writer::Minify::new()\n}\n",
            edges: &[("build", "Minify::new")],
        },
        EdgeCase {
            name: "receiver_typed_by_its_constructor_binds_only_that_types_method",
            path: "src/lib.rs",
            lang: Lang::Rust,
            source: "struct Minify;\nimpl Minify {\n    fn new() -> Self {\n        Minify\n    }\n    fn flush(&self) {}\n}\n\nstruct Plain;\nimpl Plain {\n    fn flush(&self) {}\n}\n\nfn emit() {\n    let out = Minify::new();\n    out.flush();\n}\n",
            edges: &[("emit", "Minify::flush"), ("emit", "Minify::new")],
        },
    ];
    for case in cases {
        let expected: Vec<(String, String)> = case
            .edges
            .iter()
            .map(|&(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(
            call_edges(case.path, case.lang, case.source),
            expected,
            "{}",
            case.name
        );
    }
}
