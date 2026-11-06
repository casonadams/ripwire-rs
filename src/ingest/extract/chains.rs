//! Receiver chains typed across files. A chain's base is bound in its own file (`doc`), but a member step may be
//! declared in another (`doc.select()`, where `select` is on a superclass in another file). Once every file is
//! parsed, each pending chain is typed through the members of the classes the tree declares exactly once: a name
//! declared twice could name either class, so its members are not used.

use super::types::{FileParseResult, PendingChain};
use crate::domain::SymKind;
use crate::languages::is_plain_type_name;
use crate::languages::reads::{EXTENDS, ReadBindings};
use std::collections::HashMap;

/// Type every pending receiver chain in `files` through the members of uniquely declared classes.
pub fn resolve_pending_chains(files: &mut [&mut FileParseResult]) {
    let members = unique_class_members(files);
    for file in files.iter_mut() {
        for r in &mut file.refs {
            if let Some(chain) = r.receiver_chain.take() {
                r.receiver_type = typed_chain(&members, &chain);
            }
        }
    }
}

/// The members of every class the files declare once, and the superclass of each such class when that
/// superclass is also declared once.
fn unique_class_members(files: &[&mut FileParseResult]) -> ReadBindings<'static> {
    let mut declared: HashMap<&str, usize> = HashMap::new();
    for def in files.iter().flat_map(|file| file.defs.iter()) {
        if matches!(
            def.kind,
            SymKind::Class | SymKind::Struct | SymKind::Interface
        ) {
            *declared.entry(def.name.as_str()).or_default() += 1;
        }
    }
    let unique = |name: &str| declared.get(name) == Some(&1);
    let mut table = ReadBindings::default();
    for member in files.iter().flat_map(|file| file.members.iter()) {
        if !unique(&member.owner) || (member.member == EXTENDS && !unique(&member.ty)) {
            continue;
        }
        table.insert_member(&member.owner, &member.member, member.ty.clone());
    }
    table
}

/// The declared type a chain reaches: each step is a member of the type before it, climbing superclasses.
fn typed_chain(members: &ReadBindings<'_>, chain: &PendingChain) -> Option<String> {
    let mut ty = chain.base.clone();
    for step in &chain.steps {
        ty = members.inherited_member_type(&ty, step)?.to_string();
    }
    is_plain_type_name(&ty).then_some(ty)
}
