use super::decl::{has_child_token, has_const_qualifier, has_static_storage};
use crate::domain::SymKind;
use crate::languages::{DefCaptureGateContext, is_screaming_snake};

const TEST_BLOCK_MACRO_NAMES: &[&str] = &[
    "TEST_CASE",
    "TEST_CASE_FIXTURE",
    "TEST_CASE_METHOD",
    "SCENARIO",
    "TEST_SUITE",
];

fn is_bound_by_init_declarator(
    name_node: Option<tree_sitter::Node<'_>>,
    def_node: tree_sitter::Node<'_>,
) -> bool {
    let mut curr = name_node.and_then(|n| n.parent());
    while let Some(n) = curr {
        if n == def_node {
            break;
        }
        if n.kind() == "init_declarator" {
            return true;
        }
        curr = n.parent();
    }
    false
}

fn is_cpp_global_constant_allowed(
    dn: tree_sitter::Node<'_>,
    name_node: Option<tree_sitter::Node<'_>>,
    name: &str,
    src: &[u8],
) -> bool {
    let initialized = is_bound_by_init_declarator(name_node, dn);
    if initialized {
        return is_screaming_snake(name)
            || has_const_qualifier(dn, src)
            || has_child_token(dn, src, "type_qualifier", true, &["__constant__"]);
    }
    has_child_token(dn, src, "type_qualifier", true, &["__constant__"])
        || (is_screaming_snake(name)
            && has_child_token(
                dn,
                src,
                "type_qualifier",
                true,
                &["__device__", "__managed__"],
            ))
}

fn is_c_constant_allowed(ctx: &DefCaptureGateContext<'_>, is_cpp: bool) -> bool {
    let Some(dn) = ctx.def_node else {
        return false;
    };
    if !is_cpp {
        return is_screaming_snake(ctx.name) || has_const_qualifier(dn, ctx.source_bytes);
    }
    if dn.kind() == "field_declaration" {
        return has_static_storage(dn, ctx.source_bytes)
            && has_const_qualifier(dn, ctx.source_bytes);
    }
    is_cpp_global_constant_allowed(dn, ctx.name_node, ctx.name, ctx.source_bytes)
}

fn has_pre_body_error(node: tree_sitter::Node<'_>) -> bool {
    for i in 0..node.child_count() {
        let Some(ch) = node.child(i) else {
            continue;
        };
        if ch.kind() == "compound_statement" {
            break;
        }
        if ch.is_error() || ch.kind() == "ERROR" {
            return true;
        }
    }
    false
}

fn unwrap_declarator_container(fn_decl: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut decl = fn_decl.parent();
    while let Some(p) = decl {
        if matches!(
            p.kind(),
            "pointer_declarator"
                | "reference_declarator"
                | "parenthesized_declarator"
                | "attributed_declarator"
                | "function_declarator"
        ) {
            decl = p.parent();
        } else {
            return Some(p);
        }
    }
    None
}

fn is_invalid_c_callable_scope(fn_decl: tree_sitter::Node<'_>) -> bool {
    if fn_decl.has_error() {
        return true;
    }
    let Some(container) = unwrap_declarator_container(fn_decl) else {
        return true;
    };
    if !matches!(
        container.kind(),
        "function_definition" | "declaration" | "field_declaration" | "template_declaration"
    ) || has_pre_body_error(container)
    {
        return true;
    }
    let mut curr = container.parent();
    while let Some(p) = curr {
        match p.kind() {
            "compound_statement" | "ERROR" | "if_statement" | "for_statement"
            | "while_statement" => return true,
            "field_declaration_list" | "declaration_list" | "translation_unit" => return false,
            _ => curr = p.parent(),
        }
    }
    false
}

fn is_empty_preproc_function_macro(name_node: Option<tree_sitter::Node<'_>>) -> bool {
    let Some(define_node) = name_node.and_then(|n| n.parent()) else {
        return false;
    };
    define_node.kind() == "preproc_function_def"
        && define_node.child_by_field_name("value").is_none()
}

pub(crate) fn is_c_family_def_allowed(ctx: &DefCaptureGateContext<'_>, is_cpp: bool) -> bool {
    match ctx.sub {
        Some("testmacroblock") => TEST_BLOCK_MACRO_NAMES.contains(&ctx.name),
        Some("constant") => is_c_constant_allowed(ctx, is_cpp),
        Some("specialization") => false,
        _ => match ctx.kind {
            SymKind::Function | SymKind::Method => {
                !ctx.def_node.is_some_and(is_invalid_c_callable_scope)
            }
            SymKind::Field => ctx
                .def_node
                .is_none_or(|dn| !has_static_storage(dn, ctx.source_bytes)),
            SymKind::Macro => !is_empty_preproc_function_macro(ctx.name_node),
            _ => true,
        },
    }
}

/// A declaration ties to the definition with its kind and scope, and its arity when callable (the
/// edit-check identity rule): a prototype, a member declared in its class, an `@interface`.
pub(crate) fn c_family_declaration_ties(
    decl: &crate::domain::Symbol,
    def: &crate::domain::Symbol,
) -> bool {
    decl.kind == def.kind
        && decl.scope == def.scope
        && (!decl.kind.is_callable() || decl.params == def.params)
}

pub(crate) fn resolve_c_family_def_body_and_end<'a>(
    def_node: Option<tree_sitter::Node<'a>>,
    default_end: usize,
) -> (Option<tree_sitter::Node<'a>>, usize) {
    let Some(dn) = def_node else {
        return (None, default_end);
    };
    if dn.kind() == "expression_statement"
        && let Some(sib) = dn.next_named_sibling()
        && sib.kind() == "compound_statement"
    {
        return (Some(sib), sib.end_byte());
    }
    if let Some(parent) = crate::languages::find_enclosing_definition_node(dn) {
        return (Some(parent), parent.end_byte());
    }
    (Some(dn), default_end)
}
