use crate::support::index::index_files;
use ripwire_rs::serialize::graph_export::{GraphExport, GraphFormat};
use ripwire_rs::{Lang, PageRankConfig};

fn export(format: GraphFormat) -> String {
    let (ingest, graph, interner) = index_files(&[(
        "src/a&b.rs",
        Lang::Rust,
        b"fn entry() { helper(); }\nfn helper() {}\n".as_slice(),
    )]);
    let pr = graph.page_rank(&PageRankConfig::default());
    let mut out = Vec::new();
    GraphExport {
        ingest: &ingest,
        graph: &graph,
        ranks: &pr.ranks,
        interner: &interner,
    }
    .write(format, &mut out)
    .unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn graphml_export_carries_the_call_edge_between_named_nodes_with_escaped_paths() {
    let text = export(GraphFormat::GraphMl);
    let doc = roxmltree::Document::parse(&text).unwrap();
    let data = |node: roxmltree::Node<'_, '_>, key: &str| {
        node.children()
            .find(|c| c.attribute("key") == Some(key))
            .and_then(|c| c.text())
            .map(str::to_string)
    };
    let id_of = |name: &str| {
        doc.descendants()
            .find(|n| n.has_tag_name("node") && data(*n, "name").as_deref() == Some(name))
            .map(|n| (n.attribute("id").unwrap().to_string(), data(n, "file")))
            .unwrap()
    };
    let (entry, file) = id_of("entry");
    let (helper, _) = id_of("helper");
    assert_eq!(file.as_deref(), Some("src/a&b.rs"));
    let edge = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("edge")
                && n.attribute("source") == Some(entry.as_str())
                && n.attribute("target") == Some(helper.as_str())
        })
        .expect("entry -> helper edge");
    assert_eq!(data(edge, "prov").as_deref(), Some("local"));
    assert_eq!(data(edge, "type_use").as_deref(), Some("false"));
}

#[test]
fn node_link_json_links_reference_node_ids() {
    let value: serde_json::Value =
        serde_json::from_str(&export(GraphFormat::NodeLinkJson)).unwrap();
    let id_of = |name: &str| {
        value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == name)
            .map(|n| n["id"].clone())
            .unwrap()
    };
    let (entry, helper) = (id_of("entry"), id_of("helper"));
    assert!(
        value["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["source"] == entry && l["target"] == helper && l["prov"] == "local")
    );
    assert_eq!(value["directed"], true);
}

#[test]
fn only_graphml_and_json_extensions_select_a_format() {
    assert_eq!(
        GraphFormat::from_path("out/g.graphml"),
        Some(GraphFormat::GraphMl)
    );
    assert_eq!(
        GraphFormat::from_path("g.json"),
        Some(GraphFormat::NodeLinkJson)
    );
    assert_eq!(GraphFormat::from_path("g.bin"), None);
    assert_eq!(GraphFormat::from_path("graphml"), None);
}
