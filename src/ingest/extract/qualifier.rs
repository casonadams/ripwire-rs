use super::scope::find_enclosing_scope;
use super::types::PendingChain;
use crate::domain::Lang;
use crate::languages::is_plain_type_name;

pub(super) fn member_access_receiver<'a>(
    parent: tree_sitter::Node<'a>,
    name_node: tree_sitter::Node<'a>,
    lang: Lang,
) -> Option<tree_sitter::Node<'a>> {
    let semantics = lang.semantics();
    if !semantics
        .syntax_kinds()
        .member_access
        .contains(&parent.kind())
    {
        return None;
    }
    let receiver = semantics.member_receiver(parent).or_else(|| {
        [
            "argument",
            "object",
            "value",
            "operand",
            "receiver",
            "table",
            "left",
            "expression",
        ]
        .into_iter()
        .find_map(|field| parent.child_by_field_name(field))
    })?;
    (receiver != name_node).then_some(receiver)
}

fn is_pascal_type_ident(text: &str) -> bool {
    let is_screaming_const =
        text.len() >= 2 && text.contains('_') && !text.bytes().any(|b| b.is_ascii_lowercase());
    text.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !is_screaming_const
}

fn extract_member_receiver_qualifier(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    let recv = member_access_receiver(parent, name_node, lang)?;
    let raw = recv.utf8_text(source_bytes).ok()?.trim();
    let text = &*lang.semantics().receiver_text(raw);
    if lang.semantics().self_receivers().contains(&text) {
        return find_enclosing_scope(name_node, source_bytes, lang);
    }
    let keep = is_pascal_type_ident(text) || lang.semantics().is_std_or_builtin_qualifier(text);
    keep.then(|| text.to_string())
}

pub(super) fn bare_member_receiver(
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    let recv = member_access_receiver(name_node.parent()?, name_node, lang)?;
    if let Some(name) = lang.semantics().call_receiver_name(recv, source_bytes) {
        return Some(format!("{name}()"));
    }
    if let Some(chain) = lang.semantics().receiver_chain(recv, source_bytes) {
        return Some(chain);
    }
    let raw = recv.utf8_text(source_bytes).ok()?.trim();
    let text = lang.semantics().receiver_text(raw);
    text.split('.')
        .all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .then(|| text.into_owned())
}

/// Declared type of a receiver: a bound identifier's type, or for a dotted chain the member type
/// the language reports at each step. A chain counts only when it ends at a plain type name.
pub(super) fn receiver_type_at(
    receiver: &str,
    byte: usize,
    bindings: &crate::languages::reads::ReadBindings<'_>,
    lang: Lang,
) -> Option<String> {
    if let Some(cast) = receiver.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        // A cast receiver `((T) x).m()` is typed by its cast.
        return is_plain_type_name(cast).then(|| cast.to_string());
    }
    if let Some((base, method)) = receiver.strip_suffix("()").and_then(|c| c.rsplit_once('.')) {
        // `self.method()`: the declared return of the method on the base's type.
        let base_ty = if base == "this" {
            bindings.type_at(base, byte)?.to_string()
        } else {
            receiver_type_at(base, byte, bindings, lang)?
        };
        return bindings
            .inherited_member_type(&base_ty, &format!("{method}()"))
            .filter(|ty| is_plain_type_name(ty))
            .map(str::to_string);
    }
    if let Some(callee) = receiver.strip_suffix("()") {
        let declared = bindings
            .return_type(callee)
            .filter(|ty| is_plain_type_name(ty));
        return match declared {
            Some(ty) => Some(ty.to_string()),
            None => lang
                .semantics()
                .undeclared_call_receiver_declines()
                .then(|| receiver.to_string()),
        };
    }
    let mut segments = receiver.split('.');
    let first = segments.next()?;
    let mut ty = bindings.type_at(first, byte)?.to_string();
    if !receiver.contains('.') {
        // A bare `this` receiver keeps name resolution; only a chain such as `this.client` is typed.
        return (first != "this").then_some(ty);
    }
    for segment in segments {
        ty = bindings
            .inherited_member_type(&ty, segment)
            .map(str::to_string)
            .or_else(|| lang.semantics().member_receiver_type(&ty, segment))?;
    }
    is_plain_type_name(&ty).then_some(ty)
}

/// The base type and member steps of a receiver chain whose base is bound in this file (`doc.select()` has
/// base `Document`, step `select()`). The cross-file pass types the steps.
pub(super) fn pending_chain_at(
    receiver: &str,
    byte: usize,
    bindings: &crate::languages::reads::ReadBindings<'_>,
) -> Option<PendingChain> {
    let mut parts = receiver.split('.');
    let first = parts.next()?;
    let base = bindings.type_at(first, byte)?.to_string();
    let steps: Vec<String> = parts.map(str::to_string).collect();
    let member_steps = steps
        .iter()
        .all(|step| is_plain_type_name(step.strip_suffix("()").unwrap_or(step)));
    (!steps.is_empty() && member_steps).then_some(PendingChain { base, steps })
}

pub(super) fn find_call_qualifier(
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    if let Some(qualifier) = lang.semantics().call_qualifier(name_node, source_bytes) {
        return Some(qualifier);
    }
    let parent = name_node.parent()?;
    if lang
        .semantics()
        .syntax_kinds()
        .scoped_paths
        .contains(&parent.kind())
        && let Some(path_node) = parent
            .child_by_field_name("path")
            .or_else(|| parent.child_by_field_name("scope"))
        && let Ok(text) = path_node.utf8_text(source_bytes)
    {
        if lang.semantics().self_receivers().contains(&text.trim())
            && let Some(scope) = find_enclosing_scope(name_node, source_bytes, lang)
        {
            return Some(scope);
        }
        let path = if lang.semantics().keeps_path_qualifier() {
            text
        } else {
            text.rsplit("::").next().unwrap_or(text)
        };
        let clean = path.split('<').next().unwrap_or(path).trim();
        if !clean.is_empty() {
            return Some(clean.to_string());
        }
    }
    extract_member_receiver_qualifier(parent, name_node, source_bytes, lang)
}

pub(super) fn split_top_level_qualified(raw: &str) -> (&str, Option<String>) {
    let bytes = raw.as_bytes();
    let mut angle_depth = 0u32;
    let mut last_sep = None;
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'<' => angle_depth += 1,
            b'>' => angle_depth = angle_depth.saturating_sub(1),
            b':' if bytes[i + 1] == b':' && angle_depth == 0 => {
                last_sep = Some(i);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let (qual, tail) = match last_sep {
        Some(pos) => {
            let prefix = &raw[..pos];
            let last_q = prefix.rsplit("::").next().unwrap_or(prefix);
            let clean_q = last_q.split('<').next().unwrap_or(last_q).trim();
            let q_opt = (!clean_q.is_empty()).then(|| clean_q.to_string());
            (q_opt, &raw[pos + 2..])
        }
        None => (None, raw),
    };
    let bare = if tail.starts_with("operator") || tail.starts_with('<') {
        tail.trim()
    } else {
        tail.split('<').next().unwrap_or(tail).trim()
    };
    (bare, qual)
}
