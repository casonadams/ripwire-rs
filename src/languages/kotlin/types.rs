//! Kotlin's grammar labels no fields; children are positional.

use crate::languages::reads::{EXTENDS, ReadBindings, enclosing_scope, nodes};
use crate::languages::typed::{AgreedReturns, agree, qualify_nested_type};
use crate::languages::{Implementation, is_plain_type_name};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn class_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    crate::languages::typed::class_implementations(
        root,
        source,
        &["class_declaration"],
        kotlin_class_name,
        kotlin_bases,
    )
}

fn kotlin_class_name<'s>(class: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    child_text(class, "type_identifier", source)
}

fn kotlin_bases<'s>(class: Node<'_>, source: &'s [u8]) -> Vec<&'s str> {
    supertypes(class, source)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

fn supertypes<'a>(class: Node<'_>, source: &'a [u8]) -> Vec<(&'a str, bool)> {
    let mut cursor = class.walk();
    class
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "delegation_specifier")
        .filter_map(|spec| {
            let inner = spec.named_child(0)?;
            let constructed = inner.kind() == "constructor_invocation";
            let ty = if constructed {
                inner.named_child(0)?
            } else {
                inner
            };
            if is_qualified(ty) {
                return None;
            }
            Some((type_name(ty, source)?, constructed))
        })
        .collect()
}

/// A supertype written through its outer class (`Outer.Inner<T>`) names a nested type that its bare last segment does
/// not identify: `Inner` may be declared in several classes.
fn is_qualified(ty: Node<'_>) -> bool {
    let mut cursor = ty.walk();
    ty.children(&mut cursor).any(|child| child.kind() == ".")
}

/// Member types keyed by `(owner, member)`. A key noted with two different types, or with an unknown one, stays
/// unknown: an overload set that disagrees on its result types names no single type.
#[derive(Default)]
struct AgreedMembers(HashMap<(String, String), Option<String>>);

impl AgreedMembers {
    fn note(&mut self, owner: &str, member: &str, ty: Option<String>) {
        agree(&mut self.0, (owner.to_string(), member.to_string()), ty);
    }

    /// Inserts the agreed members of every owner the file declares exactly once: a class name declared twice
    /// (a nested `Builder` in two classes) could name either class, so its members are not recorded.
    fn insert_into(self, bindings: &mut ReadBindings<'_>, declared: &HashMap<String, usize>) {
        for ((owner, member), ty) in self.0 {
            if declared.get(owner.as_str()) != Some(&1) {
                continue;
            }
            if let Some(ty) = ty {
                bindings.insert_member(&owner, &member, ty);
            }
        }
    }
}

pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::default();
    let mut returns = AgreedReturns::default();
    let mut members = AgreedMembers::default();
    for node in nodes(root) {
        match node.kind() {
            "class_declaration" | "object_declaration" => {
                record_class(&mut bindings, &mut members, node, source);
            }
            "parameter" => record_parameter(&mut bindings, node, source),
            "property_declaration" => record_property(&mut bindings, &mut members, node, source),
            "function_declaration" => {
                let ty = return_type(node, source);
                if let Some(name) = child_text(node, "simple_identifier", source) {
                    returns.note(name, ty.clone());
                }
                record_method(&mut bindings, &mut members, node, source, ty);
            }
            "navigation_expression" => record_chain_root(&mut bindings, node, source),
            _ => {}
        }
    }
    returns.insert_into(&mut bindings);
    members.insert_into(&mut bindings, &declared_classes(root, source));
    bindings
}

/// How many class or object declarations of each owner key the file holds.
fn declared_classes(root: Node<'_>, source: &[u8]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for node in nodes(root).filter(|n| is_class(*n)) {
        if let Some(key) = owner_key(node, source) {
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
}

/// The key a class's members are recorded under: its name, qualified by the class that directly encloses it.
fn owner_key(class: Node<'_>, source: &[u8]) -> Option<String> {
    let name = child_text(class, "type_identifier", source)?;
    Some(
        match enclosing_scope(class, is_class_kind)
            .and_then(|outer| child_text(outer, "type_identifier", source))
        {
            Some(outer) => format!("{outer}.{name}"),
            None => name.to_string(),
        },
    )
}

fn is_class(node: Node<'_>) -> bool {
    is_class_kind(node.kind())
}

fn is_class_kind(kind: &str) -> bool {
    matches!(kind, "class_declaration" | "object_declaration")
}

/// A bare type name written inside classes names the nested type of that name the nearest enclosing class declares
/// (`Outer.Inner`), else the name itself.
fn qualify(site: Node<'_>, name: &str, source: &[u8]) -> String {
    qualify_nested_type(
        site,
        name,
        source,
        is_class,
        |n, source| child_text(n, "type_identifier", source),
        |n| named_child_of_kind(n, "class_body"),
    )
}

fn record_class<'a>(
    bindings: &mut ReadBindings<'a>,
    members: &mut AgreedMembers,
    class: Node<'_>,
    source: &'a [u8],
) {
    let (Some(name), Some(owner)) = (
        child_text(class, "type_identifier", source),
        owner_key(class, source),
    ) else {
        return;
    };
    let scope = class.byte_range();
    bindings.insert("this", scope.clone(), Some(owner.clone()));
    bindings.insert_module(name, Some(owner.clone()));
    let bases = supertypes(class, source);
    if let Some((base, _)) = bases.iter().find(|(_, constructed)| *constructed) {
        members.note(&owner, EXTENDS, Some(qualify(class, base, source)));
    }
    if let Some((base, _)) = bases.iter().find(|(_, c)| *c) {
        bindings.insert("super", scope.clone(), Some(qualify(class, base, source)));
    }
    let Some(constructor) = named_child_of_kind(class, "primary_constructor") else {
        return;
    };
    let mut cursor = constructor.walk();
    for param in constructor
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "class_parameter")
    {
        let Some(field) = child_text(param, "simple_identifier", source) else {
            continue;
        };
        let ty = declared_type(param, source);
        if let Some(ty) = ty
            .as_ref()
            .filter(|_| has_child(param, "binding_pattern_kind"))
        {
            members.note(&owner, field, Some(ty.clone()));
        }
        bindings.insert(field, scope.clone(), ty);
    }
}

fn record_parameter<'a>(bindings: &mut ReadBindings<'a>, param: Node<'_>, source: &'a [u8]) {
    let (Some(name), Some(function)) = (
        child_text(param, "simple_identifier", source),
        param.parent().and_then(|list| list.parent()),
    ) else {
        return;
    };
    bindings.insert(name, function.byte_range(), declared_type(param, source));
}

fn record_property<'a>(
    bindings: &mut ReadBindings<'a>,
    members: &mut AgreedMembers,
    property: Node<'_>,
    source: &'a [u8],
) {
    let Some(variable) = named_child_of_kind(property, "variable_declaration") else {
        return;
    };
    let Some(name) = child_text(variable, "simple_identifier", source) else {
        return;
    };
    let ty = declared_type(variable, source).or_else(|| constructed_type(property, source));
    let container = property
        .parent()
        .filter(|parent| parent.kind() == "class_body")
        .and_then(|body| body.parent());
    if let Some(container) = container {
        let owner = owning_class(container);
        if let Some(owner_name) = owner_key(owner, source) {
            members.note(&owner_name, name, ty.clone());
        }
        bindings.insert(name, owner.byte_range(), ty);
        return;
    }
    let Some(block) = enclosing_scope(property, |kind| {
        matches!(kind, "statements" | "function_body" | "source_file")
    }) else {
        return;
    };
    bindings.insert(name, property.start_byte()..block.end_byte(), ty);
}

/// The class a member of `container` belongs to: a companion object's members are reached through the class around
/// it (`Foo.make()`), so the companion yields its enclosing class.
fn owning_class(container: Node<'_>) -> Node<'_> {
    if container.kind() != "companion_object" {
        return container;
    }
    container
        .parent()
        .and_then(|body| body.parent())
        .filter(|class| class.kind() == "class_declaration")
        .unwrap_or(container)
}

/// A function's result type as a member of its owner: a class member `name()` (or the owning class of a companion
/// object), or an extension function's member on its receiver type.
fn record_method<'a>(
    bindings: &mut ReadBindings<'a>,
    members: &mut AgreedMembers,
    function: Node<'_>,
    source: &'a [u8],
    ty: Option<String>,
) {
    let Some(name) = child_text(function, "simple_identifier", source) else {
        return;
    };
    let member = format!("{name}()");
    if let Some(receiver) = extension_receiver(function, source) {
        members.note(&receiver, &member, ty);
        bindings.insert("this", function.byte_range(), Some(receiver));
        return;
    }
    let container = function
        .parent()
        .filter(|parent| parent.kind() == "class_body")
        .and_then(|body| body.parent());
    let Some(owner) = container.map(owning_class) else {
        return;
    };
    if let Some(owner_name) = owner_key(owner, source) {
        members.note(&owner_name, &member, ty);
    }
}

/// The receiver type of an extension function `fun Foo.bar()`: the type written before the function's name.
fn extension_receiver(function: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = function.walk();
    function
        .named_children(&mut cursor)
        .take_while(|n| !matches!(n.kind(), "simple_identifier" | "function_value_parameters"))
        .find_map(|n| match n.kind() {
            "receiver_type" => declared_type(n, source),
            "user_type" | "nullable_type" => type_text(n, source),
            _ => None,
        })
}

/// `Type.member(args).next()`: a capitalised name heading a call chain types as itself, so the members of the type it
/// names type the chain. A name the file never declares stays unbound here; the cross-file pass types it.
fn record_chain_root<'a>(bindings: &mut ReadBindings<'a>, navigation: Node<'_>, source: &'a [u8]) {
    let Some(call) = navigation
        .named_child(0)
        .filter(|n| n.kind() == "call_expression")
    else {
        return;
    };
    let Some(access) = call
        .named_child(0)
        .filter(|n| n.kind() == "navigation_expression")
    else {
        return;
    };
    let Some(root) = access
        .named_child(0)
        .filter(|n| n.kind() == "simple_identifier")
    else {
        return;
    };
    let Ok(name) = root.utf8_text(source) else {
        return;
    };
    if name.starts_with(char::is_uppercase) && is_plain_type_name(name) {
        bindings.insert_module(name, Some(name.to_string()));
    }
}

/// The text a receiver spells for typing: a dotted chain whose calls end in `()` (`a.b().c`), a cast (`(T)`), with
/// a non-null assertion (`x!!`) and parentheses looked through. `None` for a receiver that is a plain name.
pub(super) fn receiver_chain(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "call_expression"
        | "navigation_expression"
        | "parenthesized_expression"
        | "as_expression" => chain(node, source),
        _ if is_non_null(node, source) => chain(node.named_child(0)?, source),
        _ => None,
    }
}

fn chain(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "simple_identifier" | "this_expression" => Some(node.utf8_text(source).ok()?.to_string()),
        "parenthesized_expression" => chain(node.named_child(0)?, source),
        "as_expression" => Some(format!("({})", declared_type(node, source)?)),
        "call_expression" => {
            let callee = node.named_child(0)?;
            match callee.kind() {
                "simple_identifier" => Some(format!("{}()", callee.utf8_text(source).ok()?)),
                "navigation_expression" => Some(format!("{}()", navigation_chain(callee, source)?)),
                _ => None,
            }
        }
        "navigation_expression" => navigation_chain(node, source),
        _ if is_non_null(node, source) => chain(node.named_child(0)?, source),
        _ => None,
    }
}

fn navigation_chain(navigation: Node<'_>, source: &[u8]) -> Option<String> {
    let base = chain(navigation.named_child(0)?, source)?;
    let suffix = navigation
        .named_child(1)
        .filter(|n| n.kind() == "navigation_suffix")?;
    let member = child_text(suffix, "simple_identifier", source)?;
    Some(format!("{base}.{member}"))
}

fn is_non_null(node: Node<'_>, source: &[u8]) -> bool {
    node.named_child(0).is_some()
        && node
            .utf8_text(source)
            .is_ok_and(|text| text.ends_with("!!"))
}

/// The declared result type of a function: the user type after its parameter list.
fn return_type(function: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = function.walk();
    function
        .named_children(&mut cursor)
        .skip_while(|n| n.kind() != "function_value_parameters")
        .find(|n| matches!(n.kind(), "user_type" | "nullable_type"))
        .and_then(|ty| type_text(ty, source))
}

fn constructed_type(property: Node<'_>, source: &[u8]) -> Option<String> {
    let call = named_child_of_kind(property, "call_expression")?;
    let callee = call
        .named_child(0)
        .filter(|n| n.kind() == "simple_identifier")?;
    let name = callee.utf8_text(source).ok()?;
    (name.starts_with(char::is_uppercase) && is_plain_type_name(name)).then(|| name.to_string())
}

fn declared_type(node: Node<'_>, source: &[u8]) -> Option<String> {
    let ty = match named_child_of_kind(node, "user_type") {
        Some(ty) => ty,
        None => named_child_of_kind(node, "nullable_type")?,
    };
    type_text(ty, source)
}

/// The type a user type names: `Outer.Inner` when written through an uppercase outer type, else its last segment,
/// qualified by the classes around it.
fn type_text(ty: Node<'_>, source: &[u8]) -> Option<String> {
    let user_type = user_type_of(ty)?;
    let (outer, last) = last_two_segments(user_type, source)?;
    match outer {
        Some(outer) if outer.starts_with(char::is_uppercase) && is_plain_type_name(outer) => {
            Some(format!("{outer}.{last}"))
        }
        _ => Some(qualify(user_type, last, source)),
    }
}

/// The `user_type` a type node names, looking through one nullable wrapper.
fn user_type_of(ty: Node<'_>) -> Option<Node<'_>> {
    let user_type = if ty.kind() == "nullable_type" {
        named_child_of_kind(ty, "user_type")?
    } else {
        ty
    };
    (user_type.kind() == "user_type").then_some(user_type)
}

/// The last two type segments of a user type, `(outer, last)`: `outer` is `None` for an unqualified name.
fn last_two_segments<'s>(
    user_type: Node<'_>,
    source: &'s [u8],
) -> Option<(Option<&'s str>, &'s str)> {
    let mut cursor = user_type.walk();
    let mut last: Option<&str> = None;
    let mut outer: Option<&str> = None;
    for segment in user_type
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "type_identifier")
    {
        outer = last;
        last = Some(segment.utf8_text(source).ok()?);
    }
    let last = last.filter(|name| is_plain_type_name(name))?;
    Some((outer.filter(|name| is_plain_type_name(name)), last))
}

fn type_name<'a>(ty: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let (_, name) = last_two_segments(user_type_of(ty)?, source)?;
    Some(name)
}

fn named_child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|n| n.kind() == kind)
}

fn has_child(node: Node<'_>, kind: &str) -> bool {
    named_child_of_kind(node, kind).is_some()
}

fn child_text<'a>(node: Node<'_>, kind: &str, source: &'a [u8]) -> Option<&'a str> {
    named_child_of_kind(node, kind)?.utf8_text(source).ok()
}
