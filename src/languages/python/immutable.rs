use crate::languages::DefCaptureGateContext;

/// Calls that build a type object (`namedtuple(...)`, `NewType(...)`, `Enum(...)`): a type, not state.
const PY_TYPE_FACTORIES: &[&str] = &[
    "namedtuple",
    "NamedTuple",
    "NewType",
    "TypeVar",
    "TypedDict",
    "Enum",
    "IntEnum",
    "StrEnum",
    "Flag",
    "IntFlag",
];

/// A literal scalar (number, string without interpolation, boolean, None) or a tuple of them: its
/// value cannot change. Tuples are immutable in Python, unlike the JS arrays the TS rule excludes.
fn is_py_scalar_literal(node: tree_sitter::Node<'_>) -> bool {
    match node.kind() {
        "integer" | "float" | "true" | "false" | "none" => true,
        "string" => {
            let mut cursor = node.walk();
            !node.named_children(&mut cursor).any(|c| c.kind() == "interpolation")
        }
        "unary_operator" => node
            .child_by_field_name("argument")
            .is_some_and(|arg| matches!(arg.kind(), "integer" | "float")),
        "tuple" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).all(is_py_scalar_literal)
        }
        _ => false,
    }
}

/// A module-level binding whose value is a literal scalar or a type-factory call: an immutable
/// binding whose reads are never state. A name some `global` statement declares can be rebound
/// from a function, so it stays state.
pub(super) fn is_py_immutable_binding(ctx: &DefCaptureGateContext<'_>) -> bool {
    let Some(name_node) = ctx.name_node else {
        return false;
    };
    let Some(assignment) = name_node
        .parent()
        .filter(|node| node.kind() == "assignment")
    else {
        return false;
    };
    let Some(value) = assignment.child_by_field_name("right") else {
        return false;
    };
    let constant = is_py_scalar_literal(value)
        || (value.kind() == "call"
            && value
                .child_by_field_name("function")
                .and_then(|callee| match callee.kind() {
                    "attribute" => callee.child_by_field_name("attribute"),
                    _ => Some(callee),
                })
                .and_then(|name| name.utf8_text(ctx.source_bytes).ok())
                .is_some_and(|name| PY_TYPE_FACTORIES.contains(&name)));
    let mut module = name_node;
    while let Some(parent) = module.parent() {
        module = parent;
    }
    constant && !is_py_declared_global(module, ctx.name, ctx.source_bytes)
}

/// True when some `global` statement in the module names `name`.
fn is_py_declared_global(module: tree_sitter::Node<'_>, name: &str, source: &[u8]) -> bool {
    let mut stack = vec![module];
    while let Some(node) = stack.pop() {
        if node.kind() == "global_statement" {
            let mut cursor = node.walk();
            if node
                .named_children(&mut cursor)
                .any(|child| child.utf8_text(source).ok() == Some(name))
            {
                return true;
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    false
}
