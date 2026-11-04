use ripwire_rs::Lang;
use ripwire_rs::clones::{
    SKETCH_HASHES, compress_source_body, compute_jaccard, compute_kgrams_ids, compute_lcs_len,
    compute_sketch, scan_body_ids, sketch_matches, token_id,
};

#[test]
fn test_tokenize_normalization() {
    let code = b"fn add(x: i32, y: i32) -> i32 { return x + y + 10; }";
    let (_, _, ids) = scan_body_ids(code, Lang::Rust);
    assert!(ids.contains(&token_id(b"$I")));
    assert!(ids.contains(&token_id(b"$N")));
    assert!(ids.contains(&token_id(b"return")));
}

#[test]
fn test_skip_comments_and_strings() {
    let code = b"// line comment\n/* block comment */\n\"hello world\"\n'c'\n# hash comment\n123";
    let (_, _, ids) = scan_body_ids(code, Lang::Python);
    assert_eq!(ids, vec![token_id(b"$S"), token_id(b"$S"), token_id(b"$N")]);
}

#[test]
fn test_kgrams_jaccard_and_sketch() {
    let toks1: Vec<u32> = ["fn", "$I", "(", "$I", ")", "{", "return", "$N", "}", ";"]
        .into_iter()
        .map(|t| token_id(t.as_bytes()))
        .collect();
    let toks2 = toks1.clone();
    let fp1 = compute_kgrams_ids(&toks1, 5);
    let fp2 = compute_kgrams_ids(&toks2, 5);
    let jaccard = compute_jaccard(&fp1, &fp2);
    assert!((jaccard - 1.0).abs() < 1e-6);
    let s1 = compute_sketch(&fp1);
    let s2 = compute_sketch(&fp2);
    assert_eq!(sketch_matches(&s1, &s2), SKETCH_HASHES as u32);
}

#[test]
fn test_lcs_len() {
    let a = [1, 2, 3, 4, 5];
    let b = [1, 2, 4, 5];
    assert_eq!(compute_lcs_len(&a, &b), 4);
    assert_eq!(compute_lcs_len(&a, &a), 5);
}

#[test]
fn test_compress_source_body() {
    let src = "double computeArea(double w, double h) {\n    /* block comment */\n    // line comment\n\n\n    const char* u = \"http://example.com // keep\";\n    const char* b = \"/* keep */\";\n    return w * h;\n}";
    let compressed = compress_source_body(src);
    assert!(!compressed.contains("block comment"));
    assert!(!compressed.contains("line comment"));
    assert!(compressed.contains("http://example.com // keep"));
    assert!(compressed.contains("/* keep */"));
    assert!(!compressed.contains("\n\n\n"));
}
