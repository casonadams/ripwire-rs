/// Nesting bound for same-file constant chains, so a cyclic reference cannot recurse forever.
const SCALAR_DEPTH_LIMIT: u8 = 8;

/// True for an initializer whose value is provably a string, number, boolean, bigint or `null`: a literal, a
/// template of scalars, an environment read (`process.env.X`, `import.meta.env.X`), a unary, arithmetic, logical
/// or comparison operator over scalars, or a same-file module `const` that is itself scalar. Objects, arrays,
/// calls and imports are never proven scalar.
pub(super) fn is_scalar_expression(
    node: tree_sitter::Node<'_>,
    module: tree_sitter::Node<'_>,
    source: &[u8],
    depth: u8,
) -> bool {
    if depth > SCALAR_DEPTH_LIMIT {
        return false;
    }
    let scalar = |child: tree_sitter::Node<'_>| is_scalar_expression(child, module, source, depth + 1);
    match node.kind() {
        "number" | "string" | "true" | "false" | "null" => true,
        "identifier" => node
            .utf8_text(source)
            .is_ok_and(|name| module_const_is_scalar(module, name, source, depth)),
        "template_string" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).all(|child| {
                child.kind() != "template_substitution" || child.named_child(0).is_some_and(scalar)
            })
        }
        "unary_expression" => {
            node.child(0).is_some_and(|op| matches!(op.kind(), "-" | "+" | "!" | "~"))
                && node.child_by_field_name("argument").is_some_and(scalar)
        }
        "as_expression" | "satisfies_expression" | "parenthesized_expression" | "non_null_expression" => {
            node.named_child(0).is_some_and(scalar)
        }
        "binary_expression" => {
            let op = node.child_by_field_name("operator").map(|op| op.kind());
            match op {
                Some("===" | "!==" | "==" | "!=" | "<" | ">" | "<=" | ">=") => true,
                Some("+" | "-" | "*" | "/" | "%" | "**" | "??" | "||" | "&&") => {
                    ["left", "right"].into_iter().all(|field| {
                        node.child_by_field_name(field).is_some_and(scalar)
                    })
                }
                _ => false,
            }
        }
        "member_expression" | "subscript_expression" => is_env_read(node, source),
        _ => false,
    }
}

/// `process.env.NAME` / `process.env["NAME"]` (or `import.meta.env`): an environment value, a string or undefined.
fn is_env_read(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    node.child_by_field_name("object")
        .and_then(|object| object.utf8_text(source).ok())
        .is_some_and(|text| matches!(text, "process.env" | "import.meta.env"))
}

/// True when the module-level `const name = value` declaration in this file has a scalar value. Only
/// top-level declarations are consulted: an initializer's bare identifier refers to the module scope.
fn module_const_is_scalar(
    module: tree_sitter::Node<'_>,
    name: &str,
    source: &[u8],
    depth: u8,
) -> bool {
    let mut cursor = module.walk();
    for stmt in module.named_children(&mut cursor) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(decl) => decl,
                None => continue,
            }
        } else {
            stmt
        };
        if decl.kind() != "lexical_declaration" || decl.child(0).is_none_or(|k| k.kind() != "const") {
            continue;
        }
        let mut inner = decl.walk();
        for declarator in decl.named_children(&mut inner) {
            let is_name = declarator
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|text| text == name);
            if is_name {
                return declarator
                    .child_by_field_name("value")
                    .is_some_and(|value| is_scalar_expression(value, module, source, depth + 1));
            }
        }
    }
    false
}

/// The outermost node of a parse tree: the module a top-level declaration belongs to.
pub(super) fn module_root(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    root
}
