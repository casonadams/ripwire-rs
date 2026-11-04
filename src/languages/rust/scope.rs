use super::use_tree::collect_use_tree;

pub(super) fn rust_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "closure_expression"
            | "block"
            | "match_arm"
            | "for_expression"
            | "if_expression"
            | "while_expression"
    )
}

pub(super) fn rust_binding_target(
    parent: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
) -> bool {
    let field = match parent.kind() {
        "let_declaration" | "let_condition" | "parameter" | "for_expression" => "pattern",
        "const_item" | "static_item" | "function_item" => "name",
        "closure_parameters" => return true,
        "match_pattern" => return true,
        _ => return false,
    };
    super::super::reads::in_field(parent, node, field)
}

/// The scope `node` is bound in, given its ancestors from the root down to its parent.
fn rust_binding_scope_in<'a>(
    node: tree_sitter::Node<'a>,
    ancestors: &[tree_sitter::Node<'a>],
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "identifier" {
        return None;
    }
    for (depth, &parent) in ancestors.iter().enumerate().rev() {
        if rust_binding_target(parent, node) {
            let above = if matches!(
                parent.kind(),
                "const_item" | "static_item" | "function_item"
            ) {
                &ancestors[..depth]
            } else {
                ancestors
            };
            return above
                .iter()
                .rev()
                .copied()
                .find(|scope| rust_scope(scope.kind()));
        }
        if rust_scope(parent.kind()) {
            break;
        }
    }
    None
}

pub(super) fn rust_binding_range_in(
    node: tree_sitter::Node<'_>,
    ancestors: &[tree_sitter::Node<'_>],
) -> Option<std::ops::Range<usize>> {
    let scope = rust_binding_scope_in(node, ancestors)?;
    let mut span = scope.byte_range();
    for parent in ancestors.iter().rev() {
        if parent.kind() == "let_declaration" {
            span.start = parent.end_byte();
            break;
        }
        if *parent == scope {
            break;
        }
    }
    Some(span)
}

#[derive(Clone, PartialEq)]
pub(super) enum NameBinding {
    Local,
    Import(String),
}

/// Type-name bindings keyed by the AST id of the container that holds the declaration.
#[derive(Default)]
pub(super) struct LexicalNames {
    scopes: std::collections::HashMap<usize, std::collections::HashMap<String, NameBinding>>,
}

impl LexicalNames {
    fn bind(&mut self, container: tree_sitter::Node<'_>, name: &str, binding: NameBinding) {
        let names = self.scopes.entry(container.id()).or_default();
        match names.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(binding);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if *slot.get() != binding {
                    *slot.get_mut() = NameBinding::Local;
                }
            }
        }
    }

    /// The binding of `name` in the nearest enclosing container that declares it.
    pub(super) fn lookup(&self, at: tree_sitter::Node<'_>, name: &str) -> Option<&NameBinding> {
        let mut curr = Some(at);
        while let Some(n) = curr {
            if let Some(binding) = self.scopes.get(&n.id()).and_then(|names| names.get(name)) {
                return Some(binding);
            }
            curr = n.parent();
        }
        None
    }
}

pub(super) fn field_text<'s>(
    node: tree_sitter::Node<'_>,
    field: &str,
    source: &'s [u8],
) -> Option<&'s str> {
    node.child_by_field_name(field)?
        .utf8_text(source)
        .ok()
        .map(str::trim)
}

pub(super) fn collect_lexical_names(root: tree_sitter::Node<'_>, source: &[u8]) -> LexicalNames {
    let mut names = LexicalNames::default();
    for node in super::super::reads::nodes(root) {
        let Some(container) = node.parent() else {
            continue;
        };
        if container
            .parent()
            .is_some_and(|p| matches!(p.kind(), "impl_item" | "trait_item"))
        {
            continue;
        }
        match node.kind() {
            "struct_item" | "enum_item" | "union_item" | "trait_item" | "type_item" => {
                if let Some(name) = field_text(node, "name", source) {
                    names.bind(container, name, NameBinding::Local);
                }
            }
            "use_declaration" => {
                let mut imports = Vec::new();
                if let Some(arg) = node.child_by_field_name("argument") {
                    collect_use_tree(arg, "", source, &mut imports);
                }
                for (name, target) in imports {
                    names.bind(container, &name, NameBinding::Import(target));
                }
            }
            _ => {}
        }
    }
    names
}
