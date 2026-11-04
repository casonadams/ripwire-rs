use ripwire_rs::cache::{
    CACHE_VERSION, CachePayload, auto_cache_path, compute_cache_key, load_cached_payload_slice,
};
use ripwire_rs::{Graph, IngestResult, Interner, PageRankResult};

fn make_payload(key: &str, strings: Vec<String>, iterations: u32, ranks: Vec<f64>) -> CachePayload {
    CachePayload {
        version: CACHE_VERSION,
        validation_key: key.into(),
        interner_strings: strings,
        ingest: IngestResult::default(),
        graph: Graph::default(),
        pr: PageRankResult {
            iterations,
            converged: true,
            ranks,
        },
    }
}

#[test]
fn test_cache_payload_roundtrip() {
    let interner = Interner::new();
    interner.intern("test_symbol");
    interner.intern("src/test.rs");
    let payload = make_payload("test_key_123", interner.all_strings(), 5, vec![1.0]);
    let json = serde_json::to_string(&payload).unwrap();
    let loaded: CachePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.version, CACHE_VERSION);
    assert_eq!(loaded.validation_key, "test_key_123");
    assert_eq!(loaded.interner_strings.len(), 2);
}

#[test]
fn test_load_cached_payload_slice_validates_key() {
    let payload = make_payload("valid_key", Vec::new(), 1, Vec::new());
    let json = serde_json::to_string(&payload).unwrap();
    assert!(load_cached_payload_slice(json.as_bytes(), "valid_key").is_some());
    assert!(load_cached_payload_slice(json.as_bytes(), "wrong_key").is_none());
}

#[test]
fn test_compute_cache_key_non_empty() {
    // A root that is not a repository: the key must not depend on the checkout's cwd or state.
    const ROOT: &str = "/nonexistent_ripwire_cache_root";
    let key = compute_cache_key(ROOT).unwrap();
    assert!(!key.is_empty());
    let path = auto_cache_path(ROOT);
    assert!(path.to_string_lossy().contains("ripwire-rs-cache-"));
}
