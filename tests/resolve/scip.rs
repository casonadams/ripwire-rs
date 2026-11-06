use protobuf::Message;
use ripwire_rs::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use ripwire_rs::resolve::scip_sites;
use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files,
};
use scip::types::occurrence::Typed_range;
use scip::types::{Document, Index, MultiLineRange, Occurrence, SingleLineRange, SymbolRole};

const SOURCES: &[(&str, Lang, &[u8])] = &[
    (
        "a/x.py",
        Lang::Python,
        b"class X:\n    def run(self):\n        return 1\n",
    ),
    (
        "b/y.py",
        Lang::Python,
        b"class Y:\n    def run(self):\n        return 2\n",
    ),
    (
        "c/use.py",
        Lang::Python,
        b"def go(o):\n    return o.run()\n\ndef ext():\n    return run()\n",
    ),
];

#[derive(Clone, Copy)]
enum Form {
    Packed,
    SingleLine,
    MultiLine,
}

fn occurrence(symbol: &str, line: i32, definition: bool) -> Occurrence {
    occurrence_in(Form::Packed, symbol, line, definition)
}

fn occurrence_in(form: Form, symbol: &str, line: i32, definition: bool) -> Occurrence {
    let mut occ = Occurrence {
        symbol: symbol.to_string(),
        symbol_roles: if definition {
            SymbolRole::Definition as i32
        } else {
            0
        },
        ..Occurrence::default()
    };
    match form {
        Form::Packed => occ.range = vec![line, 4, 7],
        Form::SingleLine => {
            occ.typed_range = Some(Typed_range::SingleLineRange(SingleLineRange {
                line,
                start_character: 4,
                end_character: 7,
                ..SingleLineRange::default()
            }));
        }
        Form::MultiLine => {
            occ.typed_range = Some(Typed_range::MultiLineRange(MultiLineRange {
                start_line: line,
                start_character: 4,
                end_line: line + 1,
                end_character: 7,
                ..MultiLineRange::default()
            }));
        }
    }
    occ
}

fn document(path: &str, occurrences: Vec<Occurrence>) -> Document {
    Document {
        relative_path: path.to_string(),
        occurrences,
        ..Document::default()
    }
}

const X_RUN: &str = "scip-python python app 1.0 a/x/run().";
const Y_RUN: &str = "scip-python python app 1.0 b/y/run().";
const LIB_RUN: &str = "scip-python python lib 2.0 lib/run().";

fn index_bytes() -> Vec<u8> {
    index_with(Form::Packed)
}

fn index_with(form: Form) -> Vec<u8> {
    Index {
        documents: vec![
            document("a/x.py", vec![occurrence_in(form, X_RUN, 1, true)]),
            document("b/y.py", vec![occurrence_in(form, Y_RUN, 1, true)]),
            document(
                "c/use.py",
                vec![
                    occurrence_in(form, Y_RUN, 1, false),
                    occurrence_in(form, LIB_RUN, 4, false),
                ],
            ),
        ],
        ..Index::default()
    }
    .write_to_bytes()
    .unwrap()
}

fn callees(sym: &str, scip: Option<&[u8]>) -> String {
    crate::support::nav::nav_xml(SOURCES, scip, sym, NavMode::Callees).unwrap()
}

#[test]
fn scip_index_pins_call_sites_the_name_ladder_declines() {
    let without = callees("go", None);
    assert!(without.contains("declined_calls=\"1\""), "{without}");
    assert_pins_run(&index_bytes());
}

fn assert_pins_run(bytes: &[u8]) {
    let with = callees("go", Some(bytes));
    assert!(
        with.contains("<s t=\"fn\" n=\"run\" p=\"b/y.py:2\" prov=\"scip\"/>"),
        "{with}"
    );
    assert!(!with.contains("a/x.py"), "{with}");
    assert!(!with.contains("declined_calls=\""), "{with}");
}

#[test]
fn scip_symbol_defined_outside_the_index_is_external() {
    let bytes = index_bytes();
    let with = callees("ext", Some(&bytes));
    assert!(with.contains("count=\"0\""), "{with}");
    assert!(!with.contains("declined_calls=\""), "{with}");
}

#[test]
fn non_scip_bytes_are_rejected() {
    let (ingest, _, interner) = crate::support::index::index_files(SOURCES);
    assert!(scip_sites(b"\xff\xff\xff\xff", &ingest, &interner).is_err());
}

fn scip_ingest(
    sources: &[(&str, Lang, &[u8])],
    documents: Vec<Document>,
) -> (IngestResult, Interner) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let mut ingest = ingest_in_memory_files(sources, &registry, &interner);
    let bytes = Index {
        documents,
        ..Index::default()
    }
    .write_to_bytes()
    .unwrap();
    ingest.scip_sites = scip_sites(&bytes, &ingest, &interner).unwrap();
    (ingest, interner)
}

#[test]
fn multiline_member_calls_use_identifier_lines_without_moving_reported_sites() {
    let sources: &[(&str, Lang, &[u8])] = &[
        SOURCES[0],
        SOURCES[1],
        (
            "c/use.py",
            Lang::Python,
            b"def go(o):\n    return (o\n        .run())\n",
        ),
    ];
    let (ingest, interner) = scip_ingest(
        sources,
        vec![
            document("b/y.py", vec![occurrence(Y_RUN, 1, true)]),
            document("c/use.py", vec![occurrence(Y_RUN, 2, false)]),
        ],
    );
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    let xml = crate::support::nav::nav_report(
        &ingest,
        &graph,
        &ranks,
        &interner,
        "run",
        NavMode::Callers,
    )
    .unwrap();
    assert!(
        xml.contains("n=\"go\" p=\"c/use.py:1\" of_def=\"2\" sites_at=\"c/use.py:2\""),
        "{xml}"
    );
    assert!(!xml.contains("declined_calls=\""), "{xml}");
    assert!(
        graph
            .out_prov
            .contains(&ripwire_rs::resolve::EdgeProv::Scip)
    );
}

#[test]
fn shared_navigation_reports_keep_scip_groups_and_unbound_sites_with_paging() {
    let sources: &[(&str, Lang, &[u8])] = &[
        SOURCES[0],
        SOURCES[1],
        (
            "c/use.py",
            Lang::Python,
            b"def go(o):\n    return o.run()\n\ndef unpinned(o):\n    return o.run()\n",
        ),
    ];
    let (ingest, interner) = scip_ingest(
        sources,
        vec![
            document("a/x.py", vec![occurrence(X_RUN, 1, true)]),
            document("b/y.py", vec![occurrence(Y_RUN, 1, true)]),
            document("c/use.py", vec![occurrence(Y_RUN, 1, false)]),
        ],
    );
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    let mut ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &ranks,
        interner: &interner,
        page: ripwire_rs::serialize::PageRequest {
            limit: Some(1),
            offset: Some(0),
            budget: None,
        },
        max_depth: None,
        pr_iters: None,
    };
    let render = |sym, mode, ctx: &NavContext<'_>| {
        let mut out = Vec::new();
        execute_nav_symbol_query_in_root("repo&root", sym, mode, ctx, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };

    let callers = render("run", NavMode::Callers, &ctx);
    assert!(callers.contains("defs=\"2\" count=\"1\""), "{callers}");
    assert!(callers.contains("root=\"repo&amp;root\""), "{callers}");
    assert!(callers.contains("<def ix=\"1\""), "{callers}");
    assert!(callers.contains("<def ix=\"2\""), "{callers}");
    assert!(
        callers.contains("n=\"go\" p=\"c/use.py:1\" of_def=\"2\" sites_at=\"c/use.py:2\""),
        "{callers}"
    );
    assert!(
        callers.contains(
            "<dc in=\"unpinned\" p=\"c/use.py:5\" cands=\"2\" via=\"declined\" of_def=\"1~d 2~d\"/>"
        ),
        "{callers}"
    );
    let callees = render("go", NavMode::Callees, &ctx);
    assert!(
        callees.contains("<s t=\"fn\" n=\"run\" p=\"b/y.py:2\" prov=\"scip\"/>"),
        "{callees}"
    );
    let unpinned = render("unpinned", NavMode::Callees, &ctx);
    assert!(unpinned.contains("count=\"0\""), "{unpinned}");
    assert!(unpinned.contains("declined_calls=\"1\""), "{unpinned}");

    let found = render("run", NavMode::Callers, &ctx);
    assert!(found.contains("root=\"repo&amp;root\""), "{found}");
    assert!(found.contains("defs=\"2\""), "{found}");
    assert!(
        found.contains("<def ix=\"1\" t=\"fn\" sc=\"X\" p=\"a/x.py:2\"/>"),
        "{found}"
    );
    assert!(
        found.contains("<def ix=\"2\" t=\"fn\" sc=\"Y\" p=\"b/y.py:2\"/>"),
        "{found}"
    );
    assert!(
        found.contains("n=\"go\" p=\"c/use.py:1\" of_def=\"2\" sites_at=\"c/use.py:2\""),
        "{found}"
    );
    assert!(
        found.contains(
            "<dc in=\"unpinned\" p=\"c/use.py:5\" cands=\"2\" via=\"declined\" of_def=\"1~d 2~d\"/>"
        ),
        "{found}"
    );
    let found_go = render("go", NavMode::Callees, &ctx);
    assert!(
        found_go.contains("<s t=\"fn\" n=\"run\" p=\"b/y.py:2\" prov=\"scip\"/>"),
        "{found_go}"
    );

    ctx.page.offset = Some(1);
    let later = render("run", NavMode::Callers, &ctx);
    assert!(!later.contains("<s t="), "{later}");
    assert!(later.contains("<def ix=\"2\""), "{later}");
    assert!(later.contains("<dc in=\"unpinned\""), "{later}");
    assert!(later.contains("offset=\"1\" limit=\"1\""), "{later}");
}

#[test]
fn every_range_form_pins_the_same_sites() {
    for form in [Form::Packed, Form::SingleLine, Form::MultiLine] {
        assert_pins_run(&index_with(form));
    }
}

#[test]
fn typed_range_takes_precedence_over_packed_range() {
    let bytes = Index {
        documents: vec![
            document("a/x.py", vec![occurrence(X_RUN, 1, true)]),
            document("b/y.py", vec![occurrence(Y_RUN, 1, true)]),
            document(
                "c/use.py",
                vec![Occurrence {
                    range: vec![40, 4, 7],
                    ..occurrence_in(Form::SingleLine, Y_RUN, 1, false)
                }],
            ),
        ],
        ..Index::default()
    }
    .write_to_bytes()
    .unwrap();
    let with = callees("go", Some(&bytes));
    assert!(
        with.contains("<s t=\"fn\" n=\"run\" p=\"b/y.py:2\" prov=\"scip\"/>"),
        "{with}"
    );
}

#[test]
fn index_whose_occurrences_have_no_readable_range_is_an_error() {
    let (ingest, _, interner) = crate::support::index::index_files(SOURCES);
    let bytes = Index {
        documents: vec![document(
            "c/use.py",
            vec![Occurrence {
                symbol: Y_RUN.to_string(),
                ..Occurrence::default()
            }],
        )],
        ..Index::default()
    }
    .write_to_bytes()
    .unwrap();
    let err = scip_sites(&bytes, &ingest, &interner).unwrap_err();
    assert!(
        err.to_string().contains("none has a readable range"),
        "{err}"
    );
}

#[test]
fn index_without_occurrences_pins_nothing() {
    let (ingest, _, interner) = crate::support::index::index_files(SOURCES);
    let bytes = Index::default().write_to_bytes().unwrap();
    assert!(scip_sites(&bytes, &ingest, &interner).unwrap().is_empty());
}
