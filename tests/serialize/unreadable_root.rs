use ripwire_rs::{
    Graph, Interner, Lang, LanguageRegistry, PageRankConfig, SerializeOptions,
    ingest_in_memory_files, serialize_xml_to_string,
};

fn map_root(unreadable: usize) -> String {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files = [("src/a.rs", Lang::Rust, b"fn a() {}".as_slice())];
    let mut ingest = ingest_in_memory_files(&files, &registry, &interner);
    ingest.unreadable = unreadable;
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let options = SerializeOptions {
        emit_legend: true,
        ..Default::default()
    };
    serialize_xml_to_string(&ingest, &graph, &pr, &interner, &options)
}

fn root_open_tag(xml: &str) -> &str {
    let start = xml.find("<r ").expect("map root element");
    let end = start + xml[start..].find('>').expect("root tag end");
    &xml[start..=end]
}

#[test]
fn unreadable_files_are_disclosed_on_the_map_root() {
    let tag = root_open_tag(&map_root(2)).to_string();
    assert!(
        tag.contains(" unreadable=\"2\" counts_floor=\"1\""),
        "{tag}"
    );
}

#[test]
fn zero_unreadable_files_leave_the_map_root_unchanged() {
    let tag = root_open_tag(&map_root(0)).to_string();
    assert!(!tag.contains("unreadable="), "{tag}");
    assert!(!tag.contains("counts_floor="), "{tag}");
}
