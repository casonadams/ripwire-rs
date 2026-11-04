/// Decorators that wrap or retype a definition without registering it anywhere.
const NON_REGISTERING_DECORATORS: &[&str] = &[
    "staticmethod",
    "classmethod",
    "property",
    "cached_property",
    "wraps",
    "lru_cache",
    "cache",
    "override",
    "overload",
    "abstractmethod",
    "setter",
    "getter",
    "deleter",
];

/// Whether a decorator registers what it decorates: its name (the last segment before a call,
/// `app.route(..)` -> `route`) is not a pure wrapper.
fn is_registering_decorator(decorator: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    decorator.kind() == "decorator"
        && decorator.utf8_text(source).is_ok_and(|text| {
            let head = text
                .trim_start_matches('@')
                .split('(')
                .next()
                .unwrap_or("")
                .trim();
            let name = head.rsplit('.').next().unwrap_or(head);
            !NON_REGISTERING_DECORATORS.contains(&name)
        })
}

/// Whether `decorated` carries a registering decorator: as a child (Python's
/// `decorated_definition`, a TypeScript class) or as a preceding sibling (a TypeScript class member).
pub(crate) fn has_registering_decorator(decorated: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = decorated.walk();
    let child = decorated
        .children(&mut cursor)
        .any(|d| is_registering_decorator(d, source));
    let sibling = std::iter::successors(decorated.prev_named_sibling(), |n| n.prev_named_sibling())
        .take_while(|n| n.kind() == "decorator")
        .any(|d| is_registering_decorator(d, source));
    child || sibling
}
