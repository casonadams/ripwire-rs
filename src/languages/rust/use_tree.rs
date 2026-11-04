use super::scope::field_text;

/// The `(module path, bound name)` pairs a `use` argument binds: `a::{b, c::d as e, self}` binds `a::b` as
/// `b`, `a::c::d` as `e` and `a` as `a`. Glob imports bind nothing.
pub(super) fn use_tree_bindings(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    expand_use_tree(raw, "", &mut out);
    out
}

fn expand_use_tree(tree: &str, prefix: &str, out: &mut Vec<(String, String)>) {
    let tree = tree.trim();
    if let Some(open) = tree.find('{') {
        let head = tree[..open].trim().trim_end_matches("::");
        let rest = &tree[open + 1..];
        let inner = rest.trim_end().strip_suffix('}').unwrap_or(rest);
        let base = if head == "self" {
            prefix.to_string()
        } else {
            join_use_path(prefix, head)
        };
        for item in split_top_level_commas(inner) {
            expand_use_tree(item, &base, out);
        }
        return;
    }
    let (path, alias) = match tree.split_once(" as ") {
        Some((path, alias)) => (path.trim(), Some(alias.trim())),
        None => (tree, None),
    };
    if path.is_empty() || path == "*" {
        return;
    }
    let full = if path == "self" {
        prefix.to_string()
    } else {
        join_use_path(prefix, path)
    };
    let last = full.rsplit("::").next().unwrap_or_default();
    let bound = alias.unwrap_or(last);
    if !full.is_empty() && !bound.is_empty() && bound != "_" {
        out.push((full.clone(), bound.to_string()));
    }
}

fn join_use_path(prefix: &str, path: &str) -> String {
    let path = match path.strip_prefix("self::") {
        Some(rest) if !prefix.is_empty() => rest,
        _ => path,
    };
    match (prefix.is_empty(), path.is_empty()) {
        (_, true) => prefix.to_string(),
        (true, false) => path.to_string(),
        (false, false) => format!("{prefix}::{path}"),
    }
}

/// Splits a use-group body at its top-level commas (commas inside nested braces stay with their item).
fn split_top_level_commas(list: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in list.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

fn join_path(prefix: &str, segment: &str) -> String {
    match (prefix.is_empty(), segment.is_empty()) {
        (_, true) => prefix.to_string(),
        (true, false) => segment.to_string(),
        (false, false) => format!("{prefix}::{segment}"),
    }
}

/// Collects `(bound name, target path)` pairs from a use tree; globs bind nothing.
pub(super) fn collect_use_tree(
    tree: tree_sitter::Node<'_>,
    prefix: &str,
    source: &[u8],
    out: &mut Vec<(String, String)>,
) {
    match tree.kind() {
        "identifier" | "scoped_identifier" | "crate" | "super" => {
            if let Ok(text) = tree.utf8_text(source) {
                let text = text.trim().trim_start_matches("::");
                let bound = text.rsplit("::").next().unwrap_or_default().to_string();
                out.push((bound, join_path(prefix, text)));
            }
        }
        "self" => {
            if let Some(last) = prefix.rsplit("::").next().filter(|s| !s.is_empty()) {
                out.push((last.to_string(), prefix.to_string()));
            }
        }
        "use_as_clause" => {
            if let (Some(alias), Some(path)) = (
                field_text(tree, "alias", source),
                field_text(tree, "path", source),
            ) && alias != "_"
            {
                let path = path.trim_start_matches("::");
                out.push((alias.to_string(), join_path(prefix, path)));
            }
        }
        "scoped_use_list" => {
            let base = field_text(tree, "path", source).unwrap_or_default();
            let next = join_path(prefix, base.trim_start_matches("::"));
            if let Some(list) = tree.child_by_field_name("list") {
                collect_use_tree(list, &next, source, out);
            }
        }
        "use_list" => {
            let mut cursor = tree.walk();
            for child in tree.named_children(&mut cursor) {
                collect_use_tree(child, prefix, source, out);
            }
        }
        _ => {}
    }
}
