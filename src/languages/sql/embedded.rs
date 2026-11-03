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

fn is_valid_sql_table_ident(ident: &str) -> bool {
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

fn scan_sql_tokens_in_literal(text: &str, start_byte: usize, line: u32, refs: &mut Vec<RawRef>) {
    if !has_dml_anchor(text.as_bytes()) {
        return;
    }
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
            if is_valid_sql_table_ident(table)
                && !ctes.iter().any(|c| c.eq_ignore_ascii_case(table))
            {
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
                    callable_field: false,
                macro_arg: false,
                    member_call: false,
                    path_call: false,
                });
            }
        }
    }
}

pub fn extract_refs(
    node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    refs: &mut Vec<RawRef>,
) {
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

