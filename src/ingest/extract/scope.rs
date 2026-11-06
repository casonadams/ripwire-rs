use crate::domain::Lang;
use crate::languages::common::util::ancestors;

pub fn distinct_scope_tail(
    cand: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    sep: &str,
) -> Option<String> {
    if cand == name_node {
        return None;
    }
    let text = cand.utf8_text(source_bytes).ok()?.trim();
    let tail = if sep.is_empty() {
        text
    } else {
        text.rsplit(sep).next().unwrap_or(text)
    };
    let clean = tail.split('<').next().unwrap_or(tail).trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

fn find_positional_type_ident(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    let first = (0..parent.named_child_count())
        .filter_map(|i| parent.named_child(i))
        .find(|c| matches!(c.kind(), "type_identifier" | "identifier"))?;
    distinct_scope_tail(first, name_node, source_bytes, "")
}

fn extract_named_container_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    if let Some(name_child) = parent.child_by_field_name("name") {
        return distinct_scope_tail(name_child, name_node, source_bytes, "::");
    }
    find_positional_type_ident(parent, name_node, source_bytes)
}

fn extract_qualified_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    let scope_node = parent.child_by_field_name("scope")?;
    distinct_scope_tail(scope_node, name_node, source_bytes, "::")
}

fn extract_node_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    if let Some(custom) = lang
        .semantics()
        .extract_custom_scope(parent, name_node, source_bytes)
    {
        return Some(custom);
    }
    let kinds = lang.semantics().syntax_kinds();
    let kind = parent.kind();
    if kinds.scoped_paths.contains(&kind) {
        extract_qualified_scope(parent, name_node, source_bytes)
    } else if kinds.scope_containers.contains(&kind) {
        extract_named_container_scope(parent, name_node, source_bytes)
    } else {
        None
    }
}

pub(super) fn find_enclosing_scope(
    root: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    ancestors(root, name_node)
        .into_iter()
        .find_map(|parent| extract_node_scope(parent, name_node, source_bytes, lang))
}
