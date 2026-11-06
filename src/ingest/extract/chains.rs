//! Receiver chains typed across files. A chain's base is bound in its own file (`doc`), but a member step may be
//! declared in another (`doc.select()`, where `select` is on a superclass in another file), and a base may take
//! the result of a function another file declares (`r := chi.NewRouter()`). Once every file is parsed, each pending
//! chain is typed through the members of the classes the tree declares exactly once, and through the results of the
//! functions it declares exactly once: a name declared twice could name either, so it is not used. The type a chain
//! reaches is written in the file that declares its last member (or its function), so that file is recorded with it
//! and names the type through its own imports.

use super::project::Declared;
use super::types::{FileParseResult, PendingChain};
use crate::domain::{Lang, SymKind};
use crate::languages::reads::{EXTENDS, inherited_entry};
use crate::languages::{is_plain_type_name, is_type_path};
use std::collections::HashMap;

/// Each member of a uniquely declared class: `(owner, member)` to its declared type and the file that declares it.
type Members = HashMap<(String, String), (String, u32)>;

/// The declared result type of each uniquely declared function, and the file that declares that result.
type Returns = HashMap<String, (String, u32)>;

/// Type every pending receiver chain in `parsed` through the members of uniquely declared classes and the results of
/// uniquely declared functions.
pub(super) fn resolve_pending_chains(
    parsed: &mut [Option<(Lang, FileParseResult)>],
    declared: &Declared,
) {
    let members = unique_class_members(parsed, declared);
    let returns = unique_returns(parsed);
    for slot in parsed.iter_mut() {
        let Some((lang, file)) = slot else {
            continue;
        };
        let dotted = lang.semantics().nested_types_are_scoped();
        for r in &mut file.refs {
            if let Some(chain) = r.receiver_chain.take() {
                let typed = typed_chain(&members, &returns, &chain, dotted);
                r.receiver_type = typed.as_ref().map(|(ty, _)| ty.clone());
                r.receiver_file = typed.and_then(|(_, file)| file);
            }
        }
    }
}

/// The members of every class the files declare once, and the superclass of each such class when that
/// superclass is also declared once.
fn unique_class_members(
    parsed: &[Option<(Lang, FileParseResult)>],
    declared: &Declared,
) -> Members {
    let mut table = Members::new();
    for (file_id, slot) in parsed.iter().enumerate() {
        let Some((_, file)) = slot else {
            continue;
        };
        for member in &file.members {
            if !declared.classes.contains(member.owner.as_str())
                || (member.member == EXTENDS && !declared.classes.contains(member.ty.as_str()))
            {
                continue;
            }
            table.insert(
                (member.owner.clone(), member.member.clone()),
                (member.ty.clone(), file_id as u32),
            );
        }
    }
    table
}

/// The declared result type of each free function the tree declares exactly once, with the file declaring it.
fn unique_returns(parsed: &[Option<(Lang, FileParseResult)>]) -> Returns {
    let mut defined: HashMap<&str, usize> = HashMap::new();
    for def in parsed
        .iter()
        .flatten()
        .flat_map(|(_, file)| file.defs.iter())
        .filter(|def| def.kind == SymKind::Function)
    {
        *defined.entry(def.name.as_str()).or_default() += 1;
    }
    let mut returns = Returns::new();
    for (file_id, slot) in parsed.iter().enumerate() {
        let Some((_, file)) = slot else {
            continue;
        };
        for (name, ty) in &file.returns {
            if defined.get(name.as_str()) == Some(&1) {
                returns.insert(name.clone(), (ty.clone(), file_id as u32));
            }
        }
    }
    returns
}

/// The declared type a chain reaches and the file declaring its last step: its base is the base type or the result of
/// its function, and each step is a member of the type before it, climbing superclasses. A dotted type (`Outer.Inner`)
/// is a type only in a language that scopes nested types.
fn typed_chain(
    members: &Members,
    returns: &Returns,
    chain: &PendingChain,
    dotted: bool,
) -> Option<(String, Option<u32>)> {
    let (mut ty, mut declared_in) = match &chain.call {
        Some(callee) => {
            let (ty, file) = returns.get(callee)?.clone();
            (ty, Some(file))
        }
        None => (chain.base.clone(), None),
    };
    for step in &chain.steps {
        let (next, file) =
            inherited_entry(members, &ty, step, |(owner, _)| owner.as_str())?.clone();
        ty = next;
        declared_in = Some(file);
    }
    let admitted = ty.split("::").all(|part| {
        if dotted {
            is_type_path(part)
        } else {
            is_plain_type_name(part)
        }
    });
    admitted.then_some((ty, declared_in))
}
