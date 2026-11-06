use super::capture::MatchExtract;
use super::qualifier::split_top_level_qualified;
use super::scope::find_enclosing_scope;
use super::types::RawDef;
use crate::domain::{Lang, SymKind};
use crate::languages::SyntaxKinds;
use crate::languages::common::util::ancestors;

fn find_def_body_start(node: tree_sitter::Node<'_>, kinds: &SyntaxKinds) -> usize {
    if kinds.body_blocks.contains(&node.kind()) {
        return node.start_byte();
    }
    if let Some(b) = node.child_by_field_name("body") {
        return b.start_byte();
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|ch| {
            if kinds.body_blocks.contains(&ch.kind()) {
                return Some(ch.start_byte());
            }
            let nested = kinds
                .body_wrappers
                .contains(&ch.kind())
                .then(|| find_def_body_start(ch, kinds))?;
            (nested > 0).then_some(nested)
        })
        .unwrap_or(0)
}

pub(super) fn record_def_item(
    root: tree_sitter::Node<'_>,
    ext: &MatchExtract<'_>,
    source_bytes: &[u8],
    lang: Lang,
    defs: &mut Vec<RawDef>,
) {
    let (Some(raw_name), Some((kind, sub, start, end, line))) = (&ext.name, ext.def) else {
        return;
    };
    let kind = lang
        .semantics()
        .refine_def_kind(kind, ext.def_node, source_bytes);
    let (orig_name, inline_scope) = split_top_level_qualified(raw_name);
    let gate_ctx = crate::languages::DefCaptureGateContext {
        kind,
        sub,
        name: orig_name,
        name_node: ext.name_node,
        def_node: ext.def_node,
        source_bytes,
    };
    if lang.semantics().is_builtin_keyword(orig_name)
        || !lang.semantics().is_def_capture_allowed(&gate_ctx)
    {
        return;
    }
    let scope = inline_scope.or_else(|| {
        ext.name_node
            .and_then(|n| find_enclosing_scope(&ancestors(root, n), n, source_bytes, lang))
    });
    let (body_node, end) = lang.semantics().resolve_def_body_and_end(ext.def_node, end);
    let kinds = lang.semantics().syntax_kinds();
    let body_byte = body_node.map_or(0, |n| find_def_body_start(n, kinds));
    let start_row = line.saturating_sub(1);
    let end_row = body_node.map_or(start_row, |n| n.end_position().row as u32);
    let loc = end_row.saturating_sub(start_row) + 1;
    let metrics = if kind == SymKind::Function || kind == SymKind::Method {
        body_node
            .map(|n| crate::metrics::compute_complexity_from_node(n, source_bytes, lang))
            .unwrap_or_default()
    } else {
        crate::metrics::ComplexityMetrics::default()
    };
    let (arity, name) =
        lang.semantics()
            .resolve_def_params_and_name(ext.def_node, source_bytes, kind, orig_name);
    let binding = if kind == SymKind::Var {
        lang.semantics().var_binding(&gate_ctx)
    } else {
        crate::domain::Binding::Mutable
    };
    let contract_member = lang.semantics().is_contract_member(&gate_ctx);
    let (decl_start, doc_start) = ext.def_node.map_or((start, start), |n| {
        lang.semantics().resolve_def_span_start(n, source_bytes)
    });
    defs.push(RawDef {
        name,
        scope,
        kind,
        line,
        start_byte: decl_start,
        doc_len: decl_start.saturating_sub(doc_start),
        body_byte,
        end_byte: end,
        loc,
        params: arity.max,
        required_params: arity.required,
        metrics,
        binding,
        contract_member,
        takes_self: lang.semantics().def_takes_self(&gate_ctx),
        accessor: lang.semantics().def_is_accessor(&gate_ctx),
        registered: lang.semantics().def_is_registered(&gate_ctx),
        constructor: lang.semantics().def_is_constructor(&gate_ctx),
    });
}
