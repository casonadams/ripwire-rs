use super::capture::{extract_match_captures, record_extracted_item};
use super::crossfile::{declared_aliases, pending_facts};
use super::qualifier::{pending_chain_at, receiver_type_at};
use super::types::{FileParseResult, MemberType, RawDef, RawRef};
use crate::domain::{Lang, RefRole, SymKind};
use crate::ingest::registry::PARSER_STATE;
use crate::languages::reads::ReadBindings;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Query, QueryCursor};

struct FileCtx<'a, 't> {
    source_bytes: &'a [u8],
    lang: Lang,
    root: tree_sitter::Node<'t>,
}

pub fn parse_file_tree(
    source_bytes: &[u8],
    language: &Language,
    query: &Query,
    lang: Lang,
) -> FileParseResult {
    PARSER_STATE.with(|state| {
        let mut borrowed = state.borrow_mut();
        let (parser, cursor) = &mut *borrowed;
        if parser.set_language(language).is_err() {
            return FileParseResult::default();
        }

        let Some(tree) = parse_with_macro_blank_fallback(parser, source_bytes, lang) else {
            return FileParseResult::default();
        };
        let line_count = source_bytes.iter().filter(|&&b| b == b'\n').count() as u32 + 1;
        let ctx = FileCtx {
            source_bytes,
            lang,
            root: tree.root_node(),
        };
        let mut defs = Vec::new();
        let mut refs = Vec::new();
        collect_items(cursor, query, &ctx, &mut defs, &mut refs);
        let bindings = lang.semantics().read_bindings(ctx.root, source_bytes);
        let aliases = declared_aliases(lang, ctx.root, source_bytes);
        let returns = lang
            .semantics()
            .function_return_types(ctx.root, source_bytes);
        drop_shadowed_refs(&mut refs, &bindings);
        attach_receiver_types(&mut refs, &bindings, lang);
        let pending_fields = pending_facts(lang, &refs, &bindings);
        append_embedded_refs(&ctx, &mut refs);
        qualify_import_bound_receivers(&mut refs, lang);
        finalize_defs(&mut defs);
        FileParseResult {
            line_count,
            byte_size: source_bytes.len() as u64,
            defs,
            refs,
            foreign_exports: lang.semantics().foreign_exports(ctx.root, source_bytes),
            callable_bindings: lang.semantics().callable_bindings(ctx.root, source_bytes),
            implementations: lang.semantics().implementations(ctx.root, source_bytes),
            members: member_types(&bindings),
            pending_fields,
            aliases,
            returns,
        }
    })
}

fn member_types(bindings: &ReadBindings<'_>) -> Vec<MemberType> {
    bindings
        .member_entries()
        .map(|(owner, member, ty)| MemberType {
            owner: owner.to_string(),
            member: member.to_string(),
            ty: ty.to_string(),
        })
        .collect()
}

fn collect_items(
    cursor: &mut QueryCursor,
    query: &Query,
    ctx: &FileCtx<'_, '_>,
    defs: &mut Vec<RawDef>,
    refs: &mut Vec<RawRef>,
) {
    let mut matches = cursor.matches(query, ctx.root, ctx.source_bytes);
    while let Some(m) = matches.next() {
        let ext = extract_match_captures(m, query, ctx.source_bytes);
        record_extracted_item(ctx.root, ext, ctx.source_bytes, ctx.lang, defs, refs);
    }
}

fn drop_shadowed_refs(refs: &mut Vec<RawRef>, bindings: &ReadBindings<'_>) {
    refs.retain(|reference| {
        if !bindings.shadows(&reference.callee_name, reference.start_byte) {
            return true;
        }
        match reference.role {
            RefRole::Read | RefRole::Write => false,
            RefRole::Call => !reference.macro_arg,
            _ => true,
        }
    });
}

fn attach_receiver_types(refs: &mut [RawRef], bindings: &ReadBindings<'_>, lang: Lang) {
    for r in refs {
        let Some(receiver) = r.receiver.as_deref() else {
            continue;
        };
        if r.receiver_type.is_some() {
            continue;
        }
        match receiver_type_at(receiver, r.start_byte, bindings, lang) {
            Some(ty) => r.receiver_type = Some(ty),
            None => r.receiver_chain = pending_chain_at(receiver, r.start_byte, bindings),
        }
    }
}

fn append_embedded_refs(ctx: &FileCtx<'_, '_>, refs: &mut Vec<RawRef>) {
    if ctx.lang.semantics().supports_embedded_sql() {
        for guest in crate::languages::ALL_LANGS {
            guest
                .semantics()
                .embedded_refs(ctx.root, ctx.source_bytes, refs);
        }
    }
}

fn finalize_defs(defs: &mut Vec<RawDef>) {
    defs.sort_by(|a, b| {
        a.start_byte
            .cmp(&b.start_byte)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| {
                let a_prio = u8::from(a.kind != SymKind::Method);
                let b_prio = u8::from(b.kind != SymKind::Method);
                a_prio.cmp(&b_prio)
            })
    });
    defs.dedup_by(|a, b| a.start_byte == b.start_byte && a.name == b.name);
    dedup_conditional_macros(defs);
}

fn count_error_bytes(node: tree_sitter::Node<'_>) -> usize {
    if node.is_error() || node.kind() == "ERROR" {
        return node.end_byte().saturating_sub(node.start_byte()).max(1);
    }
    if !node.has_error() {
        return 0;
    }
    let mut total = 0usize;
    for i in 0..node.child_count() {
        if let Some(ch) = node.child(i) {
            total += count_error_bytes(ch);
        }
    }
    total
}

fn parse_with_macro_blank_fallback(
    parser: &mut tree_sitter::Parser,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<tree_sitter::Tree> {
    let tree = parser.parse(source_bytes, None)?;
    if !tree.root_node().has_error() {
        return Some(tree);
    }
    let Some(blanked) = lang.semantics().blank_error_macros(source_bytes) else {
        return Some(tree);
    };
    let Some(reparsed) = parser.parse(&blanked, None) else {
        return Some(tree);
    };
    if count_error_bytes(reparsed.root_node()) < count_error_bytes(tree.root_node()) {
        Some(reparsed)
    } else {
        Some(tree)
    }
}

fn dedup_conditional_macros(defs: &mut Vec<RawDef>) {
    let mut seen_macros = std::collections::HashSet::new();
    defs.retain(|d| d.kind != SymKind::Macro || seen_macros.insert(d.name.clone()));
}

/// Give `alias.f()` the qualifier `alias` when this file's own imports bind `alias`; where the language names
/// module members through their module, `alias.T.f()` takes the qualifier `alias::T`.
fn qualify_import_bound_receivers(refs: &mut [RawRef], lang: Lang) {
    let bound: Vec<String> = refs
        .iter()
        .filter(|r| r.role == RefRole::Import)
        .filter_map(|r| lang.semantics().import_binding(&r.callee_name))
        .map(|(_, name)| name.to_string())
        .collect();
    if bound.is_empty() {
        return;
    }
    let module_members = lang.semantics().module_member_qualifiers();
    for r in refs.iter_mut() {
        if r.receiver.as_ref().is_some_and(|recv| bound.contains(recv)) {
            r.qualifier = r.receiver.take();
        } else if module_members
            && let Some(qualifier) = module_member_qualifier(r.receiver.as_deref(), &bound)
        {
            r.qualifier = Some(qualifier);
            r.receiver = None;
        }
    }
}

/// `alias::T` for the receiver text `alias.T` when `alias` is an import this file binds.
fn module_member_qualifier(receiver: Option<&str>, bound: &[String]) -> Option<String> {
    let (module, ty) = receiver?.split_once('.')?;
    (bound.iter().any(|b| b == module) && !ty.contains('.')).then(|| format!("{module}::{ty}"))
}
