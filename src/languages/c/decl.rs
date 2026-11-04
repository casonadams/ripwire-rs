fn matches_child_token(
    ch: tree_sitter::Node<'_>,
    src: &[u8],
    kind_filter: &str,
    allow_anon: bool,
    tokens: &[&str],
) -> bool {
    let named = ch.is_named();
    if (named && ch.kind() != kind_filter) || (!named && !allow_anon) {
        return false;
    }
    ch.utf8_text(src).is_ok_and(|t| tokens.contains(&t))
}

pub(super) fn has_child_token(
    node: tree_sitter::Node<'_>,
    src: &[u8],
    kind_filter: &str,
    allow_anon: bool,
    tokens: &[&str],
) -> bool {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .any(|ch| matches_child_token(ch, src, kind_filter, allow_anon, tokens))
}

pub(super) fn has_const_qualifier(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    has_child_token(
        node,
        src,
        "type_qualifier",
        false,
        &["const", "constexpr", "constinit"],
    )
}

pub(super) fn has_static_storage(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    has_child_token(node, src, "storage_class_specifier", false, &["static"])
}

fn function_declarator_node(mut node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    loop {
        if node.kind() == "function_declarator" {
            return Some(node);
        }
        node = node.child_by_field_name("declarator")?;
    }
}

fn matching_function_declaration(
    declaration: tree_sitter::Node<'_>,
    definition: tree_sitter::Node<'_>,
    source: &[u8],
    overloads: bool,
) -> bool {
    let Some(signature) = definition
        .child_by_field_name("declarator")
        .and_then(function_declarator_node)
    else {
        return false;
    };
    let fields: &[&str] = if overloads {
        &["declarator", "parameters"]
    } else {
        &["declarator"]
    };
    let mut cursor = declaration.walk();
    declaration
        .children_by_field_name("declarator", &mut cursor)
        .filter_map(function_declarator_node)
        .any(|candidate| {
            fields.iter().all(|field| {
                candidate
                    .child_by_field_name(field)
                    .zip(signature.child_by_field_name(field))
                    .is_some_and(|(left, right)| {
                        source[left.byte_range()] == source[right.byte_range()]
                    })
            })
        })
}

fn is_enclosing_file_scope(decl_node: tree_sitter::Node<'_>) -> bool {
    let mut p = decl_node.parent();
    while let Some(parent) = p {
        match parent.kind() {
            "compound_statement"
            | "field_declaration_list"
            | "struct_specifier"
            | "class_specifier"
            | "interface_definition" => return false,
            "translation_unit" => return true,
            _ => p = parent.parent(),
        }
    }
    false
}

pub(crate) fn is_c_internal_function(
    root: tree_sitter::Node<'_>,
    name_byte: usize,
    source: &[u8],
    overloads: bool,
) -> bool {
    if name_byte >= source.len() {
        return false;
    }
    let Some(name_node) = root.descendant_for_byte_range(name_byte, name_byte) else {
        return false;
    };
    let mut curr = Some(name_node);
    let mut decl = None;
    while let Some(n) = curr {
        match n.kind() {
            "function_definition" | "declaration" => {
                decl = Some(n);
                break;
            }
            _ => curr = n.parent(),
        }
    }
    let Some(decl_node) = decl else {
        return false;
    };
    if !is_enclosing_file_scope(decl_node) {
        return false;
    }
    if has_static_storage(decl_node, source) {
        return true;
    }
    if let Some(parent) = decl_node.parent()
        && parent.kind() == "template_declaration"
        && has_static_storage(parent, source)
    {
        return true;
    }
    let mut previous = decl_node.prev_named_sibling();
    while let Some(sibling) = previous {
        if sibling.kind() == "declaration"
            && has_static_storage(sibling, source)
            && matching_function_declaration(sibling, decl_node, source, overloads)
        {
            return true;
        }
        previous = sibling.prev_named_sibling();
    }
    false
}
