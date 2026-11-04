fn stem_derivational_suffix(t: &str) -> Option<String> {
    if let Some(prefix) = t.strip_suffix("ution").filter(|p| p.len() >= 3) {
        return Some(format!("{prefix}v"));
    }
    for (suffix, min_stem) in [
        ("ization", 4),
        ("ation", 4),
        ("ition", 4),
        ("tion", 4),
        ("sion", 4),
        ("ment", 4),
        ("ing", 4),
        ("y", 5),
    ] {
        if let Some(stripped) = t.strip_suffix(suffix)
            && stripped.len() >= min_stem
        {
            return Some(stripped.to_string());
        }
    }
    None
}

fn stem_plural_only(term: &str) -> String {
    let mut t = term.to_ascii_lowercase();
    if t.len() >= 5 && t.ends_with('s') && !t.ends_with("ss") {
        t.pop();
    }
    t
}

pub(super) fn stem_query_term(term: &str) -> String {
    let t = stem_plural_only(term);
    stem_derivational_suffix(&t).unwrap_or(t)
}

fn is_camel_boundary(bytes: &[u8], i: usize) -> bool {
    if i == 0 || !bytes[i].is_ascii_uppercase() {
        return false;
    }
    let prev_lower = bytes[i - 1].is_ascii_lowercase() || bytes[i - 1].is_ascii_digit();
    let next_lower =
        bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase) && bytes[i - 1].is_ascii_uppercase();
    prev_lower || next_lower
}

pub(super) fn normalize_camel_case_lower(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if is_camel_boundary(bytes, i) {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub(super) fn is_for_stopword(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "how"
            | "the"
            | "and"
            | "are"
            | "for"
            | "with"
            | "from"
            | "into"
            | "what"
            | "when"
            | "where"
            | "which"
            | "that"
            | "this"
            | "does"
            | "can"
            | "via"
            | "use"
            | "used"
            | "using"
    )
}

pub(super) fn contains_term_aware(haystack: &str, needle: &str) -> bool {
    if needle.len() >= 4 {
        return haystack.contains(needle);
    }
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == needle || (needle.len() == 3 && tok.starts_with(needle)))
}

/// Score a symbol against a single search term across name, scope, and file path.
pub(super) fn matches_term_or_stem(haystack: &str, t: &str, stem: &str) -> bool {
    contains_term_aware(haystack, t) || contains_term_aware(haystack, stem)
}
