use super::lookup::ResolutionLookup;
use super::outcome::RefOutcome;
use crate::domain::{Interner, RefRole, Reference, Symbol};
use lasso::Spur;

pub(super) fn is_method_std_member_blocked<L: ResolutionLookup + ?Sized>(
    lookup: &L,
    r: &Reference,
    sym: &Symbol,
    caller_scope: Option<Spur>,
) -> bool {
    let Some(sym_sc) = sym.scope else {
        return true;
    };
    if r.member_call {
        // `x.len()`: the receiver, not the caller's scope or file, says whose `len` this is.
        return r.receiver_type != Some(sym_sc) && r.qualifier != Some(sym_sc);
    }
    if caller_scope == Some(sym_sc) {
        return false;
    }
    !(caller_scope.is_none()
        && sym.file_id != r.file_id
        && lookup.file_mentions_scope(r.file_id, sym_sc))
}

/// A member call named by a core member of the language's standard types whose receiver is not typed as the
/// candidate's class: `name_.swap(other)` on a `std::string` member of `logger` is not `logger::swap`, and a
/// `std::shared_ptr<logger>` member's `get()` is not `logger::get`. A member call never targets a free function.
/// Typed owners resolve before this filter runs.
fn is_core_member_off_receiver(r: &Reference, sym: &Symbol, callee: &str) -> bool {
    r.member_call
        && r.role == RefRole::Call
        && (sym.scope.is_none() || (r.receiver_type != sym.scope && r.qualifier != sym.scope))
        && r.lang.semantics().core_member_names().contains(&callee)
}

pub(super) fn is_unpinned_std_member_blocked<L: ResolutionLookup + ?Sized>(
    lookup: &L,
    r: &Reference,
    sym: &Symbol,
    caller_scope: Option<Spur>,
    callee_str: &str,
) -> bool {
    if is_core_member_off_receiver(r, sym, callee_str) {
        return true;
    }
    if !sym.lang.semantics().is_std_or_builtin_member(callee_str) {
        return false;
    }
    if let Some(q) = r.qualifier {
        return sym.scope != Some(q) && !is_module_path_free_fn(lookup, r, sym, q);
    }
    match sym.kind {
        crate::domain::SymKind::Field | crate::domain::SymKind::Var => true,
        crate::domain::SymKind::Method | crate::domain::SymKind::Function
            if lookup.is_type_member(sym) =>
        {
            is_method_std_member_blocked(lookup, r, sym, caller_scope)
        }
        crate::domain::SymKind::Function => {
            sym.file_id != r.file_id && !lookup.file_imports_name(r.file_id, sym.file_id, sym.name)
        }
        _ => false,
    }
}

/// `module::name(..)` names the free fn `name` of the indexed module file the qualifier is bound to. A path call
/// (not a receiver call) to that file's top-level function is pinned, whatever the std-member name.
fn is_module_path_free_fn<L: ResolutionLookup + ?Sized>(
    lookup: &L,
    r: &Reference,
    sym: &Symbol,
    q: Spur,
) -> bool {
    r.path_call
        && sym.kind == crate::domain::SymKind::Function
        && sym.scope.is_none()
        && lookup.qualifier_module_file(r.file_id, q) == Some(sym.file_id)
}

pub(super) fn is_non_table_read_write(r: &Reference) -> bool {
    matches!(r.role, RefRole::Read | RefRole::Write)
        && !r.lang.semantics().is_table_read_write_target()
}

fn classify_rejected_with_interner(r: &Reference, interner: &Interner) -> RefOutcome {
    if let Some(recv) = r.receiver_type
        && let Some(recv_str) = interner.try_resolve(&recv)
        && (recv_str.contains("::") || r.lang.semantics().is_std_or_builtin_qualifier(recv_str))
    {
        return RefOutcome::External;
    }
    if r.qualifier
        .and_then(|q| interner.try_resolve(&q))
        .is_some_and(|qs| r.lang.semantics().is_std_or_builtin_qualifier(qs))
    {
        return RefOutcome::External;
    }
    if r.qualifier.is_some() && r.lang.semantics().qualifier_must_be_indexed() {
        return RefOutcome::External;
    }
    if interner
        .try_resolve(&r.callee_name)
        .is_some_and(|cs| r.lang.semantics().is_std_or_builtin_member(cs))
    {
        return RefOutcome::Builtin;
    }
    RefOutcome::Unresolved
}

pub(super) fn classify_rejected_candidates(
    r: &Reference,
    interner: Option<&Interner>,
) -> RefOutcome {
    if r.role == RefRole::Access {
        return RefOutcome::Ignored;
    }
    if matches!(r.role, RefRole::Read | RefRole::Write) {
        return if r.lang.semantics().is_table_read_write_target() {
            RefOutcome::External
        } else {
            RefOutcome::Ignored
        };
    }
    interner.map_or(RefOutcome::Unresolved, |i| {
        classify_rejected_with_interner(r, i)
    })
}
