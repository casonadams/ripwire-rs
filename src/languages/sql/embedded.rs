//! SQL table accesses found in string literals of host-language files.
use crate::domain::{Lang, RefRole};
use crate::ingest::RawRef;
const SQL_DISALLOWED_TARGETS: &[&str] = &[
    "select", "where", "set", "values", "null", "true", "false", "lateral", "unnest", "dual",
    "only", "stdin", "stdout", "json", "jsonb", "table", "index", "each", "row",
];

fn is_sql_string_node(kind: &str) -> bool {
    kind.contains("string") || kind == "raw_string_literal" || kind == "template_string"
}

pub fn has_dml_anchor(bytes: &[u8]) -> bool {
    for kw in [b"select" as &[u8], b"insert", b"update", b"delete", b"join"] {
        if bytes.windows(kw.len()).any(|w| w.eq_ignore_ascii_case(kw)) {
            return true;
        }
    }
    false
}

fn trim_sql_punct(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | ',' | ';' | ':' | '[' | ']' | '\\'
        )
    })
}

fn clean_sql_token(tok: &str) -> &str {
    let t = trim_sql_punct(tok);
    let last = t.rsplit('.').next().unwrap_or(t);
    trim_sql_punct(last)
}

/// A bare SQL identifier that is not a reserved word: a table or a function name.
pub fn is_valid_sql_ident(ident: &str) -> bool {
    if ident.len() < 2
        || ident.starts_with('$')
        || !ident
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !ident
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    !SQL_DISALLOWED_TARGETS
        .iter()
        .any(|&kw| ident.eq_ignore_ascii_case(kw))
}

fn classify_sql_keyword_pair(prev: &str, curr: &str) -> Option<RefRole> {
    if curr.eq_ignore_ascii_case("from") {
        if prev.eq_ignore_ascii_case("delete") {
            Some(RefRole::Write)
        } else if prev.eq_ignore_ascii_case("distinct") || prev.eq_ignore_ascii_case("extract") {
            None
        } else {
            Some(RefRole::Read)
        }
    } else if curr.eq_ignore_ascii_case("join") {
        Some(RefRole::Read)
    } else if (curr.eq_ignore_ascii_case("into") && prev.eq_ignore_ascii_case("insert"))
        || (curr.eq_ignore_ascii_case("update") && !prev.eq_ignore_ascii_case("for"))
    {
        Some(RefRole::Write)
    } else {
        None
    }
}

/// Names a literal declares as CTEs (`name AS (`), which are local to that query, not tables.
fn cte_names<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    tokens
        .windows(3)
        .filter(|w| {
            clean_sql_token(w[1]).eq_ignore_ascii_case("as")
                && (w[1].ends_with('(') || w[2].starts_with('('))
        })
        .map(|w| clean_sql_token(w[0]))
        .collect()
}

/// Words that open a call-shaped group in SQL without calling a function.
const SQL_NON_FUNCTIONS: &[&str] = &[
    "select",
    "where",
    "and",
    "or",
    "not",
    "in",
    "exists",
    "values",
    "as",
    "on",
    "using",
    "case",
    "when",
    "then",
    "else",
    "end",
    "cast",
    "any",
    "all",
    "over",
    "filter",
    "array",
    "row",
    "returning",
    "conflict",
    "by",
    "having",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
];

/// Words after which a call-shaped name is a table or column list, not a function call.
const SQL_NAME_POSITIONS: &[&str] = &["into", "table", "update", "as"];

fn starts_with_sql_verb(text: &str) -> bool {
    let body = text.trim_start_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '`' | 'f' | 'F' | 'r' | 'R' | 'b' | 'B' | 'u' | 'U'
            )
    });
    [
        "select", "with", "call", "perform", "insert", "update", "delete",
    ]
    .iter()
    .any(|verb| {
        body.len() > verb.len()
            && body.is_char_boundary(verb.len())
            && body[..verb.len()].eq_ignore_ascii_case(verb)
            && body.as_bytes()[verb.len()].is_ascii_whitespace()
    })
}

/// Calls of the form `name(` in a literal that is a SQL statement: `SELECT promote_template(...)`,
/// `CALL refresh(...)`. A schema qualifier is dropped.
fn scan_sql_function_calls(text: &str, start_byte: usize, line: u32, refs: &mut Vec<RawRef>) {
    if !starts_with_sql_verb(text) {
        return;
    }
    let bytes = text.as_bytes();
    let mut previous_word = String::new();
    let mut from = 0;
    while let Some((begin, end)) = next_word(bytes, from) {
        from = end;
        let word = &text[begin..end];
        if is_function_call(bytes, begin, end, &previous_word) {
            refs.push(sql_call_ref(word, start_byte, line));
        }
        if bytes.get(end) != Some(&b'.') {
            previous_word = word.to_string();
        }
    }
}

/// The next identifier at or after `from` as `(begin, end)`, skipping `${..}` interpolations.
fn next_word(bytes: &[u8], mut from: usize) -> Option<(usize, usize)> {
    while from < bytes.len() {
        let byte = bytes[from];
        if byte == b'$' && bytes.get(from + 1) == Some(&b'{') {
            from = interpolation_end(bytes, from);
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let end = bytes[from..]
                .iter()
                .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                .map_or(bytes.len(), |len| from + len);
            return Some((from, end));
        } else {
            from += 1;
        }
    }
    None
}

/// True when the word at `begin..end` is followed by `(`, is no keyword or name-list position, and is not a
/// `$name(` placeholder.
fn is_function_call(bytes: &[u8], begin: usize, end: usize, previous_word: &str) -> bool {
    let word = std::str::from_utf8(&bytes[begin..end]).unwrap_or_default();
    bytes.get(end) == Some(&b'(')
        && !SQL_NON_FUNCTIONS
            .iter()
            .any(|kw| word.eq_ignore_ascii_case(kw))
        && !SQL_NAME_POSITIONS
            .iter()
            .any(|kw| previous_word.eq_ignore_ascii_case(kw))
        && !(begin > 0 && bytes[begin - 1] == b'$')
}

/// The index after the `}` closing the `${` at `start`, or the end of `bytes` when it never closes. What a
/// template interpolation holds is host code, not SQL.
fn interpolation_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0;
    for (offset, &byte) in bytes[start + 1..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start + 2 + offset;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

/// A call reference to the SQL function `name`, for a host-language string that names it.
pub fn sql_call_ref(name: &str, start_byte: usize, line: u32) -> RawRef {
    RawRef {
        callee_name: name.to_string(),
        qualifier: None,
        line,
        name_line: line,
        start_byte,
        role: RefRole::Call,
        arg_count: 0,
        arg_count_known: false,
        lang: Some(Lang::Sql),
        receiver: None,
        receiver_type: None,
        receiver_chain: None,
        receiver_file: None,
        callable_field: false,
        macro_arg: false,
        member_call: false,
        path_call: false,
    }
}

fn scan_sql_tokens_in_literal(text: &str, start_byte: usize, line: u32, refs: &mut Vec<RawRef>) {
    if !has_dml_anchor(text.as_bytes()) {
        return;
    }
    scan_sql_function_calls(text, start_byte, line, refs);
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let ctes = cte_names(&tokens);
    for i in 0..tokens.len() {
        let prev = if i > 0 {
            clean_sql_token(tokens[i - 1])
        } else {
            ""
        };
        let curr = clean_sql_token(tokens[i]);
        let Some(role) = classify_sql_keyword_pair(prev, curr) else {
            continue;
        };
        let mut target_idx = i + 1;
        if target_idx < tokens.len()
            && clean_sql_token(tokens[target_idx]).eq_ignore_ascii_case("only")
        {
            target_idx += 1;
        }
        if let Some(&raw_target) = tokens.get(target_idx) {
            let table = clean_sql_token(raw_target);
            if is_valid_sql_ident(table) && !ctes.iter().any(|c| c.eq_ignore_ascii_case(table)) {
                refs.push(RawRef {
                    callee_name: table.to_string(),
                    qualifier: None,
                    line,
                    name_line: line,
                    start_byte,
                    role,
                    arg_count: 0,
                    arg_count_known: false,
                    lang: Some(Lang::Sql),
                    receiver: None,
                    receiver_type: None,
                    receiver_chain: None,
                    receiver_file: None,
                    callable_field: false,
                    macro_arg: false,
                    member_call: false,
                    path_call: false,
                });
            }
        }
    }
}

pub fn extract_refs(node: tree_sitter::Node<'_>, source_bytes: &[u8], refs: &mut Vec<RawRef>) {
    if is_sql_string_node(node.kind()) {
        if let Ok(text) = node.utf8_text(source_bytes) {
            let line = node.start_position().row as u32 + 1;
            scan_sql_tokens_in_literal(text, node.start_byte(), line, refs);
        }
        return;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_refs(child, source_bytes, refs);
        }
    }
}
