use ripwire_rs::metrics::nonlocal::{
    NonlocalFunction, StateCellAccess, serialize_nonlocal_state_xml,
};

#[test]
fn test_serialize_nonlocal_state_rows_and_cells() {
    let funcs = vec![NonlocalFunction {
        sym: 1,
        sym_name: "mutate_global".to_string(),
        path: "src/state.rs".to_string(),
        line: 10,
        writes: 1,
        reads: 1,
        cells: vec![
            StateCellAccess {
                cell_name: "GLOBAL_COUNTER".to_string(),
                path: "src/state.rs".to_string(),
                line: 3,
                is_write: true,
            },
            StateCellAccess {
                cell_name: "CONFIG".to_string(),
                path: "src/state.rs".to_string(),
                line: 4,
                is_write: false,
            },
        ],
    }];
    let mut out = Vec::new();
    serialize_nonlocal_state_xml(".", &funcs, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("cells=\"2\" functions=\"1\""));
    assert!(xml.contains("<cell n=\"GLOBAL_COUNTER\" p=\"src/state.rs:3\" dir=\"w\"/>"));
    assert!(xml.contains("<cell n=\"CONFIG\" p=\"src/state.rs:4\" dir=\"r\"/>"));
}

#[test]
fn nonlocal_state_report_honors_limit_and_offset() {
    use ripwire_rs::graph::GraphGauges;
    use ripwire_rs::metrics::nonlocal::NonlocalStateReport;
    use ripwire_rs::serialize::{PageRequest, XmlReport};
    let func = |name: &str| NonlocalFunction {
        sym: 1,
        sym_name: name.to_string(),
        path: "src/state.rs".to_string(),
        line: 1,
        writes: 0,
        reads: 1,
        cells: vec![StateCellAccess {
            cell_name: "CONFIG".to_string(),
            path: "src/state.rs".to_string(),
            line: 2,
            is_write: false,
        }],
    };
    let funcs = vec![func("a"), func("b"), func("c")];
    let render = |page: PageRequest| {
        let report = NonlocalStateReport {
            funcs: &funcs,
            gauges: GraphGauges::default(),
            page,
        };
        let mut out = Vec::new();
        report.write_xml_report(".", &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };
    let all = render(PageRequest::default());
    assert!(all.contains("shown=\"3\" capped=\"0\"") && all.matches("<fn ").count() == 3);
    let page = render(PageRequest {
        limit: Some(1),
        offset: Some(1),
        ..PageRequest::default()
    });
    assert_eq!(page.matches("<fn ").count(), 1);
    assert!(
        page.contains(" n=\"b\" ")
            && page.contains("shown=\"1\"")
            && page.contains("has_more=\"1\"")
    );
    let tiny = render(PageRequest {
        budget: Some(1),
        ..PageRequest::default()
    });
    assert_eq!(tiny.matches("<fn ").count(), 1);
    assert!(tiny.contains("has_more=\"1\""));
    let roomy = render(PageRequest {
        budget: Some(usize::MAX),
        ..PageRequest::default()
    });
    assert_eq!(roomy.matches("<fn ").count(), 3);
}

#[test]
fn state_accesses_remain_visible_without_call_edges() {
    use ripwire_rs::{
        FileInfo, Graph, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol,
    };
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    for (id, path) in ["src/state.rs", "src/app.rs"].into_iter().enumerate() {
        ingest.files.push(FileInfo {
            id: id as u32,
            path: interner.intern(path),
            lang: Lang::Rust,
            line_count: 10,
            byte_size: 100,
        });
    }
    let counter = interner.intern("COUNTER");
    let update = interner.intern("update");
    ingest.symbols.push(Symbol::new(
        0,
        counter,
        counter,
        SymKind::Var,
        Lang::Rust,
        0,
        2,
    ));
    ingest.symbols.push(Symbol::new(
        1,
        update,
        update,
        SymKind::Function,
        Lang::Rust,
        1,
        1,
    ));
    for (role, line) in [(RefRole::Read, 3), (RefRole::Read, 4), (RefRole::Write, 5)] {
        ingest
            .references
            .push(Reference::new(1, counter, 1, line, Lang::Rust, role));
    }
    let graph = Graph::build(&ingest, &interner);
    assert_eq!(graph.out_degree(1), 0);
    let report = ripwire_rs::metrics::find_nonlocal_state(&ingest, &graph, &interner);
    assert_eq!(report.len(), 1);
    assert_eq!((report[0].reads, report[0].writes), (1, 1));
    assert_eq!(report[0].cells.len(), 2);
    assert!(
        report[0]
            .cells
            .iter()
            .all(|cell| cell.path == "src/state.rs" && cell.line == 2)
    );
}

const RUST_SOURCE: &str = "use std::cell::Cell;\n\
use std::sync::atomic::{AtomicU64, Ordering};\n\
use std::sync::{LazyLock, OnceLock};\n\
const LIMIT: i32 = 3;\n\
static PLAIN: i32 = 4;\n\
static CELL: Cell<i32> = Cell::new(0);\n\
static mut RAW: i32 = 0;\n\
static LAZY: LazyLock<Vec<i32>> = LazyLock::new(|| vec![1]);\n\
static SLOT: OnceLock<i32> = OnceLock::new();\n\
static HITS: AtomicU64 = AtomicU64::new(0);\n\
fn read_all() -> i32 {\n\
    HITS.fetch_add(1, Ordering::Relaxed);\n\
    LIMIT + PLAIN + unsafe { RAW } + CELL.get() + LAZY.len() as i32 + *SLOT.get().unwrap()\n\
}\n";

fn rust_ingest(source: &str) -> (ripwire_rs::IngestResult, ripwire_rs::Interner) {
    use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[("src/state.rs", Lang::Rust, source.as_bytes())],
        &registry,
        &interner,
    );
    (ingest, interner)
}

#[test]
fn rust_const_and_static_items_classify_by_mutability() {
    use ripwire_rs::CorpusView;
    let (ingest, interner) = rust_ingest(RUST_SOURCE);
    let view = (&ingest, &interner);
    let immutable = |name: &str| {
        ingest
            .symbols
            .iter()
            .find(|s| view.sym_name(s) == name)
            .unwrap()
            .binding
            == ripwire_rs::domain::Binding::Frozen
    };
    assert!(immutable("LIMIT"), "const is never a cell");
    assert!(
        immutable("PLAIN"),
        "plain non-mut static of a plain type is not mutable"
    );
    assert!(
        !immutable("CELL"),
        "static of an interior-mutability type is a cell"
    );
    assert!(!immutable("RAW"), "static mut is a cell");
    assert!(
        immutable("LAZY"),
        "a LazyLock is initialised once from a fixed expression and never changes"
    );
    assert!(
        !immutable("SLOT"),
        "a OnceLock is set at runtime, so it is a cell"
    );
}

#[test]
fn rust_nonlocal_state_counts_only_mutable_statics() {
    use ripwire_rs::Graph;
    let (ingest, interner) = rust_ingest(RUST_SOURCE);
    let graph = Graph::build(&ingest, &interner);
    let funcs = ripwire_rs::metrics::find_nonlocal_state(&ingest, &graph, &interner);
    let func = funcs.iter().find(|f| f.sym_name == "read_all").unwrap();
    let mut names: Vec<&str> = func.cells.iter().map(|c| c.cell_name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["CELL", "HITS", "RAW", "SLOT"]);
}
