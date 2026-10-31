//! Calls to Python's builtin functions. A bare `len(x)` names the builtin unless the module itself
//! binds that name, so it never edges to a same-named function of another module.

use crate::languages::reads::nodes;
use tree_sitter::Node;

/// Builtin functions (CPython `builtins` module). Decorator builtins (`property`,
/// `staticmethod`, `classmethod`) are included: a bare `@property` names the builtin.
const PY_BUILTIN_FUNCTIONS: &[&str] = &[
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

/// True when `node` is the callee of a bare call (or a decorator) named after a builtin that the
/// module does not bind itself. Such a call is a builtin, never a same-named function elsewhere.
pub(super) fn is_builtin_call(node: Node<'_>, source: &[u8]) -> bool {
    let Some(name) = node.utf8_text(source).ok() else {
        return false;
    };
    PY_BUILTIN_FUNCTIONS.contains(&name)
        && is_bare_callee(node)
        && !module_binds(node, name, source)
}

fn is_bare_callee(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "decorator" => true,
        "call" => parent.child_by_field_name("function") == Some(node),
        _ => false,
    }
}

/// True when the module, outside function and class bodies, binds `name` by a definition, an
/// import, or an assignment. Bodies are skipped: a local of that name cannot shadow a module-level
/// call from another scope.
fn module_binds(node: Node<'_>, name: &str, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "function_definition" | "class_definition" => {
                if current
                    .child_by_field_name("name")
                    .is_some_and(|n| n.utf8_text(source) == Ok(name))
                {
                    return true;
                }
                continue;
            }
            "lambda" => continue,
            "import_statement" | "import_from_statement" => {
                if nodes(current).any(|n| n.kind() == "identifier" && n.utf8_text(source) == Ok(name))
                {
                    return true;
                }
                continue;
            }
            "assignment" if assigns_name(current, name, source) => return true,
            _ => {}
        }
        let mut cursor = current.walk();
        stack.extend(current.named_children(&mut cursor));
    }
    false
}

fn assigns_name(assignment: Node<'_>, name: &str, source: &[u8]) -> bool {
    assignment
        .child_by_field_name("left")
        .is_some_and(|left| left.kind() == "identifier" && left.utf8_text(source) == Ok(name))
}
