use ripwire_rs::graph::community::detect_communities;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

#[test]
fn test_community_detection_two_cliques() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();

    let code1 = b"fn a() { b(); c(); } fn b() { a(); c(); } fn c() { a(); b(); }";
    let code2 = b"fn x() { y(); z(); } fn y() { x(); z(); } fn z() { x(); y(); }";

    let ingest = ingest_in_memory_files(
        &[
            ("src/cluster1.rs", Lang::Rust, code1),
            ("src/cluster2.rs", Lang::Rust, code2),
        ],
        &registry,
        &interner,
    );

    let graph = Graph::build(&ingest, &interner);
    let comms = detect_communities(&graph);

    assert_eq!(comms.comm.len(), 6);
    assert_eq!(comms.count, 2);
    assert_eq!(comms.comm[0], comms.comm[1]);
    assert_eq!(comms.comm[1], comms.comm[2]);
    assert_eq!(comms.comm[3], comms.comm[4]);
    assert_eq!(comms.comm[4], comms.comm[5]);
    assert_ne!(comms.comm[0], comms.comm[3]);
}
