use crate::b_ingest::one_function_ingest;
use ripwire_rs::metrics::{
    classify_identifier_casing, compute_comment_coherence_with_sources,
    serialize_executive_report_md, serialize_naming_consistency_xml,
};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{FileInfo, Graph, IngestResult, Interner, Lang, SymKind, Symbol};

#[test]
fn test_naming_and_executive_report() {
    assert_eq!(
        classify_identifier_casing("MAX_SIZE"),
        "SCREAMING_SNAKE_CASE"
    );
    assert_eq!(classify_identifier_casing("my_func"), "snake_case");
    assert_eq!(classify_identifier_casing("MyClass"), "PascalCase");
    assert_eq!(classify_identifier_casing("myVar"), "camelCase");
    assert_eq!(classify_identifier_casing("run"), "lower");
    assert_eq!(classify_identifier_casing("___"), "other");

    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let name_spur = interner.intern("my_func");
    let path_spur = interner.intern("src/lib.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: path_spur,
        lang: Lang::Rust,
        line_count: 10,
        byte_size: 100,
    });
    ingest.symbols.push(Symbol::new(
        0,
        name_spur,
        name_spur,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    ));
    let graph = Graph::build(&ingest, &interner);
    let mut buf = Vec::new();
    serialize_naming_consistency_xml(".", &ingest, &interner, PageRequest::default(), &mut buf)
        .unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("snake_case"));
    buf.clear();
    let facts = ripwire_rs::query::safety::SafetyContext {
        root: ".",
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
    };
    serialize_executive_report_md(&facts, &[1.0], PageRequest::default(), &mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf).contains("# Architecture & Quality Report"));
}

#[test]
fn test_compute_comment_coherence_with_sources() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();

    // File 1: Rust doc comments with attributes
    let rust_file = interner.intern("src/calc.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path: rust_file,
        lang: Lang::Rust,
        line_count: 20,
        byte_size: 200,
    });
    let sym_rust_name = interner.intern("compute_total");
    let mut sym_rust = Symbol::new(
        0,
        sym_rust_name,
        sym_rust_name,
        SymKind::Function,
        Lang::Rust,
        0,
        4,
    );
    let rust_src = b"/// compute the total sum\n#[inline]\n#[must_use]\n\nfn compute_total() {}\n";
    let rust_needle = b"fn compute_total(";
    sym_rust.sig_start_byte = rust_src
        .windows(rust_needle.len())
        .position(|w| w == rust_needle)
        .unwrap() as u32;
    sym_rust.loc = 5;
    ingest.symbols.push(sym_rust);

    // File 2: Python doc comments with # and decorator
    let py_file = interner.intern("src/metrics.py");
    ingest.files.push(FileInfo {
        id: 1,
        path: py_file,
        lang: Lang::Python,
        line_count: 20,
        byte_size: 200,
    });
    let sym_py_name = interner.intern("calculate_metrics");
    let mut sym_py = Symbol::new(
        1,
        sym_py_name,
        sym_py_name,
        SymKind::Function,
        Lang::Python,
        1,
        4,
    );
    let py_src =
        b"# calculate and export metrics report\n@property\ndef calculate_metrics(): pass\n";
    let py_needle = b"def calculate_metrics(";
    sym_py.sig_start_byte = py_src
        .windows(py_needle.len())
        .position(|w| w == py_needle)
        .unwrap() as u32;
    sym_py.loc = 4;
    ingest.symbols.push(sym_py);

    // File 3: C-style block comments and multi-function boundary check
    let c_file = interner.intern("src/math.c");
    ingest.files.push(FileInfo {
        id: 2,
        path: c_file,
        lang: Lang::C,
        line_count: 30,
        byte_size: 300,
    });
    let sym_c_name = interner.intern("matrix_multiply");
    let mut sym_c = Symbol::new(2, sym_c_name, sym_c_name, SymKind::Function, Lang::C, 2, 6);
    let c_src = b"void previous_step() {}\n\n/*\n * matrix multiply algorithm\n */\nvoid matrix_multiply() {}\n";
    let c_needle = b"void matrix_multiply";
    sym_c.sig_start_byte = c_src
        .windows(c_needle.len())
        .position(|w| w == c_needle)
        .unwrap() as u32;
    sym_c.loc = 6;
    ingest.symbols.push(sym_c);

    // Undocumented function in File 1
    let sym_undoc_name = interner.intern("undocumented_helper");
    let mut sym_undoc = Symbol::new(
        0,
        sym_undoc_name,
        sym_undoc_name,
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    );
    sym_undoc.sig_start_byte = 0;
    sym_undoc.loc = 3;
    ingest.symbols.push(sym_undoc);

    let sources: &[(&str, &[u8])] = &[
        ("src/calc.rs", rust_src),
        ("src/metrics.py", py_src),
        ("src/math.c", c_src),
    ];

    let report =
        compute_comment_coherence_with_sources(sources, &ingest, &interner, PageRequest::default());
    assert_eq!(report.documented, 3);
    assert_eq!(report.no_comment, 1);
    assert_eq!(report.rows.len(), 3);

    // Verify compute_total: ["compute", "the", "total", "sum"] (4 words, 2 restated)
    let rust_row = report
        .rows
        .iter()
        .find(|r| r.name == "compute_total")
        .unwrap();
    assert_eq!(rust_row.words, 4);
    assert_eq!(rust_row.c_terms, 4);
    assert_eq!(rust_row.restate, 2);
    assert!((rust_row.c_coeff - 0.5).abs() < 1e-4);

    // Verify calculate_metrics: ["calculate", "and", "export", "metrics", "report"] (5 words, 2 restated)
    let py_row = report
        .rows
        .iter()
        .find(|r| r.name == "calculate_metrics")
        .unwrap();
    assert_eq!(py_row.words, 5);
    assert_eq!(py_row.c_terms, 5);
    assert_eq!(py_row.restate, 2);
    assert!((py_row.c_coeff - 0.4).abs() < 1e-4);

    // Verify matrix_multiply: boundary stopped at previous_step(), words: ["matrix", "multiply", "algorithm"] (3 words, 2 restated)
    let c_row = report
        .rows
        .iter()
        .find(|r| r.name == "matrix_multiply")
        .unwrap();
    assert_eq!(c_row.words, 3);
    assert_eq!(c_row.c_terms, 3);
    assert_eq!(c_row.restate, 2);
    assert!((c_row.c_coeff - (2.0 / 3.0)).abs() < 1e-4);
}

#[test]
fn test_comment_coherence_without_source_bytes_has_no_row() {
    let interner = Interner::new();
    let ingest = one_function_ingest(&interner, "src/users.rs", "load_user_account", 20, 200);
    let sources: &[(&str, &[u8])] = &[];
    let report =
        compute_comment_coherence_with_sources(sources, &ingest, &interner, PageRequest::default());
    assert_eq!(report.documented, 0);
    assert_eq!(report.no_comment, 1);
    assert!(report.rows.is_empty());
}

#[test]
fn test_comment_coherence_ignores_module_docs_and_bodyless_signatures() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let src = b"//! module overview words here\n\nfn plain() {}\n\n/// plain helper words\nfn decl();\n\n/// real doc words here\nfn documented() {}\n";
    let path = interner.intern("src/lib.rs");
    ingest.files.push(FileInfo {
        id: 0,
        path,
        lang: Lang::Rust,
        line_count: 9,
        byte_size: src.len() as u64,
    });
    let find = |needle: &[u8]| src.windows(needle.len()).position(|w| w == needle).unwrap() as u32;
    let plain = interner.intern("plain");
    let mut sym_plain = Symbol::new(0, plain, plain, SymKind::Function, Lang::Rust, 0, 4);
    sym_plain.sig_start_byte = find(b"fn plain");
    sym_plain.sig_end_byte = find(b"fn plain") + 8;
    sym_plain.end_byte = find(b"fn plain") + 12;
    let decl = interner.intern("decl");
    let mut sym_decl = Symbol::new(1, decl, decl, SymKind::Function, Lang::Rust, 0, 6);
    sym_decl.sig_start_byte = find(b"fn decl");
    sym_decl.sig_end_byte = find(b"fn decl") + 7;
    sym_decl.end_byte = sym_decl.sig_end_byte;
    let documented = interner.intern("documented");
    let mut sym_doc = Symbol::new(
        2,
        documented,
        documented,
        SymKind::Function,
        Lang::Rust,
        0,
        8,
    );
    sym_doc.sig_start_byte = find(b"fn documented");
    sym_doc.sig_end_byte = find(b"fn documented") + 13;
    sym_doc.end_byte = sym_doc.sig_end_byte + 3;
    ingest.symbols.extend([sym_plain, sym_decl, sym_doc]);

    let sources: &[(&str, &[u8])] = &[("src/lib.rs", src)];
    let report =
        compute_comment_coherence_with_sources(sources, &ingest, &interner, PageRequest::default());
    let names: Vec<&str> = report.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["documented"], "{names:?}");
    assert_eq!(report.documented, 1);
}
