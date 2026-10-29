use ripwire_rs::gate::baseline::{QualitySnapshot, compute_snapshot};
use ripwire_rs::gate::quality_delta::{
    QualityDeltaReport, QualityRegression, compute_quality_delta_with_acks, parse_acks_content,
};
use ripwire_rs::gate::rename::attach_body_fingerprints;
use ripwire_rs::{FileInfo, Graph, IngestResult, Interner, Lang, SymKind, Symbol, ToXmlString};

const PATH: &str = "src/lib.rs";

/// One function in a fixture file. `body` holds the statements between the braces.
struct Func {
    name: &'static str,
    kind: SymKind,
    params: u16,
    ccx: u32,
    body: String,
}

fn func(name: &'static str, ccx: u32, body: String) -> Func {
    Func {
        name,
        kind: SymKind::Function,
        params: 1,
        ccx,
        body,
    }
}

/// `count` one-line branches. Their normalized tokens differ only in the numbers, so a function with
/// 24 branches and one with 34 are close Type-3 clones of each other.
fn branches(count: u32) -> String {
    let mut body = String::from("    let mut acc = 0;\n");
    for n in 1..=count {
        body.push_str(&format!("    if x == {n} {{ return {n}; }}\n"));
    }
    body.push_str("    acc\n");
    body
}

fn unrelated_body() -> String {
    String::from(
        "    let mut total = 0;\n    for i in 0..n {\n        total += i * 3;\n        total -= i / 2;\n    }\n    total\n",
    )
}

/// Builds the ingest for one file holding `funcs` and returns it with the file's source text.
fn ingest_file(interner: &Interner, funcs: &[Func]) -> (IngestResult, String) {
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern(PATH),
        lang: Lang::Rust,
        line_count: 0,
        byte_size: 0,
    });
    let mut source = String::new();
    for (id, f) in funcs.iter().enumerate() {
        let start = source.len();
        let params: Vec<String> = (0..f.params).map(|i| format!("x{i}: i32")).collect();
        source.push_str(&format!(
            "pub fn {}({}) -> i32 {{",
            f.name,
            params.join(", ")
        ));
        let sig_end = source.len();
        source.push_str(&f.body);
        source.push_str("}\n");
        let name = interner.intern(f.name);
        let mut sym = Symbol::new(id as u32, name, name, f.kind, Lang::Rust, 0, 1);
        sym.sig_start_byte = start as u32;
        sym.sig_end_byte = sig_end as u32;
        sym.end_byte = source.len() as u32;
        sym.ccx = f.ccx;
        sym.params = f.params;
        ingest.symbols.push(sym);
    }
    (ingest, source)
}

/// A quality snapshot of one file with its changed callables fingerprinted, as the delta gate builds it.
fn snapshot(interner: &Interner, funcs: &[Func], head: &str) -> QualitySnapshot {
    let (ingest, source) = ingest_file(interner, funcs);
    let graph = Graph::build(&ingest, interner);
    let mut snap = compute_snapshot(&ingest, &graph, interner, head);
    let changed = [PATH.to_string()];
    attach_body_fingerprints(
        &mut snap,
        &ingest,
        interner,
        &changed,
        &[(PATH, source.as_bytes())].as_slice(),
    );
    snap
}

fn delta(base: &QualitySnapshot, work: &QualitySnapshot, acks: Option<&str>) -> QualityDeltaReport {
    let ledger = acks.map(parse_acks_content);
    compute_quality_delta_with_acks(work, base, "git-HEAD", ledger.as_ref())
}

/// The complexity rows of a report. Unpaired new functions also raise `dead-code` rows, which these tests ignore.
fn complexity_rows(report: &QualityDeltaReport) -> Vec<&QualityRegression> {
    report
        .regressions
        .iter()
        .filter(|r| r.kind == "complexity")
        .collect()
}

#[test]
fn rename_with_added_branches_gates_against_the_old_values() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    let work = snapshot(&interner, &[func("compute_sum", 34, branches(34))], "work");

    let report = delta(&base, &work, None);

    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 1);
    let row = rows[0];
    assert_eq!(row.sym_name, "compute_sum");
    assert_eq!((row.was_val, row.now_val), (24, 34));
    assert!(row.is_gating);
    assert!(!row.is_new_symbol);
    assert_eq!(row.renamed_from.as_deref(), Some("compute_total"));
    assert_eq!(report.renames, 1);
    assert!(report.has_gating_failures());
    assert!(
        report
            .to_xml_string()
            .contains(r#"renamed_from="compute_total""#)
    );
}

#[test]
fn pure_rename_without_worsening_yields_no_row_and_carries_its_ack() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    let work = snapshot(&interner, &[func("compute_sum", 24, branches(24))], "work");

    let report = delta(&base, &work, Some("ack complexity compute_total 24\n"));

    assert!(report.regressions.is_empty());
    assert!(report.stale_acks.is_empty());
    assert!(!report.has_gating_failures());
    assert_eq!(report.renames, 1);
    assert_eq!(report.acked_by_rename, 1);
    let xml = report.to_xml_string();
    assert!(xml.contains(r#"renames="1""#));
    assert!(xml.contains(r#"acked_by_rename="1""#));
    assert!(!xml.contains("<r kind=\""));
}

#[test]
fn ack_keyed_by_old_name_acknowledges_a_renamed_regression() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    let work = snapshot(&interner, &[func("compute_sum", 34, branches(34))], "work");

    let report = delta(&base, &work, Some("ack complexity compute_total 34\n"));

    assert!(complexity_rows(&report).is_empty());
    assert_eq!(report.acked_count, 1);
    assert!(!report.has_gating_failures());
    assert!(report.stale_acks.is_empty());
    assert_eq!(report.acked_by_rename, 1);
}

#[test]
fn identical_bodies_that_tie_for_one_removed_symbol_are_not_paired() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 10, branches(24))],
        "base",
    );
    let work = snapshot(
        &interner,
        &[
            func("compute_a", 34, branches(24)),
            func("compute_b", 34, branches(24)),
        ],
        "work",
    );

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 0);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.is_new_symbol && r.renamed_from.is_none())
    );
}

#[test]
fn different_kind_is_not_paired() {
    let interner = Interner::new();
    let method = Func {
        kind: SymKind::Method,
        ..func("compute_total", 24, branches(24))
    };
    let base = snapshot(&interner, &[method], "base");
    let work = snapshot(&interner, &[func("compute_sum", 34, branches(34))], "work");

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 0);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_new_symbol);
    assert!(rows[0].renamed_from.is_none());
}

#[test]
fn different_arity_is_not_paired() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    let two_params = Func {
        params: 2,
        ..func("compute_sum", 34, branches(34))
    };
    let work = snapshot(&interner, &[two_params], "work");

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 0);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_new_symbol);
    assert!(rows[0].renamed_from.is_none());
}

#[test]
fn body_below_the_clone_floor_is_not_paired() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 10, branches(24))],
        "base",
    );
    let work = snapshot(
        &interner,
        &[func("compute_other", 34, unrelated_body())],
        "work",
    );

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 0);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_new_symbol);
    assert!(rows[0].renamed_from.is_none());
}

#[test]
fn unrelated_new_symbol_stays_new_beside_a_paired_rename() {
    let interner = Interner::new();
    let base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    let work = snapshot(
        &interner,
        &[
            func("compute_sum", 34, branches(34)),
            func("helper", 34, unrelated_body()),
        ],
        "work",
    );

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 1);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 2);
    let helper = rows
        .iter()
        .find(|r| r.sym_name == "helper")
        .expect("helper row");
    assert!(helper.is_new_symbol);
    assert!(helper.renamed_from.is_none());
    let sum = rows
        .iter()
        .find(|r| r.sym_name == "compute_sum")
        .expect("compute_sum row");
    assert_eq!(sum.renamed_from.as_deref(), Some("compute_total"));
}

#[test]
fn pinned_sidecar_baseline_without_fingerprints_pairs_no_rename() {
    let interner = Interner::new();
    let mut base = snapshot(
        &interner,
        &[func("compute_total", 24, branches(24))],
        "base",
    );
    base.body_by_sym.clear();
    let work = snapshot(&interner, &[func("compute_sum", 34, branches(34))], "work");

    let report = delta(&base, &work, None);

    assert_eq!(report.renames, 0);
    let rows = complexity_rows(&report);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_new_symbol);
}

#[test]
fn test_same_text_ignores_node_ids_across_separately_built_indexes() {
    let base = snapshot(&Interner::new(), &[func("kept", 20, branches(24))], "base");
    let work = snapshot(
        &Interner::new(),
        &[
            func("added", 1, unrelated_body()),
            func("kept", 20, branches(24)),
        ],
        "work",
    );
    let edited = snapshot(&Interner::new(), &[func("kept", 20, branches(25))], "work");
    let key = format!("{PATH}::kept");
    let same = |a: &QualitySnapshot, b: &QualitySnapshot| {
        a.body_by_sym[&key].same_text(&b.body_by_sym[&key])
    };
    assert_eq!((same(&base, &work), same(&base, &edited)), (true, false));
}
