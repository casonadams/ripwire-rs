use crate::support::index::index_files;
use ripwire_rs::query::retrieval::{ForTaskOptions, call_for_task_with_options};
use ripwire_rs::{Lang, PageRankConfig};
use serde_json::Value;

const NAVIGATE_XML: &[u8] =
    b"def write_report():\n    return 1\n\ndef write_row():\n    return 2\n";
const DECOY: &[u8] = b"def navigate_xml_rs_src():\n    return write_report()\n";

fn answer(task: &str) -> Value {
    let (ingest, graph, interner) = index_files(&[
        ("src/navigate/xml.py", Lang::Python, NAVIGATE_XML),
        ("src/query/decoy.py", Lang::Python, DECOY),
    ]);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    call_for_task_with_options(&ingest, &interner, &ranks, task, ForTaskOptions::default())
}

fn paths(answer: &Value) -> Vec<&str> {
    answer["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["path"].as_str().unwrap())
        .collect()
}

#[test]
fn a_task_that_is_an_indexed_file_path_answers_with_that_files_symbols() {
    let res = answer("src/navigate/xml.py");
    assert_eq!(res["route"], "file(src/navigate/xml.py)");
    assert_eq!(res["confidence"], "high");
    let paths = paths(&res);
    assert_eq!(paths.len(), 2, "{res}");
    assert!(paths.iter().all(|p| *p == "src/navigate/xml.py"), "{res}");
}

#[test]
fn a_task_that_only_resembles_a_file_path_keeps_the_ranked_route() {
    let res = answer("src/navigate/missing.py");
    assert!(
        !res["route"].as_str().unwrap().starts_with("file("),
        "{res}"
    );
}
