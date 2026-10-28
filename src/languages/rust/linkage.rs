//! Internal linkage for `--dead-code`: a Rust function is internal only when nothing outside the
//! crate's private scope can reach it by name, by export, or by the test and bench harness.

use tree_sitter::Node;

const EXTERNAL_ATTRIBUTE_WORDS: &[&str] = &[
    "test",
    "bench",
    "no_mangle",
    "export_name",
    "link_section",
    "used",
    "cfg",
];

pub(super) fn is_rust_internal_function(root: Node<'_>, name_byte: usize, source: &[u8]) -> bool {
    let Some(name_node) = root.descendant_for_byte_range(name_byte, name_byte) else {
        return false;
    };
    let Some(item) = enclosing_function_item(name_node) else {
        return false;
    };
    has_private_visibility(item)
        && !has_extern_modifier(item, source)
        && !is_named_main(item, source)
        && !has_external_ancestor(item, source)
}

fn enclosing_function_item(node: Node<'_>) -> Option<Node<'_>> {
    let mut curr = Some(node);
    while let Some(n) = curr {
        if n.kind() == "function_item" {
            return Some(n);
        }
        curr = n.parent();
    }
    None
}

fn has_private_visibility(item: Node<'_>) -> bool {
    let mut cursor = item.walk();
    item.children(&mut cursor)
        .all(|child| child.kind() != "visibility_modifier")
}

fn has_extern_modifier(item: Node<'_>, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    item.children(&mut cursor)
        .filter(|child| child.kind() == "function_modifiers")
        .any(|child| has_word(child, source, "extern"))
}

fn is_named_main(item: Node<'_>, source: &[u8]) -> bool {
    item.child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        == Some("main")
}

fn has_external_ancestor(item: Node<'_>, source: &[u8]) -> bool {
    let mut curr = Some(item);
    while let Some(n) = curr {
        if is_external_container(n)
            || has_external_attribute(n, source)
        {
            return true;
        }
        curr = n.parent();
    }
    false
}

fn is_external_container(node: Node<'_>) -> bool {
    match node.kind() {
        "trait_item" | "macro_definition" => true,
        "impl_item" => node.child_by_field_name("trait").is_some(),
        _ => false,
    }
}

fn has_external_attribute(node: Node<'_>, source: &[u8]) -> bool {
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        match sibling.kind() {
            "attribute_item" => {
                if EXTERNAL_ATTRIBUTE_WORDS
                    .iter()
                    .any(|word| has_word(sibling, source, word))
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        prev = sibling.prev_sibling();
    }
    false
}

fn has_word(node: Node<'_>, source: &[u8], word: &str) -> bool {
    node.utf8_text(source).is_ok_and(|text| {
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|token| token == word)
    })
}

