use crate::corpus_support::fixture;
use ripwire_rs::gate::seams::{SeamsReport, find_integration_seams};
use ripwire_rs::serialize::{PageRequest, XmlReport};
use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};

const ONE_CALLER: &[(&str, &str)] = &[
    ("src/gate/seams.rs", "pub fn caller_fn() { callee_fn(); }\n"),
    ("lib.rs", "pub fn callee_fn() {}\n"),
];

const TWO_CALLERS: &[(&str, &str)] = &[
    ("src/gate/seams.rs", "pub fn caller_fn() { callee_fn(); }\n"),
    (
        "src/query/safety.rs",
        "pub fn caller_query() { callee_fn(); }\n",
    ),
    ("lib.rs", "pub fn callee_fn() {}\n"),
];

fn seams(files: &[(&str, &str)], page: PageRequest) -> SeamsReport {
    let corpus = fixture(Lang::Rust, files);
    find_integration_seams(&corpus.ingest, &corpus.graph, &corpus.interner, page)
}

#[test]
fn test_find_integration_seams_across_directories() {
    let rep = seams(ONE_CALLER, PageRequest::default());
    assert_eq!(rep.groups.len(), 1);
    assert_eq!(rep.groups[0].from_dir, "src/gate");
    assert_eq!(rep.groups[0].to_dir, ".");
}

#[test]
fn test_find_integration_seams_honors_limit_and_offset() {
    let page = PageRequest {
        limit: Some(1),
        offset: Some(1),
        budget: None,
    };
    let rep = seams(TWO_CALLERS, page);
    assert_eq!(rep.groups.len(), 2);
    let window = rep.page_window().expect("seams report is paged");
    assert_eq!((window.shown, window.total, window.begin), (1, 2, 1));
    assert!(!window.has_more);
}

#[test]
fn test_find_integration_seams_budget_fits_groups() {
    let roomy = PageRequest {
        limit: None,
        offset: None,
        budget: Some(usize::MAX),
    };
    let rep = seams(TWO_CALLERS, roomy);
    let window = rep.page_window().expect("seams report is paged");
    assert_eq!((window.shown, window.total, window.has_more), (2, 2, false));
    let tight = PageRequest {
        budget: Some(1),
        ..roomy
    };
    let rep = seams(TWO_CALLERS, tight);
    let window = rep.page_window().expect("seams report is paged");
    assert_eq!((window.shown, window.total, window.has_more), (1, 2, true));
}

#[test]
fn test_seam_keeps_path_qualified_call_from_shared_name_caller() {
    let rep = seams(
        &[
            (
                "src/alpha/run.rs",
                "pub fn execute() -> u32 {\n    crate::beta::helper()\n}\n",
            ),
            ("src/beta/tool.rs", "pub fn helper() -> u32 {\n    1\n}\n"),
        ],
        PageRequest::default(),
    );
    assert_eq!(rep.groups.len(), 1);
    assert_eq!(rep.groups[0].from_dir, "src/alpha");
    assert_eq!(rep.groups[0].to_dir, "src/beta");
    assert_eq!(rep.groups[0].edges.len(), 1);
    assert_eq!(rep.groups[0].edges[0].caller, "execute");
    assert_eq!(rep.groups[0].edges[0].callee, "helper");
}

#[test]
fn test_seam_ignores_path_qualified_call_within_one_directory() {
    let rep = seams(
        &[
            (
                "src/alpha/run.rs",
                "pub fn execute() -> u32 {\n    crate::alpha::local()\n}\n",
            ),
            ("src/alpha/local.rs", "pub fn local() -> u32 {\n    1\n}\n"),
        ],
        PageRequest::default(),
    );
    assert!(rep.groups.is_empty());
}

#[test]
fn test_seams_doc_links_count_cross_directory_indexed_document_links() {
    use ripwire_rs::gate::seams::count_doc_links;
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let readme: &[u8] = b"[a](docs/A.md)\n[a2](docs/A.md#sec)\n[b](docs/B.md)\n[m](docs/Missing.md)\n[w](https://example.com/x.md)\n[self](#top)\n";
    let a: &[u8] = b"[b](B.md)\n[r](../README.md)\n";
    let b: &[u8] = b"[x](#top)\n";
    let ingest = ingest_in_memory_files(
        &[
            ("README.md", Lang::Markdown, readme),
            ("docs/A.md", Lang::Markdown, a),
            ("docs/B.md", Lang::Markdown, b),
        ],
        &reg,
        &interner,
    );
    let sources: [(&str, &[u8]); 3] = [("README.md", readme), ("docs/A.md", a), ("docs/B.md", b)];
    assert_eq!(count_doc_links(&sources, &ingest, &interner), 4);
}

#[test]
fn test_seams_doc_links_attribute_is_emitted_only_when_counted() {
    let mut rep = seams(ONE_CALLER, PageRequest::default());
    let mut out = Vec::new();
    rep.render_envelope(&mut out).unwrap();
    assert!(!String::from_utf8_lossy(&out).contains("doc_links"));
    rep.doc_links = Some(4);
    let mut out = Vec::new();
    rep.render_envelope(&mut out).unwrap();
    assert!(String::from_utf8_lossy(&out).contains(" doc_links=\"4\""));
}
