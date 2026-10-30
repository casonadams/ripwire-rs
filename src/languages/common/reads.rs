use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingSpan {
    pub range: std::ops::Range<usize>,
    pub ty: Option<String>,
}

#[derive(Default)]
pub struct ReadBindings<'a> {
    scopes: HashMap<&'a str, Vec<BindingSpan>>,
    returns: HashMap<&'a str, String>,
    members: HashMap<(String, String), String>,
}

impl<'a> ReadBindings<'a> {
    pub fn collect(
        root: Node<'_>,
        source: &'a [u8],
        binding_scope: impl Fn(Node<'_>) -> Option<std::ops::Range<usize>>,
    ) -> Self {
        let mut result = Self::default();
        let mut cursor = root.walk();
        loop {
            let node = cursor.node();
            if let Some(scope) = binding_scope(node)
                && let Ok(name) = node.utf8_text(source)
            {
                result.insert(name, scope, None);
            }
            if !advance(&mut cursor) {
                return result;
            }
        }
    }

    /// [`Self::collect`] for a scope finder that needs the node's ancestors. They are kept on a stack from the
    /// one top-down walk, because `Node::parent` re-descends from the root on every call.
    pub fn collect_with_ancestors(
        root: Node<'_>,
        source: &'a [u8],
        binding_scope: impl Fn(Node<'_>, &[Node<'_>]) -> Option<std::ops::Range<usize>>,
    ) -> Self {
        let mut result = Self::default();
        walk_with_ancestors(root, |node, ancestors| {
            if let Some(scope) = binding_scope(node, ancestors)
                && let Ok(name) = node.utf8_text(source)
            {
                result.insert(name, scope, None);
            }
        });
        result
    }

    pub fn insert(&mut self, name: &'a str, range: std::ops::Range<usize>, ty: Option<String>) {
        let spans = self.scopes.entry(name).or_default();
        if let Some(existing) = spans.iter_mut().find(|s| s.range == range) {
            if existing.ty.is_none() {
                existing.ty = ty;
            }
        } else {
            spans.push(BindingSpan { range, ty });
        }
    }

    pub fn shadows(&self, name: &str, byte: usize) -> bool {
        self.scopes
            .get(name)
            .is_some_and(|spans| spans.iter().any(|span| span.range.contains(&byte)))
    }
    pub fn type_at(&self, name: &str, byte: usize) -> Option<&str> {
        let spans = self.scopes.get(name)?;
        spans
            .iter()
            .filter(|span| span.range.contains(&byte))
            .max_by_key(|span| span.range.start)
            .and_then(|span| span.ty.as_deref())
    }

    /// Records the declared result type of a function named `name`, for calls `name()`.
    pub fn insert_return(&mut self, name: &'a str, ty: String) {
        self.returns.insert(name, ty);
    }

    /// Declared result type of a function named `name`, when one is annotated.
    pub fn return_type(&self, name: &str) -> Option<&str> {
        self.returns.get(name).map(String::as_str)
    }

    /// Records the declared type of member `member` of the named type `owner` (a class field).
    pub fn insert_member(&mut self, owner: &str, member: &str, ty: String) {
        self.members
            .insert((owner.to_string(), member.to_string()), ty);
    }

    /// Declared type of member `member` of the named type `owner`, when recorded.
    pub fn member_type(&self, owner: &str, member: &str) -> Option<&str> {
        self.members
            .get(&(owner.to_string(), member.to_string()))
            .map(String::as_str)
    }
}

pub(crate) fn advance(cursor: &mut tree_sitter::TreeCursor<'_>) -> bool {
    if cursor.goto_first_child() {
        return true;
    }
    while !cursor.goto_next_sibling() {
        if !cursor.goto_parent() {
            return false;
        }
    }
    true
}

/// Visit every node under `root` in document order with its ancestors (root first, parent last), kept on a stack
/// by the walk itself, because `Node::parent` re-descends from the root on every call.
pub(crate) fn walk_with_ancestors<'t>(
    root: Node<'t>,
    mut visit: impl FnMut(Node<'t>, &[Node<'t>]),
) {
    let mut cursor = root.walk();
    let mut ancestors: Vec<Node<'t>> = Vec::new();
    loop {
        let node = cursor.node();
        visit(node, &ancestors);
        if cursor.goto_first_child() {
            ancestors.push(node);
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
            ancestors.pop();
        }
    }
}

pub(crate) fn nodes(root: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    let mut cursor = root.walk();
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let node = cursor.node();
        done = !advance(&mut cursor);
        Some(node)
    })
}

pub(crate) fn in_field(parent: Node<'_>, node: Node<'_>, field: &str) -> bool {
    parent.child_by_field_name(field).is_some_and(|child| {
        child.start_byte() <= node.start_byte() && child.end_byte() >= node.end_byte()
    })
}

pub(crate) fn enclosing_scope<'a>(
    node: Node<'a>,
    is_scope: impl Fn(&str) -> bool,
) -> Option<Node<'a>> {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if is_scope(parent.kind()) {
            return Some(parent);
        }
        ancestor = parent.parent();
    }
    None
}

pub(crate) fn value_position(
    node: Node<'_>,
    excluded: impl Fn(Node<'_>, Node<'_>) -> bool,
    is_scope: impl Fn(&str) -> bool,
) -> bool {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if excluded(parent, node) {
            return false;
        }
        if is_scope(parent.kind()) {
            break;
        }
        ancestor = parent.parent();
    }
    true
}
