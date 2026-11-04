use ripwire_rs::query::retrieval::call_lego;
use ripwire_rs::{
    Graph, Interner, Lang, LanguageRegistry, NO_NODE, RefRole, SymKind, ingest_in_memory_files,
    resolve_all_references,
};

pub fn owner_qualified_callees(files: &[(&str, Lang, &[u8])], caller: &str) -> Vec<String> {
    crate::a_callees::callees_described(files, caller, |ingest, interner, id| {
        let sym = &ingest.symbols[id as usize];
        let owner = sym.scope.map_or("", |s| interner.resolve(&s));
        format!("{owner}.{}", interner.resolve(&sym.name))
    })
}

/// Every resolved `(from, to)` edge among the indexed files, by symbol name, sorted.
pub fn resolved_edges(files: &[(&str, Lang, &[u8])]) -> Vec<(String, String)> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let name = |id: u32| {
        interner
            .resolve(&ingest.symbols[id as usize].name)
            .to_owned()
    };
    let mut edges: Vec<(String, String)> = resolve_all_references(&ingest, &interner)
        .edge_accumulators
        .keys()
        .map(|(from, to)| (name(*from), name(*to)))
        .collect();
    edges.sort();
    edges
}

pub struct ExpectedSym {
    pub name: &'static str,
    pub kind: SymKind,
    pub scope: Option<&'static str>,
    pub has_body_span: bool,
}

pub const fn sym(
    name: &'static str,
    kind: SymKind,
    scope: Option<&'static str>,
    has_body_span: bool,
) -> ExpectedSym {
    ExpectedSym {
        name,
        kind,
        scope,
        has_body_span,
    }
}

#[derive(Default)]
pub struct GraphProtocolCase<'a> {
    pub label: &'a str,
    pub files: &'a [(&'a str, Lang, &'a [u8])],
    pub expected_syms: &'a [ExpectedSym],
    pub rejected_syms: &'a [&'a str],
    pub expected_edges: &'a [(&'a str, &'a str)],
    pub expect_external: bool,
    pub expected_lego: Option<(&'a str, &'a str)>,
    pub expected_import: Option<&'a str>,
}

pub const NO_CASE: GraphProtocolCase<'static> = GraphProtocolCase {
    label: "",
    files: &[],
    expected_syms: &[],
    rejected_syms: &[],
    expected_edges: &[],
    expect_external: false,
    expected_lego: None,
    expected_import: None,
};

fn assert_expected_sym(
    case_label: &str,
    exp: &ExpectedSym,
    ingest: &ripwire_rs::IngestResult,
    interner: &Interner,
) {
    let sym = ingest
        .symbols
        .iter()
        .filter(|s| {
            interner.resolve(&s.name) == exp.name
                && s.kind == exp.kind
                && s.scope.map(|sc| interner.resolve(&sc)) == exp.scope
        })
        .max_by_key(|s| u8::from(s.sig_end_byte < s.end_byte))
        .unwrap_or_else(|| {
            let actual: Vec<_> = ingest
                .symbols
                .iter()
                .map(|s| {
                    (
                        interner.resolve(&s.name),
                        s.kind,
                        s.scope.map(|sc| interner.resolve(&sc)),
                    )
                })
                .collect();
            panic!(
                "[{}] missing expected symbol ({}, {:?}, {:?}); actual={actual:?}",
                case_label, exp.name, exp.kind, exp.scope
            )
        });

    if exp.has_body_span {
        assert!(
            sym.sig_start_byte < sym.sig_end_byte && sym.sig_end_byte < sym.end_byte,
            "[{}] symbol '{}' failed span invariant: sig_start={} sig_end={} end={}",
            case_label,
            exp.name,
            sym.sig_start_byte,
            sym.sig_end_byte,
            sym.end_byte
        );
    }
}

fn assert_expected_edge(
    case_label: &str,
    caller_name: &str,
    callee_name: &str,
    ingest: &ripwire_rs::IngestResult,
    graph: &Graph,
    interner: &Interner,
) {
    let caller = ingest
        .symbols
        .iter()
        .filter(|s| interner.resolve(&s.name) == caller_name)
        .max_by_key(|s| u8::from(s.sig_end_byte < s.end_byte))
        .unwrap_or_else(|| panic!("[{case_label}] missing caller '{caller_name}'"));
    let callee = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == callee_name)
        .unwrap_or_else(|| panic!("[{case_label}] missing callee '{callee_name}'"));

    let attributed = ingest.references.iter().any(|r| {
        r.role == RefRole::Call
            && r.from_symbol != NO_NODE
            && r.from_symbol == caller.id
            && interner.resolve(&r.callee_name) == callee_name
    });
    assert!(
        attributed,
        "[{case_label}] call '{caller_name} -> {callee_name}' was not attributed to caller NodeId {}",
        caller.id
    );

    let has_csr_edge = graph
        .callees(caller.id)
        .any(|(to_id, _)| to_id == callee.id)
        && graph
            .callers(callee.id)
            .any(|(from_id, _)| from_id == caller.id);
    assert!(
        has_csr_edge,
        "[{case_label}] CSR graph missing edge '{caller_name} -> {callee_name}'"
    );
}

pub fn verify_protocol_case(case: &GraphProtocolCase<'_>) {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(case.files, &reg, &interner);
    let resolution = resolve_all_references(&ingest, &interner);
    let graph = Graph::build(&ingest, &interner);

    for exp in case.expected_syms {
        assert_expected_sym(case.label, exp, &ingest, &interner);
    }

    for &rej in case.rejected_syms {
        let present = ingest
            .symbols
            .iter()
            .any(|s| interner.resolve(&s.name) == rej);
        assert!(
            !present,
            "[{}] noise symbol '{rej}' should be excluded",
            case.label
        );
    }

    for &(caller, callee) in case.expected_edges {
        assert_expected_edge(case.label, caller, callee, &ingest, &graph, &interner);
    }

    if case.expect_external {
        assert!(
            resolution.external >= 1,
            "[{}] expected >= 1 external stdlib call, got external={}",
            case.label,
            resolution.external
        );
    }

    if let Some((iface, impl_name)) = case.expected_lego {
        let lego = call_lego(&ingest, &interner, iface);
        let impls = lego["implementors"].as_array().expect("implementors array");
        assert!(
            impls.iter().any(|i| i["name"] == impl_name),
            "[{}] --lego({}) missing implementor '{}', got {impls:?}",
            case.label,
            iface,
            impl_name
        );
    }

    if let Some(imp_sub) = case.expected_import {
        let has_imp = ingest.references.iter().any(|r| {
            r.role == RefRole::Import && interner.resolve(&r.callee_name).contains(imp_sub)
        });
        assert!(
            has_imp,
            "[{}] missing RefRole::Import containing '{imp_sub}'",
            case.label
        );
    }
}

/// A call on a factory's result binds only to the class the factory's declared return type names.
pub fn assert_call_result_binds_factory_return(lang: Lang, path: &str, source: &str) {
    let callees = owner_qualified_callees(&[(path, lang, source.as_bytes())], "run");
    let opens: Vec<&str> = callees
        .iter()
        .map(String::as_str)
        .filter(|callee| callee.ends_with(".open"))
        .collect();
    assert_eq!(opens, ["DiskStore.open"], "{callees:?}");
}
