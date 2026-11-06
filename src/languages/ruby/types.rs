use crate::languages::reads::{ReadBindings, enclosing_scope, nodes};
use crate::languages::typed::implementation;
use crate::languages::{Implementation, is_plain_type_name};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn class_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    let mut out: Vec<Implementation> = nodes(root)
        .filter(|node| matches!(node.kind(), "class" | "module"))
        .filter_map(|class| {
            let subject = constant_leaf(class.child_by_field_name("name")?, source)?;
            let span = (class.start_byte(), class.end_byte());
            let base = class
                .child_by_field_name("superclass")
                .and_then(|clause| clause.named_child(0))
                .and_then(|base| constant_leaf(base, source));
            let mut out = vec![implementation(subject, None, span)];
            out.extend(base.map(|base| implementation(subject, Some(base), span)));
            Some(out)
        })
        .flatten()
        .collect();
    out.extend(mixins(root, source));
    out
}

/// `include M`, `prepend M` and `extend M` in a class or module body make M's methods reachable from it.
/// The span is the call itself, so the mixin claims none of the body's methods.
fn mixins(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    nodes(root)
        .filter(|call| call.kind() == "call" && call.child_by_field_name("receiver").is_none())
        .filter_map(|call| {
            let method = call.child_by_field_name("method")?.utf8_text(source).ok()?;
            if !matches!(method, "include" | "prepend" | "extend") {
                return None;
            }
            let owner = enclosing_scope(call, |kind| matches!(kind, "class" | "module"))?;
            let subject = constant_leaf(owner.child_by_field_name("name")?, source)?;
            let argument = call.child_by_field_name("arguments")?.named_child(0)?;
            let mixin = constant_leaf(argument, source)?;
            Some(implementation(
                subject,
                Some(mixin),
                (call.start_byte(), call.end_byte()),
            ))
        })
        .collect()
}

pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::default();
    let mut constants: HashMap<&'a str, Option<String>> = HashMap::new();
    for node in nodes(root) {
        match node.kind() {
            "class" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|name| constant_leaf(name, source))
                {
                    bindings.insert("self", node.byte_range(), Some(name.to_string()));
                }
            }
            "assignment" => record_assignment(&mut bindings, &mut constants, node, source),
            _ => {}
        }
    }
    // A constant is global to the file: it is typed when every assignment of it names one constructor.
    for (name, ty) in constants {
        if let Some(ty) = ty {
            bindings.insert(name, 0..source.len(), Some(ty));
        }
    }
    bindings
}

fn record_assignment<'a>(
    bindings: &mut ReadBindings<'a>,
    constants: &mut HashMap<&'a str, Option<String>>,
    assignment: Node<'_>,
    source: &'a [u8],
) {
    let Some(left) = assignment.child_by_field_name("left") else {
        return;
    };
    let ty = assignment
        .child_by_field_name("right")
        .and_then(|right| constructed_type(right, source));
    if left.kind() == "constant" {
        // Any assignment that is not a constructor leaves the constant untyped.
        if let Ok(name) = left.utf8_text(source) {
            constants
                .entry(name)
                .and_modify(|slot| {
                    if *slot != ty.map(str::to_string) {
                        *slot = None;
                    }
                })
                .or_insert_with(|| ty.map(str::to_string));
        }
        return;
    }
    let (Some(ty), Ok(name)) = (ty, left.utf8_text(source)) else {
        return;
    };
    match left.kind() {
        "identifier" => {
            let scope = enclosing_scope(assignment, |kind| {
                matches!(
                    kind,
                    "method" | "singleton_method" | "do_block" | "block" | "lambda" | "program"
                )
            });
            if let Some(scope) = scope {
                bindings.insert(name, assignment.start_byte()..scope.end_byte(), Some(ty.to_string()));
            }
        }
        "instance_variable" => {
            let owner = enclosing_scope(assignment, |kind| kind == "class")
                .and_then(|class| class.child_by_field_name("name"))
                .and_then(|name| constant_leaf(name, source));
            if let Some(owner) = owner {
                bindings.insert_member(owner, name.trim_start_matches('@'), ty.to_string());
            }
        }
        _ => {}
    }
}

pub(super) fn constructed_type<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call" {
        return None;
    }
    let method = node.child_by_field_name("method")?.utf8_text(source).ok()?;
    if method != "new" {
        return None;
    }
    constant_leaf(node.child_by_field_name("receiver")?, source)
}

fn constant_leaf<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let leaf = match node.kind() {
        "constant" => node,
        "scope_resolution" => node.child_by_field_name("name")?,
        _ => return None,
    };
    let name = leaf.utf8_text(source).ok()?;
    (leaf.kind() == "constant" && is_plain_type_name(name)).then_some(name)
}
