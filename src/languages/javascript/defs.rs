use super::names::JS_STD_MEMBERS;
use super::scalar::{is_scalar_expression, module_root};
use crate::domain::Binding;
use crate::languages::DefCaptureGateContext;

pub(super) fn is_cjs_export_target(name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    let Some(obj) = name_node
        .parent()
        .and_then(|m| m.child_by_field_name("object"))
    else {
        return false;
    };
    if obj.kind() == "identifier" {
        return obj.utf8_text(source_bytes) == Ok("exports");
    }
    if obj.kind() == "member_expression" {
        let is_mod = obj
            .child_by_field_name("object")
            .is_some_and(|o| o.kind() == "identifier" && o.utf8_text(source_bytes) == Ok("module"));
        let is_exp = obj
            .child_by_field_name("property")
            .is_some_and(|p| p.utf8_text(source_bytes) == Ok("exports"));
        return is_mod && is_exp;
    }
    false
}

pub(super) fn is_prototype_member_target(name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    name_node
        .parent()
        .and_then(|m| m.child_by_field_name("object"))
        .filter(|o| o.kind() == "member_expression")
        .and_then(|o| o.child_by_field_name("property"))
        .is_some_and(|p| p.utf8_text(source_bytes) == Ok("prototype"))
}

/// True when a named function expression is the value of a CJS export or prototype assignment, where the
/// assigned property is its def (`exports.h = function helperFn(){}` defines `h`). Its inner name is then
/// not a second definition. Other `obj.p = function inner(){}` assignments keep the inner name.
pub(super) fn is_member_assigned_function_name(ctx: &DefCaptureGateContext<'_>) -> bool {
    let Some(def) = ctx
        .def_node
        .filter(|def| def.kind() == "function_expression")
    else {
        return false;
    };
    def
        .parent()
        .filter(|assign| assign.kind() == "assignment_expression")
        .and_then(|assign| assign.child_by_field_name("left"))
        .filter(|left| left.kind() == "member_expression")
        .and_then(|left| left.child_by_field_name("property"))
        .is_some_and(|property| {
            is_cjs_export_target(property, ctx.source_bytes)
                || is_prototype_member_target(property, ctx.source_bytes)
        })
}

pub(crate) fn is_jsx_intrinsic_tag_call(
    name_node: Option<tree_sitter::Node<'_>>,
    name: &str,
) -> bool {
    let is_jsx_tag = name_node
        .and_then(|n| n.parent())
        .is_some_and(|p| matches!(p.kind(), "jsx_opening_element" | "jsx_self_closing_element"));
    is_jsx_tag && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}

/// A module-level `const` bound to an object literal, array literal, or `new` expression: mutable state
/// (`const cache = new Map()`) whatever its case. Other lowercase consts are functions or values, not indexed.
pub(crate) fn is_container_const(ctx: &DefCaptureGateContext<'_>) -> bool {
    ctx.name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
        .and_then(|declarator| declarator.child_by_field_name("value"))
        .is_some_and(|value| matches!(value.kind(), "object" | "array" | "new_expression"))
}

/// A TS/JS definition's `(declaration, doc)` span starts. An `export` wrapper supplies the declaration's start
/// (the defining node begins after the keyword). A `/** */` block directly above, with at most one line break
/// between, is its attached doc comment; plain `//` and `/* */` comments are never attached.
pub(crate) fn js_definition_span_start(
    def: tree_sitter::Node<'_>,
    source: &[u8],
) -> (usize, usize) {
    let outer = def
        .parent()
        .filter(|parent| parent.kind() == "export_statement")
        .unwrap_or(def);
    let decl = outer.start_byte();
    let doc = outer
        .prev_sibling()
        .filter(|prev| prev.kind() == "comment" && is_attached_jsdoc(*prev, decl, source))
        .map_or(decl, |prev| prev.start_byte());
    (decl, doc)
}

fn is_attached_jsdoc(comment: tree_sitter::Node<'_>, decl: usize, source: &[u8]) -> bool {
    let Some(gap) = source.get(comment.end_byte()..decl) else {
        return false;
    };
    let text = &source[comment.start_byte()..comment.end_byte()];
    text.starts_with(b"/**")
        && text.len() > 4
        && gap.iter().all(u8::is_ascii_whitespace)
        && gap.iter().filter(|&&b| b == b'\n').count() <= 1
}

/// Classifies a captured `Var` binding. A `const` whose value is provably scalar is `Frozen`; any other `const`
/// is `Const`: its binding cannot change, but an object or array it holds can, so it is state only when the
/// indexed files mutate it. `let` and `var` are `Mutable`.
pub(crate) fn js_var_binding(ctx: &DefCaptureGateContext<'_>) -> Binding {
    let Some(declarator) = ctx
        .name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
    else {
        return Binding::Mutable;
    };
    let is_const = declarator
        .parent()
        .filter(|decl| decl.kind() == "lexical_declaration")
        .and_then(|decl| decl.child(0))
        .is_some_and(|keyword| keyword.kind() == "const");
    if !is_const {
        return Binding::Mutable;
    }
    let frozen = declarator.child_by_field_name("value").is_some_and(|value| {
        is_scalar_expression(value, module_root(declarator), ctx.source_bytes, 0)
    });
    if frozen { Binding::Frozen } else { Binding::Const }
}

/// A const whose value calls a standard member (`fetch(u).then(f)`, `items.map(f)`) holds data, not a builder.
pub(crate) fn is_std_member_call_value(ctx: &DefCaptureGateContext<'_>) -> bool {
    let Some(value) = ctx
        .name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
        .and_then(|declarator| declarator.child_by_field_name("value"))
    else {
        return false;
    };
    value.kind() == "call_expression"
        && value
            .child_by_field_name("function")
            .filter(|callee| callee.kind() == "member_expression")
            .and_then(|callee| callee.child_by_field_name("property"))
            .and_then(|property| property.utf8_text(ctx.source_bytes).ok())
            .is_some_and(|name| JS_STD_MEMBERS.contains(&name))
}
