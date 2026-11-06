//! Kotlin's grammar labels no fields; children are positional.

use crate::languages::reads::{ReadBindings, enclosing_scope, nodes};
use crate::languages::typed::AgreedReturns;
use crate::languages::{Implementation, is_plain_type_name};
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
            let ty = if constructed { inner.named_child(0)? } else { inner };
            Some((type_name(ty, source)?, constructed))
        })
        .collect()
}

pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::default();
    let mut returns = AgreedReturns::default();
    for node in nodes(root) {
        match node.kind() {
            "class_declaration" => record_class(&mut bindings, node, source),
            "parameter" => record_parameter(&mut bindings, node, source),
            "property_declaration" => record_property(&mut bindings, node, source),
            "function_declaration" => record_return(&mut returns, node, source),
            _ => {}
        }
    }
    returns.insert_into(&mut bindings);
    bindings
}

fn record_class<'a>(bindings: &mut ReadBindings<'a>, class: Node<'_>, source: &'a [u8]) {
    let Some(name) = child_text(class, "type_identifier", source) else {
        return;
    };
    let scope = class.byte_range();
    bindings.insert("this", scope.clone(), Some(name.to_string()));
    if let Some((base, _)) = supertypes(class, source).into_iter().find(|(_, c)| *c) {
        bindings.insert("super", scope.clone(), Some(base.to_string()));
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
        if let Some(ty) = ty.as_ref().filter(|_| has_child(param, "binding_pattern_kind")) {
            bindings.insert_member(name, field, ty.clone());
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

fn record_property<'a>(bindings: &mut ReadBindings<'a>, property: Node<'_>, source: &'a [u8]) {
    let Some(variable) = named_child_of_kind(property, "variable_declaration") else {
        return;
    };
    let Some(name) = child_text(variable, "simple_identifier", source) else {
        return;
    };
    let ty = declared_type(variable, source).or_else(|| constructed_type(property, source));
    let class = property
        .parent()
        .filter(|parent| parent.kind() == "class_body")
        .and_then(|body| body.parent());
    if let Some(class) = class {
        if let (Some(owner), Some(ty)) = (child_text(class, "type_identifier", source), &ty) {
            bindings.insert_member(owner, name, ty.clone());
        }
        bindings.insert(name, class.byte_range(), ty);
        return;
    }
    let Some(block) = enclosing_scope(property, |kind| {
        matches!(kind, "statements" | "function_body" | "source_file")
    }) else {
        return;
    };
    bindings.insert(name, property.start_byte()..block.end_byte(), ty);
}

fn record_return<'a>(returns: &mut AgreedReturns<'a>, function: Node<'_>, source: &'a [u8]) {
    let Some(name) = child_text(function, "simple_identifier", source) else {
        return;
    };
    let mut cursor = function.walk();
    let ty = function
        .named_children(&mut cursor)
        .skip_while(|n| n.kind() != "function_value_parameters")
        .find(|n| matches!(n.kind(), "user_type" | "nullable_type"))
        .and_then(|ty| type_name(ty, source))
        .map(str::to_string);
    returns.note(name, ty);
}

fn constructed_type(property: Node<'_>, source: &[u8]) -> Option<String> {
    let call = named_child_of_kind(property, "call_expression")?;
    let callee = call.named_child(0).filter(|n| n.kind() == "simple_identifier")?;
    let name = callee.utf8_text(source).ok()?;
    (name.starts_with(char::is_uppercase) && is_plain_type_name(name)).then(|| name.to_string())
}

fn declared_type(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let ty = node
        .named_children(&mut cursor)
        .find(|n| matches!(n.kind(), "user_type" | "nullable_type"))?;
    type_name(ty, source).map(str::to_string)
}

fn type_name<'a>(ty: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let user_type = if ty.kind() == "nullable_type" {
        named_child_of_kind(ty, "user_type")?
    } else {
        ty
    };
    if user_type.kind() != "user_type" {
        return None;
    }
    let mut cursor = user_type.walk();
    let name = user_type
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "type_identifier")
        .last()?
        .utf8_text(source)
        .ok()?;
    is_plain_type_name(name).then_some(name)
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
