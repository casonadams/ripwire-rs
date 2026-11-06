use crate::domain::Lang;

pub const SKETCH_HASHES: usize = 128;
pub const SKETCH_MIN_MATCHES: u32 = 26;

const fn sketch_splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

const fn build_sketch_multipliers() -> [u64; SKETCH_HASHES] {
    let mut m = [0u64; SKETCH_HASHES];
    let mut j = 0;
    while j < SKETCH_HASHES {
        m[j] = sketch_splitmix64((j as u64) + 1) | 1;
        j += 1;
    }
    m
}

pub const SKETCH_MUL: [u64; SKETCH_HASHES] = build_sketch_multipliers();

/// Compute 128-byte deterministic MinHash sketch of a sorted k-gram fingerprint slice.
#[must_use]
pub fn compute_sketch(kgrams: &[u64]) -> [u8; SKETCH_HASHES] {
    let mut sig = [0xFFu8; SKETCH_HASHES];
    if kgrams.is_empty() {
        return sig;
    }
    for (j, &mul) in SKETCH_MUL.iter().enumerate() {
        let mut mn = u64::MAX;
        for &g in kgrams {
            let mut h = g.wrapping_mul(mul);
            h ^= h >> 32;
            if h < mn {
                mn = h;
            }
        }
        sig[j] = mn as u8;
    }
    sig
}

/// Count matching MinHash components between two 128-byte sketches.
#[must_use]
pub fn sketch_matches(a: &[u8; SKETCH_HASHES], b: &[u8; SKETCH_HASHES]) -> u32 {
    a.iter().zip(b).map(|(x, y)| u32::from(x == y)).sum()
}

pub(crate) fn skip_whitespace(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn skip_slash_comment(bytes: &[u8], i: &mut usize) -> bool {
    if *i + 1 >= bytes.len() || bytes[*i] != b'/' {
        return false;
    }
    if bytes[*i + 1] == b'/' {
        *i += 2;
        while *i < bytes.len() && bytes[*i] != b'\n' {
            *i += 1;
        }
        true
    } else if bytes[*i + 1] == b'*' {
        *i += 2;
        while *i + 1 < bytes.len() && !(bytes[*i] == b'*' && bytes[*i + 1] == b'/') {
            *i += 1;
        }
        if *i + 1 < bytes.len() {
            *i += 2;
        }
        true
    } else {
        false
    }
}

fn skip_hash_comment(bytes: &[u8], i: &mut usize) -> bool {
    if *i < bytes.len() && bytes[*i] == b'#' {
        *i += 1;
        while *i < bytes.len() && bytes[*i] != b'\n' {
            *i += 1;
        }
        true
    } else {
        false
    }
}

fn skip_comment(bytes: &[u8], i: &mut usize, hash_comments: bool) -> bool {
    skip_slash_comment(bytes, i) || (hash_comments && skip_hash_comment(bytes, i))
}

pub(crate) fn scan_string_slice<'a>(bytes: &'a [u8], i: &mut usize) -> &'a [u8] {
    let quote = bytes[*i];
    let start = *i;
    *i += 1;
    while *i < bytes.len() {
        if bytes[*i] == b'\\' {
            *i = (*i + 2).min(bytes.len());
            continue;
        }
        if bytes[*i] == quote {
            *i += 1;
            break;
        }
        *i += 1;
    }
    &bytes[start..(*i).min(bytes.len())]
}

fn scan_alnum_plus<'a>(bytes: &'a [u8], i: &mut usize, extra: u8) -> &'a [u8] {
    let start = *i;
    while *i < bytes.len() && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == extra) {
        *i += 1;
    }
    &bytes[start..*i]
}

pub(crate) fn scan_number_slice<'a>(bytes: &'a [u8], i: &mut usize) -> &'a [u8] {
    scan_alnum_plus(bytes, i, b'.')
}

pub(crate) fn scan_ident_slice<'a>(bytes: &'a [u8], i: &mut usize) -> &'a [u8] {
    scan_alnum_plus(bytes, i, b'_')
}

pub(crate) fn scan_operator_slice<'a>(bytes: &'a [u8], i: &mut usize) -> &'a [u8] {
    let multi_ops: &[&[u8]] = &[
        b"-=", b"*=", b"/=", b"%=", b"&=", b"|=", b"^=", b"->", b"=>", b"::", b"..", b"++", b"--",
        b"??", b"?.",
    ];
    for op in multi_ops {
        if bytes[*i..].starts_with(op) {
            *i += op.len();
            return op;
        }
    }
    let start = *i;
    *i += 1;
    &bytes[start..*i]
}

fn next_token_slices<'a>(bytes: &'a [u8], i: &mut usize, lang: Lang) -> (&'a [u8], &'a [u8]) {
    let b = bytes[*i];
    if b == b'"' || b == b'\'' || b == b'`' {
        let raw = scan_string_slice(bytes, i);
        (raw, b"$S")
    } else if b.is_ascii_digit() {
        let raw = scan_number_slice(bytes, i);
        (raw, b"$N")
    } else if b.is_ascii_alphabetic() || b == b'_' {
        let raw = scan_ident_slice(bytes, i);
        let norm = if lang.semantics().is_keyword(raw) {
            raw
        } else {
            b"$I"
        };
        (raw, norm)
    } else {
        let start = *i;
        *i += 1;
        let punct = &bytes[start..*i];
        (punct, punct)
    }
}

fn fnv1a_absorb_token(mut hash: u64, token: &[u8]) -> u64 {
    for &b in token {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash ^= 0x20;
    hash.wrapping_mul(0x1000_0000_01b3)
}

fn for_each_body_token<'a>(bytes: &'a [u8], lang: Lang, mut visit: impl FnMut(&'a [u8], &'a [u8])) {
    let hash_comments = lang.semantics().uses_hash_comments();
    let mut i = 0;
    while i < bytes.len() {
        skip_whitespace(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        if skip_comment(bytes, &mut i, hash_comments) {
            continue;
        }
        let (raw_slice, norm_slice) = next_token_slices(bytes, &mut i, lang);
        visit(raw_slice, norm_slice);
    }
}

/// Zero-allocation scanner returning `(raw_hash, norm_hash, norm_token_ids)`.
#[must_use]
pub fn scan_body_ids(bytes: &[u8], lang: Lang) -> (u64, u64, Vec<u32>) {
    let mut raw_hash = 0xcbf2_9ce4_8422_2325u64;
    let mut norm_hash = 0xcbf2_9ce4_8422_2325u64;
    let mut norm_ids = Vec::with_capacity(bytes.len() / 6);
    for_each_body_token(bytes, lang, |raw_slice, norm_slice| {
        raw_hash = fnv1a_absorb_token(raw_hash, raw_slice);
        norm_hash = fnv1a_absorb_token(norm_hash, norm_slice);
        norm_ids.push(token_id(norm_slice));
    });
    (raw_hash, norm_hash, norm_ids)
}

fn sorted_window_hashes<T>(slice: &[T], k: usize, hasher: fn(&[T]) -> u64) -> Vec<u64> {
    if slice.len() < k {
        return Vec::new();
    }
    let mut list: Vec<u64> = slice.windows(k).map(hasher).collect();
    list.sort_unstable();
    list.dedup();
    list
}

/// Compute 32-bit FNV-1a hash of a token byte slice.
#[must_use]
pub fn token_id(token: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for &b in token {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Compute 64-bit FNV-1a hash of a token id window.
#[must_use]
pub fn hash_token_window(window: &[u32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &id in window {
        for b in id.to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        hash ^= 0x20;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Compute sorted, unique k-gram fingerprint hashes from token ids.
#[must_use]
pub fn compute_kgrams_ids(ids: &[u32], k: usize) -> Vec<u64> {
    sorted_window_hashes(ids, k, hash_token_window)
}

/// Compute Jaccard similarity of two sorted, unique k-gram hash vectors.
#[must_use]
pub fn compute_jaccard(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut i = 0;
    let mut j = 0;
    let mut intersection = 0;

    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            intersection += 1;
            i += 1;
            j += 1;
        } else if a[i] < b[j] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let union = a.len() + b.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn trim_common_edges<'a>(a: &'a [u32], b: &'a [u32]) -> (&'a [u32], &'a [u32], usize) {
    let mut prefix = 0;
    let min_len = a.len().min(b.len());
    while prefix < min_len && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < (min_len - prefix) && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix] {
        suffix += 1;
    }
    (
        &a[prefix..a.len() - suffix],
        &b[prefix..b.len() - suffix],
        prefix + suffix,
    )
}

/// Compute Longest Common Subsequence (LCS) length using prefix/suffix trim and 1-row DP.
#[must_use]
pub fn compute_lcs_len(a: &[u32], b: &[u32]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let (a_mid, b_mid, trimmed) = trim_common_edges(a, b);
    if a_mid.is_empty() || b_mid.is_empty() {
        return trimmed;
    }
    let (long, short) = if a_mid.len() >= b_mid.len() {
        (a_mid, b_mid)
    } else {
        (b_mid, a_mid)
    };
    let mut dp = vec![0u32; short.len() + 1];
    for &x in long {
        let mut prev_diag = 0u32;
        for (j, &y) in short.iter().enumerate() {
            let saved = dp[j + 1];
            dp[j + 1] = if x == y {
                prev_diag + 1
            } else {
                saved.max(dp[j])
            };
            prev_diag = saved;
        }
    }
    trimmed + (dp[short.len()] as usize)
}

#[must_use]
fn strip_comments_preserving_strings(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if skip_slash_comment(bytes, &mut i) {
            continue;
        }
        if matches!(bytes[i], b'"' | b'\'' | b'`') {
            out.extend_from_slice(scan_string_slice(bytes, &mut i));
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

pub fn compress_source_body(body: &str) -> String {
    let out = strip_comments_preserving_strings(body.as_bytes());
    let stripped = String::from_utf8_lossy(&out);
    let mut lines = Vec::new();
    let mut prev_blank = false;
    for line in stripped.lines() {
        let trimmed_end = line.trim_end();
        if !trimmed_end.trim().is_empty() {
            lines.push(trimmed_end.to_string());
            prev_blank = false;
        } else if !prev_blank && !lines.is_empty() {
            lines.push(String::new());
            prev_blank = true;
        }
    }
    lines.join("\n")
}
