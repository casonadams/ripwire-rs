//! Java and C# grammars label types and names with fields, so bindings read them by field name.

use super::reads::{EXTENDS, ReadBindings, enclosing_scope, nodes};
use crate::languages::{Implementation, is_plain_type_name, is_type_path};
use std::collections::HashMap;
use tree_sitter::Node;

/// Marks a receiver typed through a transparent wrapper (`std::unique_ptr<T>` types `T`, marked).
pub(crate) const WRAPPED_RECEIVER: &str = "~w";

pub struct TypedSyntax {
    pub classes: &'static [&'static str],
    pub supertype_clauses: &'static [&'static str],
    pub params: &'static [&'static str],
    pub functions: &'static [&'static str],
    pub locals: &'static [&'static str],
    pub fields: &'static [&'static str],
    pub declarators: &'static [&'static str],
    pub blocks: &'static [&'static str],
    pub constructions: &'static [&'static str],
    pub inferred: &'static [&'static str],
    pub assignments: &'static [&'static str],
    pub calls: &'static [(&'static str, &'static str)],
    pub return_type_field: &'static str,
    pub superclass: Option<(&'static str, &'static str)>,
    /// Scala: `val x: T` is a declaration whose own declarator is named by `pattern`.
    pub declaration_name: Option<&'static str>,
    pub class_params: &'static [&'static str],
    /// Wrapper templates whose one type argument is the receiver's type (`std::unique_ptr<T>`).
    pub transparent_wrappers: &'static [&'static str],
    /// Nested classes are named by the class around them (`Outer.Inner`): members and types recorded under a nested
    /// class, and bare type names that name a class nested in an enclosing class, are qualified by that class.
    pub scoped_nesting: bool,
}

/// Field facts per `(class, field)`: `None` marks fields given two different types.
pub type FieldFacts = HashMap<(String, String), Option<String>>;

pub fn record_fact(facts: &mut FieldFacts, owner: &str, field: &str, ty: String) {
    agree(facts, (owner.to_string(), field.to_string()), Some(ty));
}

/// Notes `value` under `key`: the first value stands, and a later value that differs turns the key unknown
/// (`None`), so a key whose sites disagree on its type names no single type.
pub fn agree<K: Eq + std::hash::Hash, V: PartialEq>(
    facts: &mut HashMap<K, Option<V>>,
    key: K,
    value: Option<V>,
) {
    facts
        .entry(key)
        .and_modify(|slot| {
            if *slot != value {
                *slot = None;
            }
        })
        .or_insert(value);
}

/// Gathers field facts from every class of `class_kinds` under `root`, with `facts` filling the explicit
/// and derived maps of each class body, then folds them into `bindings`: an explicit fact wins over a
/// derived one, and a field whose facts disagree stays untyped.
pub fn record_field_facts(
    bindings: &mut ReadBindings<'_>,
    root: Node<'_>,
    source: &[u8],
    class_kinds: &[&str],
    mut facts: impl FnMut(&ReadBindings<'_>, &str, Node<'_>, &mut FieldFacts, &mut FieldFacts),
) {
    let mut explicit = FieldFacts::new();
    let mut derived = FieldFacts::new();
    for (owner, body) in class_bodies(root, source, class_kinds) {
        facts(bindings, owner, body, &mut explicit, &mut derived);
    }
    for ((owner, field), ty) in &explicit {
        if let Some(ty) = ty {
            bindings.insert_member(owner, field, ty.clone());
        }
    }
    for ((owner, field), ty) in &derived {
        if explicit.contains_key(&(owner.clone(), field.clone())) {
            continue;
        }
        if let Some(ty) = ty {
            bindings.insert_member(owner, field, ty.clone());
        }
    }
}

/// The `(name, body)` of every class of `class_kinds` under `root` that has both.
fn class_bodies<'t, 's>(
    root: Node<'t>,
    source: &'s [u8],
    class_kinds: &[&str],
) -> Vec<(&'s str, Node<'t>)> {
    nodes(root)
        .filter(|class| class_kinds.contains(&class.kind()))
        .filter_map(|class| {
            let name = class.child_by_field_name("name")?.utf8_text(source).ok()?;
            Some((name, class.child_by_field_name("body")?))
        })
        .collect()
}

/// Every class of `class_kinds` with its own span, then one implementation per base name that
/// `base_names` yields for it. `class_name` returns `None` to skip a class.
pub fn class_implementations<'s>(
    root: Node<'_>,
    source: &'s [u8],
    class_kinds: &[&str],
    class_name: impl Fn(Node<'_>, &'s [u8]) -> Option<&'s str>,
    base_names: impl Fn(Node<'_>, &'s [u8]) -> Vec<&'s str>,
) -> Vec<Implementation> {
    let mut out = Vec::new();
    for class in nodes(root).filter(|n| class_kinds.contains(&n.kind())) {
        let Some(subject) = class_name(class, source) else {
            continue;
        };
        let span = (class.start_byte(), class.end_byte());
        out.push(implementation(subject, None, span));
        out.extend(
            base_names(class, source)
                .into_iter()
                .map(|base| implementation(subject, Some(base), span)),
        );
    }
    out
}

/// Java and C# shaped grammars: the classes and supertype clauses named by `syntax`.
pub fn typed_class_implementations(
    root: Node<'_>,
    source: &[u8],
    syntax: &TypedSyntax,
) -> Vec<Implementation> {
    class_implementations(
        root,
        source,
        syntax.classes,
        |class, source| {
            text_of(class.child_by_field_name("name"), source)
                .filter(|name| is_plain_type_name(name))
        },
        |class, source| {
            let mut cursor = class.walk();
            class
                .named_children(&mut cursor)
                .filter(|n| syntax.supertype_clauses.contains(&n.kind()))
                .flat_map(|clause| type_names(clause, source))
                .collect()
        },
    )
}

fn type_names<'a>(clause: Node<'_>, source: &'a [u8]) -> impl Iterator<Item = &'a str> {
    nodes(clause)
        .filter(|n| matches!(n.kind(), "type_identifier" | "identifier" | "name"))
        .filter_map(move |n| n.utf8_text(source).ok())
        .collect::<Vec<_>>()
        .into_iter()
}

#[must_use]
pub fn implementation(
    subject: &str,
    trait_path: Option<&str>,
    span: (usize, usize),
) -> Implementation {
    Implementation {
        subject: subject.to_string(),
        trait_path: trait_path.map(str::to_string),
        subject_bound: None,
        generic_subject: false,
        extends: false,
        start_byte: span.0,
        end_byte: span.1,
    }
}
pub fn typed_read_bindings<'a>(
    root: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::default();
    let mut returns = AgreedReturns::default();
    for node in nodes(root) {
        let kind = node.kind();
        if syntax.classes.contains(&kind) {
            record_class(&mut bindings, node, source, syntax);
        } else if syntax.params.contains(&kind) {
            record_parameter(&mut bindings, node, source, syntax);
        } else if syntax.locals.contains(&kind) {
            record_local(&mut bindings, node, source, syntax);
        } else if syntax.assignments.contains(&kind) {
            record_assignment(&mut bindings, node, source, syntax);
        } else if syntax.functions.contains(&kind) {
            record_return(&mut returns, node, source, syntax);
            record_out_of_line_this(&mut bindings, node, source);
        }
    }
    returns.insert_into(&mut bindings);
    bindings
}

#[derive(Default)]
pub struct AgreedReturns<'a>(HashMap<&'a str, Option<String>>);

impl<'a> AgreedReturns<'a> {
    pub fn note(&mut self, name: &'a str, ty: Option<String>) {
        self.0
            .entry(name)
            .and_modify(|seen| {
                if *seen != ty {
                    *seen = None;
                }
            })
            .or_insert(ty);
    }

    pub fn insert_into(self, bindings: &mut ReadBindings<'a>) {
        for (name, ty) in self.0 {
            if let Some(ty) = ty {
                bindings.insert_return(name, ty);
            }
        }
    }
}

fn record_return<'a>(
    returns: &mut AgreedReturns<'a>,
    function: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) {
    let Some(name) = text_of(function.child_by_field_name("name"), source) else {
        return;
    };
    let ty = text_of(
        function.child_by_field_name(syntax.return_type_field),
        source,
    )
    .filter(|ty| is_plain_type_name(ty))
    .map(str::to_string);
    returns.note(name, ty);
}

pub fn call_receiver_name<'a>(
    receiver: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) -> Option<&'a str> {
    let &(_, callee_field) = syntax
        .calls
        .iter()
        .find(|(kind, _)| *kind == receiver.kind())?;
    if receiver.child_by_field_name("object").is_some() {
        return None;
    }
    let callee = receiver.child_by_field_name(callee_field)?;
    text_of(Some(callee), source).filter(|name| is_plain_type_name(name))
}

fn record_assignment<'a>(
    bindings: &mut ReadBindings<'a>,
    assignment: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) {
    let (Some(left), Some(right)) = (
        assignment.child_by_field_name("left"),
        assignment.child_by_field_name("right"),
    ) else {
        return;
    };
    let (Some(name), Some(block)) = (
        text_of(Some(left), source).filter(|name| is_plain_type_name(name)),
        enclosing_scope(assignment, |kind| syntax.blocks.contains(&kind)),
    ) else {
        return;
    };
    let ty = syntax
        .constructions
        .contains(&right.kind())
        .then(|| construction_type(right, source))
        .flatten();
    bindings.insert(name, assignment.start_byte()..block.end_byte(), ty);
}

/// The key a class's members are recorded under: its name, qualified by the class around it when the language scopes
/// nested classes.
fn owner_key(class: Node<'_>, name: &str, source: &[u8], syntax: &TypedSyntax) -> String {
    if !syntax.scoped_nesting {
        return name.to_string();
    }
    match enclosing_scope(class, |kind| syntax.classes.contains(&kind))
        .and_then(|outer| text_of(outer.child_by_field_name("name"), source))
    {
        Some(outer) => format!("{outer}.{name}"),
        None => name.to_string(),
    }
}

fn is_class(node: Node<'_>, syntax: &TypedSyntax) -> bool {
    syntax.classes.contains(&node.kind())
}

/// A type written through its outer class (`Outer.Inner`), where the language scopes nested classes.
fn scoped_type_text(ty: Node<'_>, source: &[u8], syntax: &TypedSyntax) -> Option<String> {
    let text = ty.utf8_text(source).ok()?;
    (syntax.scoped_nesting && is_type_path(text) && !syntax.inferred.contains(&text))
        .then(|| text.to_string())
}

/// A bare type name written inside classes names the nested class of that name the nearest enclosing class declares
/// (`Outer.Inner`), else the name itself.
pub fn qualify_nested_type(
    site: Node<'_>,
    name: &str,
    source: &[u8],
    is_class: impl Fn(Node<'_>) -> bool,
    class_name: impl for<'s> Fn(Node<'_>, &'s [u8]) -> Option<&'s str>,
    body_of: impl Fn(Node<'_>) -> Option<Node<'_>>,
) -> String {
    let mut ancestor = site.parent();
    while let Some(node) = ancestor {
        if is_class(node)
            && body_of(node)
                .is_some_and(|body| declares_class(body, name, source, &is_class, &class_name))
            && let Some(outer) = class_name(node, source)
        {
            return format!("{outer}.{name}");
        }
        ancestor = node.parent();
    }
    name.to_string()
}

fn qualify_in_scope(site: Node<'_>, name: &str, source: &[u8], syntax: &TypedSyntax) -> String {
    if !syntax.scoped_nesting || !is_plain_type_name(name) {
        return name.to_string();
    }
    qualify_nested_type(
        site,
        name,
        source,
        |n| is_class(n, syntax),
        |n, source| text_of(n.child_by_field_name("name"), source),
        |n| n.child_by_field_name("body"),
    )
}

/// Whether the class body `body` directly declares a class named `name`: a child that `is_class` admits and
/// `class_name` names.
pub fn declares_class(
    body: Node<'_>,
    name: &str,
    source: &[u8],
    is_class: impl Fn(Node<'_>) -> bool,
    class_name: impl for<'s> Fn(Node<'_>, &'s [u8]) -> Option<&'s str>,
) -> bool {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .any(|child| is_class(child) && class_name(child, source) == Some(name))
}

fn record_class<'a>(
    bindings: &mut ReadBindings<'a>,
    class: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) {
    let (Some(name), Some(body)) = (
        text_of(class.child_by_field_name("name"), source),
        class.child_by_field_name("body"),
    ) else {
        return;
    };
    let owner = owner_key(class, name, source, syntax);
    bindings.insert("this", body.byte_range(), Some(owner.clone()));
    if let Some((clause_kind, keyword)) = syntax.superclass {
        let mut cursor = class.walk();
        let base = class
            .named_children(&mut cursor)
            .find(|n| n.kind() == clause_kind)
            .and_then(|clause| type_names(clause, source).next());
        if let Some(base) = base {
            bindings.insert(keyword, body.byte_range(), Some(base.to_string()));
            bindings.insert_member(&owner, EXTENDS, base.to_string());
        }
    }
    let mut cursor = body.walk();
    let body_fields: Vec<Node<'_>> = body
        .named_children(&mut cursor)
        .filter(|n| syntax.fields.contains(&n.kind()))
        .collect();
    let mut fields: Vec<(&str, Option<String>)> = body_fields
        .into_iter()
        .flat_map(|field| {
            let declared = declared_type(field, source, syntax);
            declarator_types(field, declared.as_deref(), source, syntax)
        })
        .collect();
    fields.extend(
        children_and_grandchildren(class)
            .filter(|n| syntax.class_params.contains(&n.kind()))
            .filter_map(|param| {
                let name = binding_name(param, source, syntax)?;
                Some((name, declared_type(param, source, syntax)))
            }),
    );
    for (field_name, ty) in fields {
        if let Some(ty) = &ty {
            bindings.insert_member(&owner, field_name, ty.clone());
        }
        bindings.insert(field_name, body.byte_range(), ty);
    }
    for (method, ty) in class_method_returns(body, source, syntax) {
        bindings.insert_member(&owner, &format!("{method}()"), ty);
    }
}

/// The declared result type of each method of a class body; an overloaded name counts only when its
/// overloads agree on the type.
fn class_method_returns<'a>(
    body: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) -> Vec<(&'a str, String)> {
    let mut returns = AgreedReturns::default();
    let mut cursor = body.walk();
    for method in body
        .named_children(&mut cursor)
        .filter(|n| syntax.functions.contains(&n.kind()))
    {
        let Some(name) = text_of(method.child_by_field_name("name"), source) else {
            continue;
        };
        let ty = text_of(method.child_by_field_name(syntax.return_type_field), source)
            .filter(|ty| is_plain_type_name(ty) || (syntax.scoped_nesting && is_type_path(ty)))
            .map(|ty| qualify_in_scope(method, ty, source, syntax));
        returns.note(name, ty);
    }
    returns
        .0
        .into_iter()
        .filter_map(|(name, ty)| Some((name, ty?)))
        .collect()
}

fn binding_name<'a>(node: Node<'_>, source: &'a [u8], syntax: &TypedSyntax) -> Option<&'a str> {
    if let Some(name) = node.child_by_field_name("name") {
        return text_of(Some(name), source);
    }
    let mut current = node.child_by_field_name(syntax.declaration_name?)?;
    loop {
        match current.kind() {
            "identifier" | "field_identifier" => return current.utf8_text(source).ok(),
            "function_declarator" => return None,
            kind if kind.ends_with("declarator") => {
                current = current
                    .child_by_field_name("declarator")
                    .or_else(|| current.named_child(0))?;
            }
            _ => return None,
        }
    }
}

fn record_parameter<'a>(
    bindings: &mut ReadBindings<'a>,
    param: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) {
    let Some(name) = binding_name(param, source, syntax) else {
        return;
    };
    // A for-each variable is bound in its loop alone; other parameters in their function.
    let scope = if param.kind() == "enhanced_for_statement" {
        param.byte_range()
    } else {
        let Some(function) = enclosing_scope(param, |kind| syntax.functions.contains(&kind)) else {
            return;
        };
        function.byte_range()
    };
    bindings.insert(name, scope, declared_type(param, source, syntax));
}

fn record_local<'a>(
    bindings: &mut ReadBindings<'a>,
    local: Node<'_>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) {
    let Some(block) = enclosing_scope(local, |kind| syntax.blocks.contains(&kind)) else {
        return;
    };
    let declared = declared_type(local, source, syntax);
    for (name, ty) in declarator_types(local, declared.as_deref(), source, syntax) {
        bindings.insert(name, local.start_byte()..block.end_byte(), ty);
    }
}

fn declarator_types<'a>(
    declaration: Node<'_>,
    declared: Option<&str>,
    source: &'a [u8],
    syntax: &TypedSyntax,
) -> Vec<(&'a str, Option<String>)> {
    if syntax.declaration_name.is_some() {
        let ty = declared
            .map(str::to_string)
            .or_else(|| constructed_type(declaration, source, syntax));
        return binding_name(declaration, source, syntax)
            .map(|name| vec![(name, ty)])
            .unwrap_or_default();
    }
    let mut out = Vec::new();
    for declarator in
        children_and_grandchildren(declaration).filter(|n| syntax.declarators.contains(&n.kind()))
    {
        let Some(name) = text_of(declarator.child_by_field_name("name"), source) else {
            continue;
        };
        let ty = declared
            .map(str::to_string)
            .or_else(|| constructed_type(declarator, source, syntax));
        out.push((name, ty));
    }
    out
}

/// C++ `ns::Box` declares a `Box` in namespace `ns`: the type is the bare name. A template `Box<int>` keeps its
/// argument list, which the cross-file pass matches against the specialization the tree declares, if any.
fn declared_base(ty: Node<'_>) -> Option<(Node<'_>, Option<Node<'_>>)> {
    match ty.kind() {
        "qualified_identifier" => declared_base(ty.child_by_field_name("name")?),
        "template_type" => Some((
            plain_name(ty.child_by_field_name("name")?)?,
            ty.child_by_field_name("arguments"),
        )),
        _ => Some((ty, None)),
    }
}

/// The bare identifier a name ends in (`ns::Box` names `Box`).
fn plain_name(name: Node<'_>) -> Option<Node<'_>> {
    match name.kind() {
        "qualified_identifier" => plain_name(name.child_by_field_name("name")?),
        "type_identifier" | "identifier" => Some(name),
        _ => None,
    }
}

/// The declared type a type node names: its bare name, with a template's argument list after it, spaces removed as a
/// specialization's scope spells it. `None` unless that name is plain, and never one of `inferred`.
pub(crate) fn declared_type_text(ty: Node<'_>, source: &[u8], inferred: &[&str]) -> Option<String> {
    let (base, arguments) = declared_base(ty)?;
    let text = base.utf8_text(source).ok()?;
    if !is_plain_type_name(text) || inferred.contains(&text) {
        return None;
    }
    let arguments: String = match arguments {
        Some(arguments) => arguments
            .utf8_text(source)
            .ok()?
            .split_whitespace()
            .collect(),
        None => String::new(),
    };
    Some(format!("{text}{arguments}"))
}

/// A body defined out of line, `void Box::get() {}`, has the class `Box` as its `this`.
fn record_out_of_line_this<'a>(
    bindings: &mut ReadBindings<'a>,
    function: Node<'_>,
    source: &'a [u8],
) {
    let (Some(declarator), Some(body)) = (
        function.child_by_field_name("declarator"),
        function.child_by_field_name("body"),
    ) else {
        return;
    };
    let Some(scope) = out_of_line_scope(declarator) else {
        return;
    };
    let Ok(text) = scope.utf8_text(source) else {
        return;
    };
    let class = text.rsplit("::").next().unwrap_or(text);
    if is_plain_type_name(class) {
        bindings.insert("this", body.byte_range(), Some(class.to_string()));
    }
}

/// The scope a qualified function declarator names (`ns::Box` of `ns::Box::get`), through any wrapping declarator.
fn out_of_line_scope(declarator: Node<'_>) -> Option<Node<'_>> {
    match declarator.kind() {
        "qualified_identifier" => declarator.child_by_field_name("scope"),
        "function_declarator" | "pointer_declarator" | "reference_declarator" => out_of_line_scope(
            declarator
                .child_by_field_name("declarator")
                .or_else(|| declarator.named_child(0))?,
        ),
        _ => None,
    }
}

/// C# fields wrap their type in a `variable_declaration` child.
fn declared_type(node: Node<'_>, source: &[u8], syntax: &TypedSyntax) -> Option<String> {
    let ty = node.child_by_field_name("type").or_else(|| {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .find_map(|child| child.child_by_field_name("type"))
    })?;
    let (inner, wrapped) = unwrap_transparent(ty, source, syntax.transparent_wrappers);
    let text = match declared_type_text(inner, source, syntax.inferred) {
        Some(text) => text,
        None => scoped_type_text(inner, source, syntax)?,
    };
    let text = qualify_in_scope(inner, &text, source, syntax);
    Some(if wrapped {
        format!("{text}{WRAPPED_RECEIVER}")
    } else {
        text
    })
}

/// The type a transparent wrapper hands its receiver: `T` of `std::unique_ptr<T>`, through any nesting. `wrapped` is
/// false for any other type.
pub(crate) fn unwrap_transparent<'t>(
    ty: Node<'t>,
    source: &[u8],
    wrappers: &[&str],
) -> (Node<'t>, bool) {
    let mut current = ty;
    let mut wrapped = false;
    while let Some(inner) = transparent_argument(current, source, wrappers) {
        current = inner;
        wrapped = true;
    }
    (current, wrapped)
}

/// The one plain type argument of a wrapper named in `wrappers`.
fn transparent_argument<'t>(ty: Node<'t>, source: &[u8], wrappers: &[&str]) -> Option<Node<'t>> {
    if !matches!(ty.kind(), "template_type" | "qualified_identifier") {
        return None;
    }
    let text: String = ty.utf8_text(source).ok()?.split_whitespace().collect();
    let head = text.split_once('<')?.0;
    if !wrappers.contains(&head) {
        return None;
    }
    let template = if ty.kind() == "template_type" {
        ty
    } else {
        ty.child_by_field_name("name")?
    };
    let arguments = template.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let mut named = arguments.named_children(&mut cursor);
    let only = named.next()?;
    if named.next().is_some() {
        return None;
    }
    if only.kind() != "type_descriptor" {
        return Some(only);
    }
    if only.child_by_field_name("declarator").is_some() {
        return None;
    }
    only.child_by_field_name("type")
}

fn constructed_type(declarator: Node<'_>, source: &[u8], syntax: &TypedSyntax) -> Option<String> {
    let mut cursor = declarator.walk();
    let construction = declarator
        .named_children(&mut cursor)
        .find(|n| syntax.constructions.contains(&n.kind()))?;
    construction_type(construction, source)
}

/// PHP `new Connector()` names the class in a bare `name` node; JavaScript `new C()` is its first named child.
pub(crate) fn construction_type(construction: Node<'_>, source: &[u8]) -> Option<String> {
    let ty = construction
        .child_by_field_name("type")
        .or_else(|| construction.named_child(0));
    let text = text_of(ty, source)?;
    is_plain_type_name(text).then(|| text.to_string())
}

fn children_and_grandchildren(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    let grandchildren: Vec<Node<'_>> = children
        .iter()
        .flat_map(|child| {
            let mut inner = child.walk();
            child.named_children(&mut inner).collect::<Vec<_>>()
        })
        .collect();
    children.into_iter().chain(grandchildren)
}

/// PHP variable nodes include the `$` sigil.
fn text_of<'a>(node: Option<Node<'_>>, source: &'a [u8]) -> Option<&'a str> {
    Some(node?.utf8_text(source).ok()?.trim_start_matches('$'))
}
