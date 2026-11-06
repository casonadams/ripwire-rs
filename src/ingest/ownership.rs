use super::extract::RawRef;
use crate::crawler::CrawledFile;
use crate::domain::{Interner, MODULE_SCOPE_NAME, NO_NODE, NodeId, Reference, SymKind, Symbol};

pub fn find_enclosing_symbol(file_symbols: &[Symbol], start_byte: usize) -> NodeId {
    let upper = file_symbols.partition_point(|s| (s.sig_start_byte as usize) <= start_byte);
    let mut best_id = NO_NODE;
    let mut best_span = u32::MAX;

    for s in file_symbols[..upper].iter().rev() {
        let sig_start = s.sig_start_byte as usize;
        let dist = start_byte - sig_start;
        if dist as u32 >= best_span {
            break;
        }
        if start_byte <= (s.end_byte as usize) {
            let span = s.end_byte - s.sig_start_byte;
            if span < best_span {
                best_span = span;
                best_id = s.id;
            }
        }
    }
    best_id
}

pub fn build_file_references(
    refs: Vec<RawRef>,
    file: &CrawledFile,
    file_id: u32,
    file_symbols: &[Symbol],
    interner: &Interner,
) -> Vec<Reference> {
    refs.into_iter()
        .map(|r| {
            let from_symbol = find_enclosing_symbol(file_symbols, r.start_byte);

            let callee_spur = interner.intern(&r.callee_name);
            let mut reference = Reference::new(
                from_symbol,
                callee_spur,
                file_id,
                r.line,
                r.lang.unwrap_or(file.lang),
                r.role,
            );
            if let Some(q) = &r.qualifier {
                reference.qualifier = Some(interner.intern(q));
            }
            if let Some(rt) = &r.receiver_type {
                reference.receiver_type = Some(interner.intern(rt));
            }
            reference.receiver_file = r.receiver_file;
            reference.callable_field = r.callable_field;
            reference.macro_arg = r.macro_arg;
            reference.member_call = r.member_call;
            reference.path_call = r.path_call;
            reference.start_byte = r.start_byte as u32;
            reference.name_line = r.name_line;
            reference.arg_count = r.arg_count;
            reference.arg_count_known = r.arg_count_known;
            reference
        })
        .collect()
}

/// Give references outside every definition a per-file `ModuleScope` owner so they become graph edges.
pub fn assign_module_scope_owner(
    file_symbols: &mut Vec<Symbol>,
    file_refs: &mut [Reference],
    file: &CrawledFile,
    file_id: u32,
    interner: &Interner,
    next_node_id: &mut NodeId,
) {
    let mut orphans = file_refs
        .iter_mut()
        .filter(|r| r.from_symbol == NO_NODE)
        .peekable();
    if orphans.peek().is_none() {
        return;
    }
    let id = *next_node_id;
    *next_node_id += 1;
    for r in orphans {
        r.from_symbol = id;
    }
    let name = interner.intern(MODULE_SCOPE_NAME);
    let canonical = interner.intern(&format!("{}::{MODULE_SCOPE_NAME}", file.rel_path));
    file_symbols.push(Symbol::new(
        id,
        name,
        canonical,
        SymKind::ModuleScope,
        file.lang,
        file_id,
        1,
    ));
}
