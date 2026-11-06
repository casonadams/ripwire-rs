use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingSpan {
    pub range: std::ops::Range<usize>,
    pub ty: Option<String>,
    /// The element type of a generic collection bound here (`List<Kids>` binds `Kids`).
    pub element: Option<String>,
    /// The function whose result initializes this binding (`x := f()`), typed across files by the project pass.
    pub call: Option<String>,
}

#[derive(Default)]
pub struct ReadBindings<'a> {
    scopes: HashMap<&'a str, Vec<BindingSpan>>,
    returns: HashMap<&'a str, String>,
    members: HashMap<(String, String), String>,
    /// Element type of a collection-typed member, keyed `(owner, member)`: `Iterable<Kid> kids()` records `Kid`.
    elements: HashMap<(String, String), String>,
    /// Declared type of a module-level name, read where no local binding of that name is in scope.
    /// `None` marks names whose module-level bindings disagree.
    module: HashMap<&'a str, Option<String>>,
}

/// The entry of member `member` of `owner`, or of the nearest ancestor that declares it, climbing the `EXTENDS`
/// entries (`superclass` reads the class name an `EXTENDS` entry holds).
pub fn inherited_entry<'m, V>(
    members: &'m HashMap<(String, String), V>,
    owner: &str,
    member: &str,
    superclass: impl Fn(&V) -> &str,
) -> Option<&'m V> {
    let mut current = owner.to_string();
    for _ in 0..MAX_ANCESTORS {
        if let Some(entry) = members.get(&(current.clone(), member.to_string())) {
            return Some(entry);
        }
        current = superclass(members.get(&(current, EXTENDS.to_string()))?).to_string();
    }
    None
}

/// The member a class records for its superclass, so a member lookup can climb to the ancestor that declares it.
pub const EXTENDS: &str = "extends";
const MAX_ANCESTORS: usize = 16;

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
        let span = self.span_over(name, range);
        if span.ty.is_none() {
            span.ty = ty;
        }
    }

    /// Records the element type of the collection `name` bound over `range`.
    pub fn insert_element(
        &mut self,
        name: &'a str,
        range: std::ops::Range<usize>,
        element: String,
    ) {
        self.span_over(name, range).element = Some(element);
    }

    /// The span of `name` bound exactly over `range`, added when absent.
    fn span_over(&mut self, name: &'a str, range: std::ops::Range<usize>) -> &mut BindingSpan {
        let spans = self.scopes.entry(name).or_default();
        let index = match spans.iter().position(|s| s.range == range) {
            Some(index) => index,
            None => {
                spans.push(BindingSpan {
                    range,
                    ty: None,
                    element: None,
                    call: None,
                });
                spans.len() - 1
            }
        };
        &mut spans[index]
    }

    pub fn shadows(&self, name: &str, byte: usize) -> bool {
        self.scopes
            .get(name)
            .is_some_and(|spans| spans.iter().any(|span| span.range.contains(&byte)))
    }

    /// The declared type of `name` at `byte`: the innermost local binding in scope, else the
    /// module-level binding of the same name.
    pub fn type_at(&self, name: &str, byte: usize) -> Option<&str> {
        match self.span_at(name, byte) {
            Some(span) => span.ty.as_deref(),
            None => self.module.get(name)?.as_deref(),
        }
    }

    /// Records that the binding of `name` over `range` takes the result of the function `callee`.
    pub fn insert_call(&mut self, name: &'a str, range: std::ops::Range<usize>, callee: String) {
        self.span_over(name, range).call = Some(callee);
    }

    /// The function whose result initializes the innermost binding of `name` at `byte`.
    pub fn call_at(&self, name: &str, byte: usize) -> Option<&str> {
        self.span_at(name, byte)?.call.as_deref()
    }

    /// The element type of the collection `name` at `byte`, when its innermost binding records one.
    pub fn element_at(&self, name: &str, byte: usize) -> Option<&str> {
        self.span_at(name, byte)?.element.as_deref()
    }

    fn span_at(&self, name: &str, byte: usize) -> Option<&BindingSpan> {
        self.scopes
            .get(name)?
            .iter()
            .filter(|span| span.range.contains(&byte))
            .max_by_key(|span| span.range.start)
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

    /// Declared type of member `member` of `owner`, or of the nearest ancestor that declares it (climbing the
    /// `EXTENDS` members each class records).
    pub fn inherited_member_type(&self, owner: &str, member: &str) -> Option<&str> {
        inherited_entry(&self.members, owner, member, String::as_str).map(String::as_str)
    }

    /// Every recorded member type, as `(owner, member, type)`.
    pub fn member_entries(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.members
            .iter()
            .map(|((owner, member), ty)| (owner.as_str(), member.as_str(), ty.as_str()))
    }

    /// Records the element type of a collection-typed member `member` of `owner` (`Iterable<Kid> kids()` records
    /// `Kid`), so a call's result can type the element a lambda receives.
    pub fn insert_member_element(&mut self, owner: &str, member: &str, element: String) {
        self.elements
            .insert((owner.to_string(), member.to_string()), element);
    }

    /// Element type of member `member` of `owner`, as recorded by [`Self::insert_member_element`].
    pub fn member_element(&self, owner: &str, member: &str) -> Option<&str> {
        self.elements
            .get(&(owner.to_string(), member.to_string()))
            .map(String::as_str)
    }

    /// Records the declared type of a module-level binding of `name`. Bindings that disagree, or one
    /// that declares no type, leave the name untyped.
    pub fn insert_module(&mut self, name: &'a str, ty: Option<String>) {
        self.module
            .entry(name)
            .and_modify(|slot| {
                if *slot != ty {
                    *slot = None;
                }
            })
            .or_insert(ty);
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

/// Every named child of `node` whose kind is `kind`.
pub(crate) fn named_children_of_kind<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

pub(crate) fn in_field(parent: Node<'_>, node: Node<'_>, field: &str) -> bool {
    parent.child_by_field_name(field).is_some_and(|child| {
        child.start_byte() <= node.start_byte() && child.end_byte() >= node.end_byte()
    })
}

/// Per-language table of the syntax positions that are never a value read.
pub(crate) struct NonValueRules {
    /// Parent kinds whose every child is a non-value position.
    pub(crate) always: &'static [&'static str],
    /// `(kind, field)`: the node is exactly the parent's `field` child.
    pub(crate) is_field: &'static [(&'static str, &'static str)],
    /// `(kind, field)`: the node lies within the parent's `field` child.
    pub(crate) within_field: &'static [(&'static str, &'static str)],
}

pub(crate) fn non_value(
    parent: Node<'_>,
    node: Node<'_>,
    binding_target: impl Fn(Node<'_>, Node<'_>) -> bool,
    rules: &NonValueRules,
) -> bool {
    if binding_target(parent, node) {
        return true;
    }
    let kind = parent.kind();
    rules.always.contains(&kind)
        || rules
            .is_field
            .iter()
            .any(|&(k, field)| k == kind && parent.child_by_field_name(field) == Some(node))
        || rules
            .within_field
            .iter()
            .any(|&(k, field)| k == kind && in_field(parent, node, field))
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

/// The innermost ancestor of a node, given its ancestor chain nearest first, whose kind passes `is_scope`.
pub(crate) fn scope_in<'a>(
    ancestors: &[Node<'a>],
    is_scope: impl Fn(&str) -> bool,
) -> Option<Node<'a>> {
    ancestors
        .iter()
        .copied()
        .find(|parent| is_scope(parent.kind()))
}

/// The scope a binding of `node` is declared in. `ancestors` run from the root down to `node`'s parent. The
/// ancestor for which `declares(parent, node)` holds binds the name; when `owns_scope` holds for it, its own
/// enclosing scope is searched, otherwise the binding's.
pub(crate) fn binding_scope_in<'a>(
    node: Node<'a>,
    ancestors: &[Node<'a>],
    declares: impl Fn(Node<'a>, Node<'a>) -> bool,
    is_scope: impl Fn(&str) -> bool,
    owns_scope: impl Fn(&str) -> bool,
) -> Option<Node<'a>> {
    for (depth, &parent) in ancestors.iter().enumerate().rev() {
        if declares(parent, node) {
            let above = if owns_scope(parent.kind()) {
                &ancestors[..depth]
            } else {
                ancestors
            };
            return above
                .iter()
                .rev()
                .copied()
                .find(|scope| is_scope(scope.kind()));
        }
        if is_scope(parent.kind()) {
            break;
        }
    }
    None
}

/// `ancestors` is the chain of `node`'s ancestors, nearest first.
pub(crate) fn value_position(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    excluded: impl Fn(Node<'_>, Node<'_>) -> bool,
    is_scope: impl Fn(&str) -> bool,
) -> bool {
    for &parent in ancestors {
        if excluded(parent, node) {
            return false;
        }
        if is_scope(parent.kind()) {
            break;
        }
    }
    true
}

/// A read of `node` when its parent kind is one of `expression_parents` and no enclosing position excludes it.
pub(crate) fn value_read(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    expression_parents: &[&str],
    excluded: impl Fn(Node<'_>, Node<'_>) -> bool,
    is_scope: impl Fn(&str) -> bool,
) -> bool {
    ancestors
        .first()
        .is_some_and(|parent| expression_parents.contains(&parent.kind()))
        && value_position(node, ancestors, excluded, is_scope)
}
