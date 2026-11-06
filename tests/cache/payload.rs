use ripwire_rs::cache::{
    CACHE_VERSION, CachePayload, auto_cache_path, blob_state, compute_cache_key, encode_payload,
    load_cached_payload_slice,
};
use ripwire_rs::{Graph, IngestResult, Interner, PageRankResult};

fn make_payload(key: &str, strings: Vec<String>, iterations: u32, ranks: Vec<f64>) -> CachePayload {
    CachePayload {
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
    let payload = make_payload("test_key_123", interner.all_strings(), 5, vec![1.0, 0.25]);
    let blob = encode_payload(&payload).unwrap();
    assert_eq!(blob_state(&blob), "ok");
    let (_, _, pr, loaded) = load_cached_payload_slice(&blob, "test_key_123").unwrap();
    assert_eq!(pr.iterations, 5);
    assert_eq!(pr.ranks, vec![1.0, 0.25]);
    assert_eq!(loaded.all_strings(), interner.all_strings());
}

#[test]
fn test_encode_payload_is_deterministic() {
    let payload = make_payload("k", vec!["a".into(), "b".into()], 2, vec![0.5]);
    assert_eq!(
        encode_payload(&payload).unwrap(),
        encode_payload(&payload).unwrap()
    );
}

#[test]
fn test_load_cached_payload_slice_checks_key_then_header() {
    let payload = make_payload("valid_key", Vec::new(), 1, Vec::new());
    let blob = encode_payload(&payload).unwrap();
    assert!(load_cached_payload_slice(&blob, "valid_key").is_some());
    assert!(load_cached_payload_slice(&blob, "wrong_key").is_none());
    let mut stale = blob;
    stale[4..8].copy_from_slice(&CACHE_VERSION.wrapping_add(1).to_le_bytes());
    assert_eq!(blob_state(&stale), "cache-version");
    assert!(load_cached_payload_slice(&stale, "valid_key").is_none());
}

#[test]
fn test_foreign_and_damaged_blobs_are_misses_not_panics() {
    let payload = make_payload("valid_key", vec!["x".into()], 3, vec![0.1, 0.2]);
    let blob = encode_payload(&payload).unwrap();
    let json = br#"{"version":38,"validation_key":"valid_key"}"#;
    assert!(load_cached_payload_slice(json, "valid_key").is_none());
    assert_eq!(blob_state(json), "cache-version");
    for len in 0..blob.len() {
        assert!(load_cached_payload_slice(&blob[..len], "valid_key").is_none());
    }
    let mut trailing = blob.clone();
    trailing.push(0);
    assert!(load_cached_payload_slice(&trailing, "valid_key").is_none());
    assert_eq!(blob_state(&trailing), "corrupt");
    // A flipped body byte may still decode to valid data; the contract here is only that it never panics.
    let mut flipped = blob;
    flipped[8] ^= 0xff;
    let _ = load_cached_payload_slice(&flipped, "valid_key");
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
