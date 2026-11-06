//! Declared types resolved across files. A class declared in a header types its fields for the bodies that another
//! file defines (`backtracer-inl.h`), and a type alias declared in one header types the receivers that name it in
//! another. A template is typed by its bare name only when the tree declares no specialization of that name: a
//! specialization is a different class, and the name cannot say which one a receiver means.

use super::project::Declared;
use super::types::{FileParseResult, PendingField, RawRef};
use crate::domain::Lang;
use crate::languages::common::typed::WRAPPED_RECEIVER;
use crate::languages::reads::ReadBindings;
use std::collections::HashMap;

/// Receivers the file's own bindings cannot type but a class field of another file can: `member` or `this.member`
/// inside a body whose `this` is class `class`, with no binding of `member` in the file.
pub(super) fn pending_field_refs(
    refs: &[RawRef],
    bindings: &ReadBindings<'_>,
) -> Vec<PendingField> {
    refs.iter()
        .enumerate()
        .filter_map(|(ref_index, r)| {
            if r.receiver_type.is_some() {
                return None;
            }
            let receiver = r.receiver.as_deref()?;
            let member = match receiver.split_once('.') {
                Some(("this", member)) if !member.contains('.') => member,
                None => receiver,
                Some(_) => return None,
            };
            if bindings.shadows(member, r.start_byte) {
                return None;
            }
            let class = bindings.type_at("this", r.start_byte)?;
            Some(PendingField {
                ref_index,
                class: class.to_string(),
                member: member.to_string(),
            })
        })
        .collect()
}

/// The type aliases a file declares, as `(alias, declared target type)`; empty unless its language types fields
/// across files.
pub(super) fn declared_aliases(
    lang: Lang,
    root: tree_sitter::Node<'_>,
    source: &[u8],
) -> Vec<(String, String)> {
    if lang.semantics().cross_file_field_types() {
        lang.semantics().type_aliases(root, source)
    } else {
        Vec::new()
    }
}

/// The receivers a file leaves to the cross-file pass; empty unless its language types fields across files.
pub(super) fn pending_facts(
    lang: Lang,
    refs: &[RawRef],
    bindings: &ReadBindings<'_>,
) -> Vec<PendingField> {
    if lang.semantics().cross_file_field_types() {
        pending_field_refs(refs, bindings)
    } else {
        Vec::new()
    }
}

/// The project-wide tables the per-file pass reads.
struct Table<'a> {
    declared: &'a Declared,
    /// Alias name to the type it names; `None` when the target is a template with a specialization.
    aliases: HashMap<String, Option<String>>,
    /// Declared type of `(class, member)`, for classes declared once and members declared once.
    members: HashMap<(String, String), String>,
}

impl Table<'_> {
    /// The type a declared type text stands for: an alias's target, a template's bare name when the tree does not
    /// specialize it, or the specialization the text names exactly. `None` when the text names a specialized template
    /// that is not declared exactly.
    fn clean(&self, ty: &str) -> Option<String> {
        if ty.contains('<') {
            return self.declared.template_target(ty);
        }
        match self.aliases.get(ty) {
            Some(target) => target.clone(),
            None => Some(ty.to_string()),
        }
    }

    /// The receiver type a declared type gives a call named `callee`. A receiver typed through a transparent wrapper
    /// (`std::unique_ptr<T>`) takes its class `T`, except for the wrapper's own members (`reset` on the pointer).
    fn receiver_type(&self, ty: &str, callee: &str, wrapper_members: &[&str]) -> Option<String> {
        match ty.strip_suffix(WRAPPED_RECEIVER) {
            Some(_) if wrapper_members.contains(&callee) => None,
            Some(base) => self.clean(base),
            None => self.clean(ty),
        }
    }
}

/// Type every reference of the files that declare their fields across files, from the tree's declarations.
pub(super) fn resolve_cross_file_types(
    parsed: &mut [Option<(Lang, FileParseResult)>],
    declared: &Declared,
) {
    let table = Table::build(parsed, declared);
    for (lang, file) in parsed.iter_mut().flatten() {
        if !lang.semantics().cross_file_field_types() {
            continue;
        }
        let wrapper_members = lang.semantics().core_member_names();
        for r in &mut file.refs {
            if let Some(ty) = r.receiver_type.take() {
                r.receiver_type = table.receiver_type(&ty, &r.callee_name, wrapper_members);
            }
        }
        for pending in std::mem::take(&mut file.pending_fields) {
            let Some(r) = file.refs.get_mut(pending.ref_index) else {
                continue;
            };
            let declared = table
                .members
                .get(&(pending.class, pending.member))
                .and_then(|ty| table.receiver_type(ty, &r.callee_name, wrapper_members));
            if let Some(declared) = declared {
                r.receiver_type = Some(declared);
            }
        }
    }
}

impl<'a> Table<'a> {
    fn build(parsed: &[Option<(Lang, FileParseResult)>], declared: &'a Declared) -> Self {
        let files = || parsed.iter().flatten().map(|(_, file)| file);
        let mut alias_count: HashMap<&str, usize> = HashMap::new();
        for (alias, _) in files().flat_map(|file| file.aliases.iter()) {
            *alias_count.entry(alias.as_str()).or_default() += 1;
        }
        let mut aliases = HashMap::new();
        for (alias, target) in files().flat_map(|file| file.aliases.iter()) {
            if alias_count.get(alias.as_str()) != Some(&1) || declared.classes.contains(alias) {
                continue;
            }
            let resolved = declared.template_target(target);
            aliases.insert(alias.clone(), resolved);
        }
        let mut member_count: HashMap<(&str, &str), usize> = HashMap::new();
        let mut member_types: HashMap<(&str, &str), &str> = HashMap::new();
        for member in files().flat_map(|file| file.members.iter()) {
            if !declared.classes.contains(member.owner.as_str()) {
                continue;
            }
            let key = (member.owner.as_str(), member.member.as_str());
            *member_count.entry(key).or_default() += 1;
            member_types.insert(key, member.ty.as_str());
        }
        let members = member_types
            .into_iter()
            .filter(|(key, _)| member_count.get(key) == Some(&1))
            .map(|((owner, member), ty)| ((owner.to_string(), member.to_string()), ty.to_string()))
            .collect();
        Self {
            declared,
            aliases,
            members,
        }
    }
}
