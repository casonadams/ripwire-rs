use super::arguments::count_call_arguments;
use super::capture::MatchExtract;
use super::params::is_enclosing_parameter;
use super::qualifier::{
    bare_member_receiver, find_call_qualifier, member_access_receiver, split_top_level_qualified,
};
use super::scope::find_enclosing_scope;
use super::types::RawRef;
use crate::domain::{Lang, RefRole};

fn ref_is_skipped(
    ext: &MatchExtract<'_>,
    name: &str,
    role: RefRole,
    lang: Lang,
    source_bytes: &[u8],
) -> bool {
    let is_pkg_prefix_extends = role == RefRole::Extends
        && ext
            .name_node
            .and_then(|n| n.next_sibling())
            .is_some_and(|s| s.kind() == ".");
    is_pkg_prefix_extends
        || (role == RefRole::Access && ext.name_node.is_some_and(is_call_or_write_member))
        || lang.semantics().is_builtin_keyword(name)
        || (role == RefRole::Call && lang.semantics().should_ignore_call(ext.name_node, name))
        || (role == RefRole::Read
            && ext
                .name_node
                .is_some_and(|node| !lang.semantics().is_read_capture_allowed(node, source_bytes)))
        || (role == RefRole::Write
            && ext.name_node.is_some_and(|node| {
                !lang
                    .semantics()
                    .is_write_capture_allowed(node, source_bytes)
            }))
}

fn ref_qualifiers(
    ext: &MatchExtract<'_>,
    inline_qual: Option<String>,
    role: RefRole,
    lang: Lang,
    source_bytes: &[u8],
) -> (Option<String>, Option<String>) {
    if role == RefRole::Call
        && let Some(name_node) = ext.name_node
        && let Some(base_type) = lang
            .semantics()
            .struct_update_base_type(name_node, source_bytes)
    {
        return (Some(base_type), None);
    }
    if role == RefRole::Extends && lang.semantics().has_trait_impls() {
        let recv_ty = ext
            .name_node
            .and_then(|n| find_enclosing_scope(n, source_bytes, lang));
        let qual = inline_qual.or_else(|| {
            ext.name_node
                .and_then(|n| find_call_qualifier(n, source_bytes, lang))
        });
        return (qual, recv_ty);
    }
    let qual = inline_qual.or_else(|| {
        ext.name_node.and_then(|n| {
            if role == RefRole::Extends {
                find_enclosing_scope(n, source_bytes, lang)
            } else {
                find_call_qualifier(n, source_bytes, lang)
            }
        })
    });
    (qual, None)
}

fn is_shadowed_by_parameter(
    ext: &MatchExtract<'_>,
    name: &str,
    role: RefRole,
    lang: Lang,
    source_bytes: &[u8],
) -> bool {
    let shadow_applies = role == RefRole::Read
        || (role == RefRole::Call && lang.semantics().parameters_shadow_calls());
    shadow_applies
        && ext
            .name_node
            .is_some_and(|n| !is_member_access(n) && is_enclosing_parameter(n, name, source_bytes))
}

pub(super) fn record_ref_item(
    ext: &MatchExtract<'_>,
    source_bytes: &[u8],
    lang: Lang,
    refs: &mut Vec<RawRef>,
) {
    let (Some(raw_name), Some((role, start, line))) = (&ext.name, ext.r#ref) else {
        return;
    };
    let (name, inline_qual) = if role == RefRole::Import {
        (raw_name.as_str(), None)
    } else {
        split_top_level_qualified(raw_name)
    };
    if ref_is_skipped(ext, name, role, lang, source_bytes) {
        return;
    }
    let (qualifier, receiver_type) = ref_qualifiers(ext, inline_qual, role, lang, source_bytes);
    if qualifier.is_none() && is_shadowed_by_parameter(ext, name, role, lang, source_bytes) {
        return;
    }
    let receiver = qualifier
        .is_none()
        .then(|| {
            ext.name_node
                .and_then(|n| bare_member_receiver(n, source_bytes, lang))
        })
        .flatten();
    let receiver_type = receiver_type.or_else(|| {
        let name_node = ext.name_node.filter(|_| qualifier.is_none())?;
        let recv = member_access_receiver(name_node.parent()?, name_node, lang)?;
        lang.semantics()
            .constructed_receiver(recv, source_bytes)
            .map(str::to_string)
    });
    let (arg_count, arg_count_known) = ext
        .name_node
        .map(count_call_arguments)
        .unwrap_or((0, false));
    let callee_name = if role == RefRole::Call {
        lang.semantics()
            .format_callee_name(name, arg_count, arg_count_known)
    } else {
        name.to_string()
    };
    refs.push(RawRef {
        callee_name,
        qualifier,
        line,
        name_line: ext
            .name_node
            .map_or(line, |n| n.start_position().row as u32 + 1),
        start_byte: start,
        role,
        arg_count,
        arg_count_known,
        lang: None,
        receiver,
        receiver_type,
        receiver_chain: None,
        callable_field: ext
            .name_node
            .is_some_and(|n| lang.semantics().is_callable_field_access(n)),
        macro_arg: role == RefRole::Call
            && ext
                .name_node
                .is_some_and(|n| lang.semantics().is_macro_argument(n)),
        member_call: ext.name_node.is_some_and(is_member_access),
        path_call: ext.name_node.is_some_and(is_path_access),
    });
}

/// Whether the member expression `name_node` names is called (a call reference covers it) or
/// assigned (a setter, not a read).
fn is_call_or_write_member(name_node: tree_sitter::Node<'_>) -> bool {
    let Some(member) = name_node.parent() else {
        return false;
    };
    member.parent().is_some_and(|outer| {
        let field = |f: &str| outer.child_by_field_name(f) == Some(member);
        (outer.kind().starts_with("call") && field("function"))
            || (outer.kind().contains("assignment") && field("left"))
    })
}

fn is_member_access(name_node: tree_sitter::Node<'_>) -> bool {
    name_node
        .prev_sibling()
        .is_some_and(|s| matches!(s.kind(), "." | "?." | "optional_chain" | "->" | "::"))
}

fn is_path_access(name_node: tree_sitter::Node<'_>) -> bool {
    name_node.prev_sibling().is_some_and(|s| s.kind() == "::")
}
