use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    ColorBy, Graph, IngestResult, Interner, Lang, SymKind, Symbol, write_call_graph_html,
};

#[test]
fn test_color_by_parsing() {
    assert_eq!(ColorBy::from_str_opt("lang"), Some(ColorBy::Lang));
    assert_eq!(ColorBy::from_str_opt("community"), Some(ColorBy::Community));
    assert_eq!(ColorBy::from_str_opt("cx"), Some(ColorBy::Cx));
    assert_eq!(ColorBy::from_str_opt("churn"), Some(ColorBy::Churn));
    assert_eq!(ColorBy::from_str_opt("tested"), Some(ColorBy::Tested));
    assert_eq!(ColorBy::from_str_opt("invalid"), None);

    assert_eq!(ColorBy::Lang.mode_name(), "lang");
    assert_eq!(ColorBy::Community.mode_name(), "community");
    assert_eq!(ColorBy::Cx.mode_name(), "cx");
    assert_eq!(ColorBy::Churn.mode_name(), "churn");
    assert_eq!(ColorBy::Tested.mode_name(), "tested");
}

#[test]
fn test_lang_display_colors() {
    assert_eq!(Lang::Rust.semantics().display_color(), "#dea584");
    assert_eq!(Lang::Python.semantics().display_color(), "#3572a5");
    assert_eq!(Lang::Unknown.semantics().display_color(), "#888888");
}

#[test]
fn test_generate_and_write_call_graph_html() {
    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    let f_spur = interner.intern("src/main.rs");
    let n1_spur = interner.intern("main");
    let n2_spur = interner.intern("helper");
    ingest.symbols.push(Symbol::new(
        0,
        f_spur,
        n1_spur,
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    ));
    ingest.symbols.push(Symbol::new(
        1,
        f_spur,
        n2_spur,
        SymKind::Function,
        Lang::Rust,
        0,
        10,
    ));

    let graph = Graph::build(&ingest, &interner);
    let ranks = vec![0.6, 0.4];

    let mut buf = Vec::new();
    write_call_graph_html(
        &mut buf,
        &ingest,
        &graph,
        &ranks,
        &interner,
        ColorBy::Community,
        PageRequest {
            limit: Some(10),
            ..Default::default()
        },
    )
    .unwrap();

    let html = String::from_utf8(buf).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<canvas id=\"c\">"));
    assert!(html.contains("const NODES"));
}
