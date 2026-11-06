//! Declared-type facts Python exposes to call resolution, so a receiver such as `self.text`,
//! `progress` (a module-level `progress = Progress()`) or `Text.from_markup(...)` names its class:
//! annotated and constructed bindings, `self` inside its class, class fields (class-level annotations
//! and `self.f = ...` in a method), and same-file result types of functions and methods. A name whose
//! binding sites disagree, or any of whose sites declares no type, keeps no type.

use super::bindings::python_binding_range;
use super::types::{annotation, constructed_class, is_class_name, type_annotation};
use crate::languages::Implementation;
use crate::languages::reads::{ReadBindings, enclosing_scope, nodes};
use crate::languages::typed::{FieldFacts, record_fact, record_field_facts};
use std::collections::HashMap;
use std::ops::Range;
use tree_sitter::Node;

/// Read bindings with declared types.
pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::collect(root, source, |node| python_binding_range(root, node));
    record_return_types(&mut bindings, root, source);
    record_class_fields(&mut bindings, root, source);
    record_module_bindings(&mut bindings, root, source);
    record_local_bindings(&mut bindings, root, source);
    bindings
}

fn class_name<'s>(class: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    class.child_by_field_name("name")?.utf8_text(source).ok()
}

/// The class a method is defined in, when `func` is a direct member of a class body.
fn method_owner<'s>(func: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    let mut block = func.parent()?;
    if block.kind() == "decorated_definition" {
        block = block.parent()?;
    }
    let class = block.parent().filter(|c| c.kind() == "class_definition")?;
    class_name(class, source)
}

/// The innermost class enclosing `node`.
fn enclosing_class<'s>(node: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    enclosing_scope(node, |kind| kind == "class_definition").and_then(|class| class_name(class, source))
}

/// The innermost function whose body `node` lies in.
fn enclosing_function(node: Node<'_>) -> Option<Node<'_>> {
    enclosing_scope(node, |kind| kind == "function_definition")
}

/// Typed parameters of `func` by name; the first `self` or `cls` parameter is its method's class.
fn function_params(func: Node<'_>, source: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(params) = func.child_by_field_name("parameters") else {
        return out;
    };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        let (name, ty) = match param.kind() {
            "typed_parameter" => (param.named_child(0), annotation(param, source)),
            "typed_default_parameter" => {
                (param.child_by_field_name("name"), annotation(param, source))
            }
            _ => continue,
        };
        let (Some(name), Some(ty)) = (name, ty) else {
            continue;
        };
        if let Ok(text) = name.utf8_text(source) {
            out.insert(text.to_string(), ty);
        }
    }
    if let Some(owner) = self_param(func, source) {
        out.insert(owner.0.to_string(), owner.1.to_string());
    }
    out
}

/// The `self` or `cls` first parameter of a method, with its class.
fn self_param<'s>(func: Node<'_>, source: &'s [u8]) -> Option<(&'s str, &'s str)> {
    let owner = method_owner(func, source)?;
    let params = func.child_by_field_name("parameters")?;
    let first = params.named_child(0).filter(|p| p.kind() == "identifier")?;
    let name = first.utf8_text(source).ok()?;
    matches!(name, "self" | "cls").then_some((name, owner))
}

/// What an expression's value is declared to be.
struct Ctx<'c, 'a> {
    bindings: &'c ReadBindings<'a>,
    params: &'c HashMap<String, String>,
    owner: Option<&'c str>,
    source: &'c [u8],
}

fn expr_type(expr: Node<'_>, ctx: &Ctx<'_, '_>) -> Option<String> {
    match expr.kind() {
        "identifier" => ctx.params.get(expr.utf8_text(ctx.source).ok()?).cloned(),
        "call" => call_type(expr, ctx),
        _ => None,
    }
}

/// A constructed class, a same-file function's result, or a method result on a class or on `self`.
fn call_type(call: Node<'_>, ctx: &Ctx<'_, '_>) -> Option<String> {
    if let Some(class) = constructed_class(call, ctx.source) {
        return Some(class);
    }
    let function = call.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => ctx
            .bindings
            .return_type(function.utf8_text(ctx.source).ok()?)
            .map(str::to_string),
        "attribute" => {
            let object = function.child_by_field_name("object")?;
            if object.kind() != "identifier" {
                return None;
            }
            let receiver = object.utf8_text(ctx.source).ok()?;
            let owner = if receiver == "self" {
                ctx.owner?
            } else if is_class_name(receiver) {
                receiver
            } else {
                return None;
            };
            let method = function.child_by_field_name("attribute")?.utf8_text(ctx.source).ok()?;
            ctx.bindings
                .inherited_member_type(owner, &format!("{method}()"))
                .map(str::to_string)
        }
        _ => None,
    }
}

/// The expression context of `node`: its function's typed parameters and its class.
fn expression_context<'c, 'a>(
    node: Node<'_>,
    bindings: &'c ReadBindings<'a>,
    params: &'c HashMap<String, String>,
    source: &'c [u8],
) -> Ctx<'c, 'a> {
    Ctx {
        bindings,
        params,
        owner: enclosing_class(node, source),
        source,
    }
}

/// A module-level function records its annotated result under its name, and a method under
/// `m()` on its class. Names defined twice record nothing.
fn record_return_types<'a>(bindings: &mut ReadBindings<'a>, root: Node<'_>, source: &'a [u8]) {
    let mut functions: HashMap<&'a str, Vec<Node<'_>>> = HashMap::new();
    let mut methods: HashMap<(&'a str, &'a str), Vec<Node<'_>>> = HashMap::new();
    for func in nodes(root).filter(|n| n.kind() == "function_definition") {
        let Some(name) = func
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
        else {
            continue;
        };
        match method_owner(func, source) {
            Some(owner) => methods.entry((owner, name)).or_default().push(func),
            None if enclosing_class(func, source).is_none() => {
                functions.entry(name).or_default().push(func);
            }
            None => {}
        }
    }
    for (name, defs) in functions {
        if let [func] = defs.as_slice()
            && let Some(ty) = return_annotation(*func, source)
        {
            bindings.insert_return(name, ty);
        }
    }
    for ((owner, name), defs) in methods {
        if let [func] = defs.as_slice()
            && let Some(ty) = return_annotation(*func, source)
        {
            bindings.insert_member(owner, &format!("{name}()"), ty);
        }
    }
}

fn return_annotation(func: Node<'_>, source: &[u8]) -> Option<String> {
    type_annotation(func.child_by_field_name("return_type")?, source)
}

/// Class fields: an annotation (class-level `x: C`, or `self.x: C = ...` in a method) wins over the
/// type of the value assigned (`self.x = C()`, `self.x = typed_param`, `self.x = f()`).
fn record_class_fields(bindings: &mut ReadBindings<'_>, root: Node<'_>, source: &[u8]) {
    record_field_facts(
        bindings,
        root,
        source,
        &["class_definition"],
        |bindings, owner, body, explicit, derived| {
            record_class_level_fields(bindings, body, owner, source, explicit, derived);
            record_method_fields(bindings, body, owner, source, explicit, derived);
        },
    );
}

fn record_class_level_fields(
    bindings: &ReadBindings<'_>,
    body: Node<'_>,
    owner: &str,
    source: &[u8],
    explicit: &mut FieldFacts,
    derived: &mut FieldFacts,
) {
    let no_params = HashMap::new();
    let ctx = Ctx {
        bindings,
        params: &no_params,
        owner: Some(owner),
        source,
    };
    let mut cursor = body.walk();
    for stmt in body.named_children(&mut cursor) {
        let Some((field, assign)) = class_level_assignment(stmt, source) else {
            continue;
        };
        if let Some(ty) = annotation(assign, source) {
            record_fact(explicit, owner, field, ty);
        } else if let Some(ty) = assign
            .child_by_field_name("right")
            .and_then(|right| expr_type(right, &ctx))
        {
            record_fact(derived, owner, field, ty);
        }
    }
}

/// A class-level `name = value` statement: the assigned name and its assignment node.
fn class_level_assignment<'t, 's>(stmt: Node<'t>, source: &'s [u8]) -> Option<(&'s str, Node<'t>)> {
    let assign = stmt
        .named_child(0)
        .filter(|a| stmt.kind() == "expression_statement" && a.kind() == "assignment")?;
    let field = assign
        .child_by_field_name("left")
        .filter(|l| l.kind() == "identifier")
        .and_then(|l| l.utf8_text(source).ok())?;
    Some((field, assign))
}

fn record_method_fields(
    bindings: &ReadBindings<'_>,
    body: Node<'_>,
    owner: &str,
    source: &[u8],
    explicit: &mut FieldFacts,
    derived: &mut FieldFacts,
) {
    for method in nodes(body).filter(|n| n.kind() == "function_definition") {
        if method_owner(method, source) != Some(owner) {
            continue;
        }
        let params = function_params(method, source);
        let Some(method_body) = method.child_by_field_name("body") else {
            continue;
        };
        let ctx = Ctx {
            bindings,
            params: &params,
            owner: Some(owner),
            source,
        };
        for assign in nodes(method_body).filter(|n| n.kind() == "assignment") {
            record_self_field(assign, &ctx, explicit, derived);
        }
    }
}


fn record_self_field(
    assign: Node<'_>,
    ctx: &Ctx<'_, '_>,
    explicit: &mut FieldFacts,
    derived: &mut FieldFacts,
) {
    let (Some(left), Some(owner)) = (assign.child_by_field_name("left"), ctx.owner) else {
        return;
    };
    if left.kind() != "attribute"
        || left
            .child_by_field_name("object")
            .is_none_or(|o| o.utf8_text(ctx.source) != Ok("self"))
    {
        return;
    }
    let Some(field) = left
        .child_by_field_name("attribute")
        .and_then(|a| a.utf8_text(ctx.source).ok())
    else {
        return;
    };
    if let Some(ty) = annotation(assign, ctx.source) {
        record_fact(explicit, owner, field, ty);
    } else if let Some(ty) = assign
        .child_by_field_name("right")
        .and_then(|right| expr_type(right, ctx))
    {
        record_fact(derived, owner, field, ty);
    }
}

/// Module-level assignments type the module name for every function that reads it without binding
/// it. Assignments that disagree leave the name untyped.
fn record_module_bindings<'a>(bindings: &mut ReadBindings<'a>, root: Node<'_>, source: &'a [u8]) {
    let no_params = HashMap::new();
    for assign in nodes(root).filter(|n| n.kind() == "assignment" && is_module_level(*n)) {
        let Some(name) = assign
            .child_by_field_name("left")
            .filter(|l| l.kind() == "identifier")
            .and_then(|l| l.utf8_text(source).ok())
        else {
            continue;
        };
        let ctx = Ctx {
            bindings,
            params: &no_params,
            owner: None,
            source,
        };
        let ty = annotation(assign, source).or_else(|| {
            assign
                .child_by_field_name("right")
                .and_then(|right| expr_type(right, &ctx))
        });
        bindings.insert_module(name, ty);
    }
}

/// The declared type one binding site of a local name states, if any. `self` and `cls` are the
/// method's class; a `with C() as x` target is the constructed class.
fn site_type(node: Node<'_>, bindings: &ReadBindings<'_>, source: &[u8]) -> Option<String> {
    let parent = node.parent()?;
    match parent.kind() {
        "typed_parameter" if parent.named_child(0) == Some(node) => annotation(parent, source),
        "typed_default_parameter" if parent.child_by_field_name("name") == Some(node) => {
            annotation(parent, source)
        }
        "parameters" => {
            let func = parent.parent()?;
            let name = node.utf8_text(source).ok()?;
            self_param(func, source)
                .filter(|(self_name, _)| *self_name == name)
                .map(|(_, owner)| owner.to_string())
        }
        "assignment" if parent.child_by_field_name("left") == Some(node) => {
            annotation(parent, source).or_else(|| {
                let right = parent.child_by_field_name("right")?;
                let params = enclosing_function(parent)
                    .map(|func| function_params(func, source))
                    .unwrap_or_default();
                expr_type(right, &expression_context(parent, bindings, &params, source))
            })
        }
        "as_pattern_target" | "as_pattern" => {
            let as_pattern = if parent.kind() == "as_pattern" {
                parent
            } else {
                parent.parent()?
            };
            as_pattern.child_by_field_name("alias")?;
            constructed_class(as_pattern.named_child(0)?, source)
        }
        _ => None,
    }
}

/// Local names: every binding site of a name within one scope must declare the same type for the
/// name to carry it. A parameter without an annotation, or a reassignment of another type, is a site
/// that declares none.
fn record_local_bindings<'a>(bindings: &mut ReadBindings<'a>, root: Node<'_>, source: &'a [u8]) {
    let mut sites: HashMap<(&'a str, Range<usize>), Option<Option<String>>> = HashMap::new();
    for node in nodes(root).filter(|n| n.kind() == "identifier") {
        let (Some(range), Ok(name)) = (python_binding_range(root, node), node.utf8_text(source)) else {
            continue;
        };
        let ty = site_type(node, bindings, source);
        merge_site(sites.entry((name, range)).or_default(), ty);
    }
    for ((name, range), slot) in sites {
        if let Some(Some(ty)) = slot {
            bindings.insert(name, range, Some(ty));
        }
    }
}

/// Folds one binding site's declared type into a name's agreed type: the first site sets it, and a
/// site that disagrees (or declares none) clears it.
fn merge_site(slot: &mut Option<Option<String>>, ty: Option<String>) {
    match slot {
        None => *slot = Some(ty),
        Some(agreed) => {
            if *agreed != ty {
                *agreed = None;
            }
        }
    }
}

/// True when `node` lies outside every function and class body.
fn is_module_level(node: Node<'_>) -> bool {
    enclosing_scope(node, |kind| matches!(kind, "function_definition" | "class_definition")).is_none()
}

/// Every class with its own span and each base class its bases name (`class C(Base, mod.Other)`).
pub(super) fn class_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    crate::languages::typed::class_implementations(
        root,
        source,
        &["class_definition"],
        class_name,
        python_bases,
    )
}

/// The base classes a class names in its `superclasses` list.
fn python_bases<'s>(class: Node<'_>, source: &'s [u8]) -> Vec<&'s str> {
    let Some(bases) = class.child_by_field_name("superclasses") else {
        return Vec::new();
    };
    let mut cursor = bases.walk();
    bases
        .named_children(&mut cursor)
        .filter_map(|base| base_name(base, source))
        .collect()
}

/// The class name a base-class expression names: `Base`, or the last segment of `pkg.Base`.
fn base_name<'s>(base: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    let leaf = match base.kind() {
        "identifier" => base,
        "attribute" => base.child_by_field_name("attribute")?,
        _ => return None,
    };
    leaf.utf8_text(source).ok()
}
