//! Receiver chains typed across files. A chain's base is bound in its own file (`doc`), but a member step may be
//! declared in another (`doc.select()`, where `select` is on a superclass in another file). Once every file is
//! parsed, each pending chain is typed through the members of the classes the tree declares exactly once: a name
//! declared twice could name either class, so its members are not used. The type a chain reaches is written in the
//! file that declares its last member, so that file is recorded with it and names the type through its own imports.

use super::project::Declared;
use super::types::{FileParseResult, PendingChain};
use crate::domain::Lang;
use crate::languages::is_plain_type_name;
use crate::languages::reads::{EXTENDS, inherited_entry};
use std::collections::HashMap;

/// Each member of a uniquely declared class: `(owner, member)` to its declared type and the file that declares it.
type Members = HashMap<(String, String), (String, u32)>;

/// Type every pending receiver chain in `parsed` through the members of uniquely declared classes.
pub(super) fn resolve_pending_chains(
    parsed: &mut [Option<(Lang, FileParseResult)>],
    declared: &Declared,
) {
    let members = unique_class_members(parsed, declared);
    for slot in parsed.iter_mut() {
        let Some((_, file)) = slot else {
            continue;
        };
        for r in &mut file.refs {
            if let Some(chain) = r.receiver_chain.take() {
                let typed = typed_chain(&members, &chain);
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

/// The declared type a chain reaches and the file declaring its last step: each step is a member of the type
/// before it, climbing superclasses.
fn typed_chain(members: &Members, chain: &PendingChain) -> Option<(String, Option<u32>)> {
    let mut ty = chain.base.clone();
    let mut declared_in = None;
    for step in &chain.steps {
        let (next, file) =
            inherited_entry(members, &ty, step, |(owner, _)| owner.as_str())?.clone();
        ty = next;
        declared_in = Some(file);
    }
    is_plain_type_name(&ty).then_some((ty, declared_in))
}
