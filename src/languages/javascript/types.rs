//! Declared-type facts JavaScript exposes to call resolution, so `this.queue.push(...)`, `q._tasks.shift()`
//! and `heap.toArray()` name their class. `this` is the class inside its methods and the owner of a
//! prototype-assigned function; a `new C()` local, a `this.f = new C()` assignment or a class field types
//! its name as `C`; an object literal with constructed members is typed by those members; a same-file
//! function whose every return is `new C()` has result type `C`. A name whose sites disagree stays untyped.

use super::{javascript_binding_scope, javascript_binding_scope_in};
use crate::languages::reads::{ReadBindings, enclosing_scope, nodes};
use crate::languages::typed::{agree, construction_type};
use std::collections::HashMap;
use tree_sitter::Node;

/// Read bindings with declared types.
pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::collect_with_ancestors(root, source, |node, ancestors| {
        javascript_binding_scope_in(node, ancestors).map(|scope| scope.byte_range())
    });
    let mut fields = FieldFacts::default();
    for node in nodes(root) {
        match node.kind() {
            "class_declaration" | "class" => {
                if let Some(name) = declared_name(node, source) {
                    bindings.insert("this", node.byte_range(), Some(name.to_string()));
                }
            }
            // A plain function has its own `this`; a prototype-assigned one is its type's method.
            "function_declaration"
            | "function_expression"
            | "generator_function_declaration"
            | "generator_function" => {
                bindings.insert("this", node.byte_range(), prototype_owner(node, source));
            }
            "variable_declarator" => record_declarator(&mut bindings, node, source),
            "assignment_expression" => record_this_assignment(&mut fields, node, source),
            "field_definition" => record_class_field(&mut fields, node, source),
            _ => {}
        }
    }
    for ((owner, field), ty) in fields.0 {
        if let Some(ty) = ty {
            bindings.insert_member(&owner, &field, ty);
        }
    }
    record_return_types(&mut bindings, root, source);
    bindings
}

/// The text of a declaration's `name` field: a class or a function.
fn declared_name<'s>(node: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

/// The type `this` names inside `node`: the class of a method, or the owner of a prototype-assigned
/// function. Arrow functions keep their enclosing `this`, so they are skipped.
fn this_owner(node: Node<'_>, source: &[u8]) -> Option<String> {
    let function = enclosing_scope(node, binds_this)?;
    if function.kind() == "method_definition" {
        return declared_name(enclosing_class(function)?, source).map(str::to_string);
    }
    prototype_owner(function, source)
}

fn enclosing_class(node: Node<'_>) -> Option<Node<'_>> {
    enclosing_scope(node, |kind| matches!(kind, "class_declaration" | "class"))
}

/// `Foo.prototype.bar = function () {..}` binds `this` inside the function to `Foo`.
fn prototype_owner(function: Node<'_>, source: &[u8]) -> Option<String> {
    let assignment = function
        .parent()
        .filter(|parent| parent.kind() == "assignment_expression")?;
    if assignment.child_by_field_name("right")?.id() != function.id() {
        return None;
    }
    let member = assignment
        .child_by_field_name("left")
        .filter(|left| left.kind() == "member_expression")?;
    let prototype = member
        .child_by_field_name("object")
        .filter(|object| object.kind() == "member_expression")?;
    if prototype.child_by_field_name("property")?.utf8_text(source).ok()? != "prototype" {
        return None;
    }
    let owner = prototype
        .child_by_field_name("object")
        .filter(|object| object.kind() == "identifier")?;
    owner.utf8_text(source).ok().map(str::to_string)
}

/// The class `C` of `new C(..)`, when `C` is a plain name.
fn constructed_type(value: Node<'_>, source: &[u8]) -> Option<String> {
    (value.kind() == "new_expression")
        .then(|| construction_type(value, source))
        .flatten()
}

/// The constructed members of an object literal, `(key, class)` for each `key: new C()`.
fn object_members(value: Node<'_>, source: &[u8]) -> Vec<(String, String)> {
    if value.kind() != "object" {
        return Vec::new();
    }
    let mut cursor = value.walk();
    value
        .named_children(&mut cursor)
        .filter(|pair| pair.kind() == "pair")
        .filter_map(|pair| {
            let key = pair
                .child_by_field_name("key")
                .filter(|key| key.kind() == "property_identifier")?
                .utf8_text(source)
                .ok()?;
            let ty = constructed_type(pair.child_by_field_name("value")?, source)?;
            Some((key.to_string(), ty))
        })
        .collect()
}

/// `const q = new C()` types `q` as `C`. `const q = { k: new C() }` types `q` as the object type
/// `{ k: C }`, whose members are recorded under that type text, so `q.k` reads as `C`.
fn record_declarator<'a>(bindings: &mut ReadBindings<'a>, declarator: Node<'_>, source: &'a [u8]) {
    let (Some(name), Some(value)) = (
        declarator.child_by_field_name("name"),
        declarator.child_by_field_name("value"),
    ) else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    let (Ok(text), Some(scope)) = (name.utf8_text(source), javascript_binding_scope(name)) else {
        return;
    };
    if let Some(ty) = constructed_type(value, source) {
        bindings.insert(text, scope.byte_range(), Some(ty));
        return;
    }
    let members = object_members(value, source);
    if members.is_empty() {
        return;
    }
    let shape = format!(
        "{{ {} }}",
        members
            .iter()
            .map(|(key, ty)| format!("{key}: {ty}"))
            .collect::<Vec<_>>()
            .join("; ")
    );
    for (key, ty) in members {
        bindings.insert_member(&shape, &key, ty);
    }
    bindings.insert(text, scope.byte_range(), Some(shape));
}

/// Field types per `(class, field)`: `None` once a site assigns the field something other than one
/// constructor.
#[derive(Default)]
struct FieldFacts(HashMap<(String, String), Option<String>>);

impl FieldFacts {
    fn record(&mut self, owner: &str, field: &str, ty: Option<String>) {
        agree(&mut self.0, (owner.to_string(), field.to_string()), ty);
    }
}

/// `this.f = new C()` inside a method or prototype function types field `f` of its owner; any other value
/// assigned to `f` leaves it untyped.
fn record_this_assignment(fields: &mut FieldFacts, assignment: Node<'_>, source: &[u8]) {
    let (Some(left), Some(right)) = (
        assignment.child_by_field_name("left"),
        assignment.child_by_field_name("right"),
    ) else {
        return;
    };
    if left.kind() != "member_expression"
        || !left
            .child_by_field_name("object")
            .is_some_and(|object| object.kind() == "this")
    {
        return;
    }
    let (Some(field), Some(owner)) = (
        left.child_by_field_name("property")
            .and_then(|property| property.utf8_text(source).ok()),
        this_owner(assignment, source),
    ) else {
        return;
    };
    fields.record(&owner, field, constructed_type(right, source));
}

/// `f = new C()` in a class body types field `f` of that class.
fn record_class_field(fields: &mut FieldFacts, definition: Node<'_>, source: &[u8]) {
    let (Some(property), Some(owner)) = (
        definition
            .child_by_field_name("property")
            .and_then(|property| property.utf8_text(source).ok()),
        enclosing_class(definition).and_then(|class| declared_name(class, source)),
    ) else {
        return;
    };
    let ty = definition
        .child_by_field_name("value")
        .and_then(|value| constructed_type(value, source));
    fields.record(owner, property, ty);
}

/// A function whose every `return` is `new C()` has result type `C`, when its name is declared once.
fn record_return_types<'a>(bindings: &mut ReadBindings<'a>, root: Node<'_>, source: &'a [u8]) {
    let functions: Vec<Node<'_>> = nodes(root)
        .filter(|node| node.kind() == "function_declaration")
        .collect();
    let mut declared: HashMap<&str, usize> = HashMap::new();
    for function in &functions {
        if let Some(name) = declared_name(*function, source) {
            *declared.entry(name).or_default() += 1;
        }
    }
    for function in functions {
        let Some(name) = declared_name(function, source) else {
            continue;
        };
        if declared.get(name) != Some(&1) {
            continue;
        }
        let Some(body) = function.child_by_field_name("body") else {
            continue;
        };
        let mut result: Option<Option<String>> = None;
        for ret in nodes(body).filter(|node| {
            node.kind() == "return_statement"
                && enclosing_scope(*node, is_function).is_some_and(|f| f.id() == function.id())
        }) {
            let ty = ret
                .named_child(0)
                .and_then(|value| constructed_type(value, source));
            match &result {
                None => result = Some(ty),
                Some(seen) if *seen != ty => result = Some(None),
                Some(_) => {}
            }
        }
        if let Some(Some(ty)) = result {
            bindings.insert_return(name, ty);
        }
    }
}

fn is_function(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
    )
}

/// Functions with their own `this`: arrow functions inherit theirs and are not among them.
fn binds_this(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "method_definition"
    )
}
