use ripwire_rs::{
    FileInfo, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol,
    resolve_all_references,
};

#[test]
fn test_precedence_ladder_same_file() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/a.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let f1 = FileInfo {
        id: 1,
        path: interner.intern("src/b.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let name = interner.intern("target");
    let canon = interner.intern("canon");
    let caller = Symbol::new(
        0,
        interner.intern("caller"),
        canon,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let cand_same_file = Symbol::new(1, name, canon, SymKind::Function, Lang::Rust, 0, 5);
    let cand_other_file = Symbol::new(2, name, canon, SymKind::Function, Lang::Rust, 1, 5);

    let call = Reference::new(0, name, 0, 2, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.push(f0);
    ingest.files.push(f1);
    ingest.symbols.push(caller);
    ingest.symbols.push(cand_same_file);
    ingest.symbols.push(cand_other_file);
    ingest.references.push(call);

    let res = resolve_all_references(&ingest, &interner);
    assert_eq!(res.edge_accumulators.len(), 1);
    assert!(res.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res.edge_accumulators[&(0, 1)].conf_sum, 1.0);
    assert_eq!(res.edge_accumulators[&(0, 1)].n_ref, 1);
    assert_eq!(res.amb_out[0], 0);
}

#[test]
fn test_precedence_ladder_same_dir_and_unique_global() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("pkg/a.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let f1 = FileInfo {
        id: 1,
        path: interner.intern("pkg/b.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let f2 = FileInfo {
        id: 2,
        path: interner.intern("other/c.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };

    let name = interner.intern("shared");
    let canon = interner.intern("canon");
    let caller = Symbol::new(
        0,
        interner.intern("caller"),
        canon,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let cand_same_dir = Symbol::new(1, name, canon, SymKind::Function, Lang::Rust, 1, 5);
    let cand_other_dir = Symbol::new(2, name, canon, SymKind::Function, Lang::Rust, 2, 5);

    let call = Reference::new(0, name, 0, 2, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.extend_from_slice(&[f0, f1, f2]);
    ingest
        .symbols
        .extend_from_slice(&[caller, cand_same_dir, cand_other_dir]);
    ingest.references.push(call);

    let res = resolve_all_references(&ingest, &interner);
    assert_eq!(res.edge_accumulators.len(), 1);
    assert!(res.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res.edge_accumulators[&(0, 1)].conf_sum, 0.5);

    let cand_unique_global = Symbol::new(1, name, canon, SymKind::Function, Lang::Rust, 2, 5);
    let mut ingest_global = IngestResult::new();
    ingest_global.files.extend_from_slice(&[f0, f1, f2]);
    ingest_global
        .symbols
        .extend_from_slice(&[caller, cand_unique_global]);
    ingest_global.references.push(call);

    let res_global = resolve_all_references(&ingest_global, &interner);
    assert_eq!(res_global.edge_accumulators.len(), 1);
    assert!(res_global.edge_accumulators.contains_key(&(0, 1)));
    assert_eq!(res_global.edge_accumulators[&(0, 1)].conf_sum, 0.2);
}

#[test]
fn test_precedence_ladder_non_unique_global_dropped() {
    let interner = Interner::new();
    let f0 = FileInfo {
        id: 0,
        path: interner.intern("src/a.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let f1 = FileInfo {
        id: 1,
        path: interner.intern("dir1/b.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };
    let f2 = FileInfo {
        id: 2,
        path: interner.intern("dir2/c.rs"),
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    };

    let name = interner.intern("common");
    let canon = interner.intern("canon");
    let caller = Symbol::new(
        0,
        interner.intern("caller"),
        canon,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    );
    let cand1 = Symbol::new(1, name, canon, SymKind::Function, Lang::Rust, 1, 5);
    let cand2 = Symbol::new(2, name, canon, SymKind::Function, Lang::Rust, 2, 5);
    let call = Reference::new(0, name, 0, 2, Lang::Rust, RefRole::Call);

    let mut ingest = IngestResult::new();
    ingest.files.extend_from_slice(&[f0, f1, f2]);
    ingest.symbols.extend_from_slice(&[caller, cand1, cand2]);
    ingest.references.push(call);

    let res = resolve_all_references(&ingest, &interner);
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

#[test]
fn test_python_method_named_like_a_container_method_needs_its_type_on_an_unknown_receiver() {
    let py = "class Jar:\n    def get(self, name):\n        return name\n\n    def lookup(self, name):\n        return self.get(name)\n\n\ndef read(headers):\n    return headers.get(\"Host\")\n";
    let edge = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        call_edges("src/jar.py", Lang::Python, py),
        vec![edge("Jar::lookup", "Jar::get")]
    );
}

#[test]
fn test_python_property_reads_bind_to_the_property_but_plain_and_standard_names_need_evidence() {
    let py = "class Jar:\n    @property\n    def contents(self):\n        return 1\n\n    @property\n    def name(self):\n        return \"jar\"\n\n    def label(self):\n        return self.name\n\n\ndef inspect(jar, path):\n    return (jar.contents, jar.other, path.name)\n";
    let edge = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        call_edges("src/jar.py", Lang::Python, py),
        vec![
            edge("Jar::label", "Jar::name"),
            edge("inspect", "Jar::contents")
        ]
    );
}
