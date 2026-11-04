use crate::languages::DefCaptureGateContext;

const PYTHON_ENUM_BASES: &[&str] = &[
    "Enum",
    "IntEnum",
    "StrEnum",
    "Flag",
    "IntFlag",
    "ReprEnum",
    "Choices",
    "TextChoices",
    "IntegerChoices",
];

pub(super) fn is_staticmethod(ctx: &DefCaptureGateContext<'_>) -> bool {
    has_decorator(ctx, |name| name == "staticmethod")
}

pub(super) fn has_decorator(ctx: &DefCaptureGateContext<'_>, matches: impl Fn(&str) -> bool) -> bool {
    let Some(decorated) = ctx
        .def_node
        .and_then(|def| def.parent())
        .filter(|p| p.kind() == "decorated_definition")
    else {
        return false;
    };
    let mut cursor = decorated.walk();
    decorated.children(&mut cursor).any(|d| {
        d.kind() == "decorator"
            && d.utf8_text(ctx.source_bytes)
                .is_ok_and(|text| matches(text.trim_start_matches('@').trim()))
    })
}

fn has_enclosing_method_in_class(node: tree_sitter::Node<'_>) -> bool {
    let mut saw_fn = false;
    let mut curr = node.parent();
    while let Some(up) = curr {
        match up.kind() {
            "function_definition" => saw_fn = true,
            "class_definition" => return saw_fn,
            _ => {}
        }
        curr = up.parent();
    }
    false
}

pub(super) fn is_py_self_field_capture(
    name_node: Option<tree_sitter::Node<'_>>,
    def_node: Option<tree_sitter::Node<'_>>,
    src: &[u8],
) -> bool {
    let (Some(nn), Some(dn)) = (name_node, def_node) else {
        return false;
    };
    let Some(attr) = nn.parent().filter(|p| p.kind() == "attribute") else {
        return false;
    };
    let is_self = attr
        .child_by_field_name("object")
        .is_some_and(|o| o.kind() == "identifier" && o.utf8_text(src) == Ok("self"));
    is_self && has_enclosing_method_in_class(dn)
}

fn is_py_enum_base_node(base: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    let ident = if base.kind() == "attribute" {
        base.child_by_field_name("attribute")
    } else {
        Some(base)
    };
    let Some(node) = ident.filter(|n| n.kind() == "identifier") else {
        return false;
    };
    node.utf8_text(src)
        .is_ok_and(|name| PYTHON_ENUM_BASES.contains(&name))
}

pub(super) fn is_py_enum_member_target(name_node: Option<tree_sitter::Node<'_>>, src: &[u8]) -> bool {
    let Some(cls) = name_node
        .and_then(|n| n.parent())
        .and_then(|n| n.parent())
        .and_then(|n| n.parent())
        .and_then(|n| n.parent())
        .filter(|c| c.kind() == "class_definition")
    else {
        return false;
    };
    let Some(bases) = cls.child_by_field_name("superclasses") else {
        return false;
    };
    (0..bases.named_child_count())
        .filter_map(|i| bases.named_child(i))
        .any(|b| is_py_enum_base_node(b, src))
}
