use ripwire_rs::metrics::execute_biggest_first_query;
use ripwire_rs::serialize::{
    CommunityModule, CommunitySummary, PageRequest, serialize_communities_with_summary_xml,
    serialize_external_surface_xml,
};
use ripwire_rs::{IngestResult, Interner, Lang, SymKind, Symbol};

fn biggest_first(page: PageRequest) -> String {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    for (name, loc) in [("big_fn", 300), ("mid_fn", 200), ("small_fn", 100)] {
        let mut sym = Symbol::new(
            0,
            interner.intern(name),
            interner.intern(&format!("src/lib.rs::{name}")),
            SymKind::Function,
            Lang::Rust,
            0,
            1,
        );
        sym.loc = loc;
        ingest.symbols.push(sym);
    }
    let mut buf = Vec::new();
    execute_biggest_first_query(".", &ingest, &interner, page, &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

#[test]
fn biggest_first_pages_and_fits_budget() {
    let first = biggest_first(PageRequest {
        limit: Some(1),
        ..PageRequest::default()
    });
    assert!(first.contains("big_fn"));
    assert!(!first.contains("mid_fn"));
    assert!(!first.contains("small_fn"));

    let second = biggest_first(PageRequest {
        limit: Some(1),
        offset: Some(1),
        ..PageRequest::default()
    });
    assert!(second.contains("mid_fn"));
    assert!(!second.contains("big_fn"));

    let all = biggest_first(PageRequest::default());
    assert!(all.contains("big_fn"));
    assert!(all.contains("mid_fn"));
    assert!(all.contains("small_fn"));

    let tiny = biggest_first(PageRequest {
        budget: Some(1),
        ..PageRequest::default()
    });
    assert!(tiny.contains("big_fn"));
    assert!(!tiny.contains("mid_fn"));
    assert!(tiny.contains("has_more=\"1\""));

    let roomy = biggest_first(PageRequest {
        budget: Some(usize::MAX),
        ..PageRequest::default()
    });
    assert!(roomy.contains("small_fn"));
    assert!(roomy.contains("capped=\"0\""));

    let limited = biggest_first(PageRequest {
        limit: Some(2),
        budget: Some(1),
        ..PageRequest::default()
    });
    assert!(limited.contains("big_fn") && limited.contains("mid_fn"));
    assert!(!limited.contains("small_fn"));
}

#[test]
fn external_surface_honors_limit_and_offset() {
    let externals = [
        ("alpha_ext", "rs", 1),
        ("beta_ext", "rs", 1),
        ("gamma_ext", "rs", 1),
    ];
    let mut buf = Vec::new();
    let page = PageRequest {
        limit: Some(1),
        offset: Some(1),
        ..PageRequest::default()
    };
    serialize_external_surface_xml(".", &externals, page, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("beta_ext"));
    assert!(!out.contains("alpha_ext"));
    assert!(!out.contains("gamma_ext"));
}

#[test]
fn communities_honor_limit() {
    let modules = [
        CommunityModule {
            id: 0,
            size: 1,
            dir: "src",
            label: "alpha_mod",
            members: vec![("function", "alpha_fn", "src/a.rs", 1)],
        },
        CommunityModule {
            id: 1,
            size: 1,
            dir: "src",
            label: "beta_mod",
            members: vec![("function", "beta_fn", "src/b.rs", 1)],
        },
    ];
    let summary = CommunitySummary::default();
    let mut buf = Vec::new();
    let page = PageRequest {
        limit: Some(1),
        ..PageRequest::default()
    };
    serialize_communities_with_summary_xml(".", &modules, &summary, page, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("alpha_mod"));
    assert!(!out.contains("beta_mod"));
}
