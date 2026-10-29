use super::registry::PARSER_STATE;
use crate::domain::{Lang, RefRole, SymKind};
use crate::languages::{SyntaxKinds, is_plain_type_name};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Query};

/// Raw definition extracted before deduplication and global ID assignment.
#[derive(Debug, Clone)]
pub struct RawDef {
    pub name: String,
    pub scope: Option<String>,
    pub kind: SymKind,
    pub line: u32,
    pub start_byte: usize,
    /// Bytes from the attached doc comment (if any) up to `start_byte`; 0 when none.
    pub doc_len: usize,
    pub body_byte: usize,
    pub end_byte: usize,
    pub loc: u32,
    pub params: u16,
    pub metrics: crate::metrics::ComplexityMetrics,
    /// Set by the language plugin for `Var` bindings (see `var_binding`); other kinds are `Mutable`.
    pub binding: crate::domain::Binding,
    /// Set by the language plugin for bodyless members reached only by typed dispatch (see `is_contract_member`).
    pub contract_member: bool,
    pub takes_self: bool,
    pub accessor: bool,
}

impl RawDef {
    #[must_use]
    pub fn span(name: String, kind: SymKind, line: u32, bytes: std::ops::Range<usize>) -> Self {
        Self {
            name,
            scope: None,
            kind,
            line,
            start_byte: bytes.start,
            doc_len: 0,
            body_byte: bytes.start,
            end_byte: bytes.end,
            loc: 1,
            params: 0,
            metrics: crate::metrics::ComplexityMetrics::default(),
            binding: crate::domain::Binding::Mutable,
            contract_member: false,
            takes_self: false,
            accessor: false,
        }
    }
}

/// Raw reference extracted before enclosing symbol attribution.
#[derive(Debug, Clone)]
pub struct RawRef {
    pub callee_name: String,
    pub qualifier: Option<String>,
    pub line: u32,
    pub name_line: u32,
    pub start_byte: usize,
    pub role: RefRole,
    pub arg_count: u16,
    pub arg_count_known: bool,
    /// Language the reference is written in when it differs from its file's (embedded SQL).
    pub lang: Option<crate::domain::Lang>,
    /// Bare identifier receiver (`x` in `x.f()`) not kept as the qualifier.
    pub receiver: Option<String>,
    pub receiver_type: Option<String>,
    pub callable_field: bool,
    pub macro_arg: bool,
    /// The reference is written as a member access (`x.f()`), not a bare or path name.
    pub member_call: bool,
}

/// Parsed facts from a single file.
#[derive(Debug, Default)]
pub struct FileParseResult {
    pub line_count: u32,
    pub byte_size: u64,
    pub defs: Vec<RawDef>,
    pub refs: Vec<RawRef>,
    pub foreign_exports: Vec<crate::languages::ForeignExport>,
    pub callable_bindings: Vec<crate::languages::CallableBinding>,
    pub implementations: Vec<crate::languages::Implementation>,
}

pub enum Capture {
    Name,
    ImportPath,
    Def(SymKind, Option<&'static str>),
    Ref(RefRole),
}

pub fn classify_capture(name: &str) -> Option<Capture> {
    if name == "name" {
        return Some(Capture::Name);
    }
    if name == "import.path" {
        return Some(Capture::ImportPath);
    }
    if let Some(tail) = name.strip_prefix("definition.") {
        let (kind, sub) = match tail {
            "function" => (SymKind::Function, None),
            "cjsexport" => (SymKind::Function, Some("cjsexport")),
            "method" => (SymKind::Method, None),
            "protomethod" => (SymKind::Method, Some("protomethod")),
            "class" | "object" => (SymKind::Class, None),
            "specialization" => (SymKind::Class, Some("specialization")),
            "struct" => (SymKind::Struct, None),
            "interface" => (SymKind::Interface, None),
            "constant" => (SymKind::Var, Some("constant")),
            "enummember" => (SymKind::Var, Some("enummember")),
            "attribute" => (SymKind::Var, Some("attribute")),
            "type" | "var" | "property" => (SymKind::Var, None),
            "macro" => (SymKind::Macro, None),
            "testmacroblock" => (SymKind::Macro, Some("testmacroblock")),
            "yamlkey" => (SymKind::Section, Some("yamlkey")),
            "module" | "section" => (SymKind::Section, None),
            "field" => (SymKind::Field, None),
            _ => return None,
        };
        return Some(Capture::Def(kind, sub));
    }
    name.strip_prefix("reference.").and_then(|tail| {
        Some(Capture::Ref(match tail {
            "read" | "attribute" => RefRole::Read,
            "write" => RefRole::Write,
            "import" => RefRole::Import,
            "extends" | "implementation" | "class" => RefRole::Extends,
            "macro" => RefRole::Macro,
            "type" => RefRole::Type,
            "value" => RefRole::Value,
            "through" => RefRole::Through,
            "access" => RefRole::Access,
            "call" | "bare" | "operator" => RefRole::Call,
            _ => return None,
        }))
    })
}

pub fn distinct_scope_tail(
    cand: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    sep: &str,
) -> Option<String> {
    if cand == name_node {
        return None;
    }
    let text = cand.utf8_text(source_bytes).ok()?.trim();
    let tail = if sep.is_empty() {
        text
    } else {
        text.rsplit(sep).next().unwrap_or(text)
    };
    let clean = tail.split('<').next().unwrap_or(tail).trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

pub fn find_positional_type_ident(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    let first = (0..parent.named_child_count())
        .filter_map(|i| parent.named_child(i))
        .find(|c| matches!(c.kind(), "type_identifier" | "identifier"))?;
    distinct_scope_tail(first, name_node, source_bytes, "")
}

fn extract_named_container_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    if let Some(name_child) = parent.child_by_field_name("name") {
        return distinct_scope_tail(name_child, name_node, source_bytes, "::");
    }
    find_positional_type_ident(parent, name_node, source_bytes)
}

fn extract_qualified_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    let scope_node = parent.child_by_field_name("scope")?;
    distinct_scope_tail(scope_node, name_node, source_bytes, "::")
}

fn extract_node_scope(
    parent: tree_sitter::Node<'_>,
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    if let Some(custom) = lang
        .semantics()
        .extract_custom_scope(parent, name_node, source_bytes)
    {
        return Some(custom);
    }
    let kinds = lang.semantics().syntax_kinds();
    let kind = parent.kind();
    if kinds.scoped_paths.contains(&kind) {
        extract_qualified_scope(parent, name_node, source_bytes)
    } else if kinds.scope_containers.contains(&kind) {
        extract_named_container_scope(parent, name_node, source_bytes)
    } else {
        None
    }
}

fn find_enclosing_scope(
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    let mut curr = name_node.parent();
    while let Some(parent) = curr {
        if let Some(scope) = extract_node_scope(parent, name_node, source_bytes, lang) {
            return Some(scope);
        }
        curr = parent.parent();
    }
    None
}

fn member_access_receiver<'a>(
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

fn bare_member_receiver(
    name_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> Option<String> {
    let recv = member_access_receiver(name_node.parent()?, name_node, lang)?;
    if let Some(name) = lang.semantics().call_receiver_name(recv, source_bytes) {
        return Some(format!("{name}()"));
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
fn receiver_type_at(
    receiver: &str,
    byte: usize,
    bindings: &crate::languages::reads::ReadBindings<'_>,
    lang: Lang,
) -> Option<String> {
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
            .member_type(&ty, segment)
            .map(str::to_string)
            .or_else(|| lang.semantics().member_receiver_type(&ty, segment))?;
    }
    is_plain_type_name(&ty).then_some(ty)
}

fn find_call_qualifier(
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

fn split_top_level_qualified(raw: &str) -> (&str, Option<String>) {
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

/// Ruby passes trailing `key => value` pairs as one hash argument.
fn count_arguments_in_list(child: tree_sitter::Node<'_>) -> u16 {
    let mut count = 0u16;
    let mut in_pairs = false;
    let mut cursor = child.walk();
    for arg in child
        .named_children(&mut cursor)
        .filter(|arg| !arg.kind().contains("comment"))
    {
        let is_pair = arg.kind() == "pair";
        if !(is_pair && in_pairs) {
            count = count.saturating_add(1);
        }
        in_pairs = is_pair;
    }
    count
}

fn find_arguments_child(parent: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    for i in 0..parent.child_count() {
        if let Some(child) = parent.child(i) {
            let ckind = child.kind();
            if ckind.contains("argument") || ckind == "args" {
                return Some(child);
            }
        }
    }
    None
}

fn count_call_arguments(name_node: tree_sitter::Node<'_>) -> (u16, bool) {
    let mut curr = name_node.parent();
    while let Some(parent) = curr {
        let kind = parent.kind();
        if kind.contains("call") || kind == "invocation" || kind == "method_invocation" {
            if let Some(args_node) = find_arguments_child(parent) {
                return (count_arguments_in_list(args_node), true);
            }
            break;
        }
        curr = parent.parent();
    }
    (0, false)
}

struct MatchExtract<'a> {
    name: Option<String>,
    name_node: Option<tree_sitter::Node<'a>>,
    def: Option<(SymKind, Option<&'static str>, usize, usize, u32)>,
    def_node: Option<tree_sitter::Node<'a>>,
    r#ref: Option<(RefRole, usize, u32)>,
}

fn apply_capture<'a>(
    ext: &mut MatchExtract<'a>,
    cap_name: &str,
    node: tree_sitter::Node<'a>,
    source_bytes: &[u8],
) {
    match classify_capture(cap_name) {
        Some(Capture::Name) => {
            if let Ok(text) = node.utf8_text(source_bytes) {
                ext.name = Some(text.to_string());
                ext.name_node = Some(node);
            }
        }
        Some(Capture::ImportPath) => {
            let line = node.start_position().row as u32 + 1;
            ext.r#ref = Some((RefRole::Import, node.start_byte(), line));
            if let Ok(text) = node.utf8_text(source_bytes) {
                ext.name = Some(text.to_string());
                ext.name_node = Some(node);
            }
        }
        Some(Capture::Def(k, sub)) => {
            let line = node.start_position().row as u32 + 1;
            ext.def = Some((k, sub, node.start_byte(), node.end_byte(), line));
            ext.def_node = Some(node);
        }
        Some(Capture::Ref(r)) => {
            let line = node.start_position().row as u32 + 1;
            ext.r#ref = Some((r, node.start_byte(), line));
        }
        None => {}
    }
}

fn extract_match_captures<'a>(
    m: &tree_sitter::QueryMatch<'a, '_>,
    query: &Query,
    source_bytes: &[u8],
) -> MatchExtract<'a> {
    let mut ext = MatchExtract {
        name: None,
        name_node: None,
        def: None,
        def_node: None,
        r#ref: None,
    };
    for capture in m.captures {
        let cap_name = query.capture_names()[capture.index as usize];
        apply_capture(&mut ext, cap_name, capture.node, source_bytes);
    }
    ext
}

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

fn record_def_item(
    ext: &MatchExtract<'_>,
    source_bytes: &[u8],
    lang: Lang,
    defs: &mut Vec<RawDef>,
) {
    let (Some(raw_name), Some((kind, sub, start, end, line))) = (&ext.name, ext.def) else {
        return;
    };
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
            .and_then(|n| find_enclosing_scope(n, source_bytes, lang))
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
    let (params, name) =
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
        params,
        metrics,
        binding,
        contract_member,
        takes_self: lang.semantics().def_takes_self(&gate_ctx),
        accessor: lang.semantics().def_is_accessor(&gate_ctx),
    });
}

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

fn record_ref_item(
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
        callable_field: ext
            .name_node
            .is_some_and(|n| lang.semantics().is_callable_field_access(n)),
        macro_arg: role == RefRole::Call
            && ext
                .name_node
                .is_some_and(|n| lang.semantics().is_macro_argument(n)),
        member_call: ext.name_node.is_some_and(is_member_access),
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

const NON_BINDING_PARAM_FIELDS: &[&str] = &[
    "value",
    "right",
    "type",
    "default_type",
    "bounds",
    "constraint",
    "decorator",
    "type_arguments",
];

/// True when a function or closure enclosing `node` declares a parameter named `name`.
fn is_enclosing_parameter(node: tree_sitter::Node<'_>, name: &str, source_bytes: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if let Some(params) = n.child_by_field_name("parameters")
            && node.start_byte() >= params.end_byte()
            && binds_name(params, name, source_bytes)
        {
            return true;
        }
        cur = n.parent();
    }
    false
}

fn binds_name(node: tree_sitter::Node<'_>, name: &str, source_bytes: &[u8]) -> bool {
    if node.child_count() == 0 {
        return node.utf8_text(source_bytes).is_ok_and(|t| t == name);
    }
    (0..node.child_count()).any(|i| {
        node.field_name_for_child(i as u32)
            .is_none_or(|f| !NON_BINDING_PARAM_FIELDS.contains(&f))
            && node
                .child(i)
                .is_some_and(|c| binds_name(c, name, source_bytes))
    })
}

fn is_macro_call_keyword(ident: &str, lang: Lang) -> bool {
    lang.semantics().is_builtin_keyword(ident)
        || matches!(
            ident,
            "if" | "for" | "while" | "switch" | "return" | "defined" | "typeof"
        )
}

fn scan_macro_body_calls(
    def_node: Option<tree_sitter::Node<'_>>,
    macro_name: &str,
    line: u32,
    start_byte: usize,
    lang: Lang,
    source_bytes: &[u8],
    refs: &mut Vec<RawRef>,
) {
    let Some(val_node) = def_node.and_then(|n| n.child_by_field_name("value")) else {
        return;
    };
    let Ok(text) = val_node.utf8_text(source_bytes) else {
        return;
    };
    for seg in text.split('(').collect::<Vec<_>>().windows(2) {
        let prefix = seg[0].trim_end();
        let ident: &str = prefix
            .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        let valid = ident.len() >= 2
            && ident != macro_name
            && ident
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && !is_macro_call_keyword(ident, lang);
        if valid {
            refs.push(RawRef {
                callee_name: ident.to_string(),
                qualifier: None,
                line,
                name_line: line,
                start_byte: start_byte.saturating_add(1),
                role: RefRole::Call,
                arg_count: 0,
                arg_count_known: false,
                lang: None,
                receiver: None,
                receiver_type: None,
                callable_field: false,
                macro_arg: false,
                member_call: false,
            });
        }
    }
}

fn record_extracted_item(
    ext: MatchExtract<'_>,
    source_bytes: &[u8],
    lang: Lang,
    defs: &mut Vec<RawDef>,
    refs: &mut Vec<RawRef>,
) {
    if let Some((kind, _, start, _, line)) = ext.def {
        record_def_item(&ext, source_bytes, lang, defs);
        if kind == SymKind::Macro
            && let Some(name) = &ext.name
        {
            scan_macro_body_calls(ext.def_node, name, line, start, lang, source_bytes, refs);
        }
    } else if ext.r#ref.is_some() {
        record_ref_item(&ext, source_bytes, lang, refs);
    }
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
        let mut defs = Vec::new();
        let mut refs = Vec::new();

        {
            let mut matches = cursor.matches(query, tree.root_node(), source_bytes);
            while let Some(m) = matches.next() {
                let ext = extract_match_captures(m, query, source_bytes);
                record_extracted_item(ext, source_bytes, lang, &mut defs, &mut refs);
            }
        }
        let bindings = lang
            .semantics()
            .read_bindings(tree.root_node(), source_bytes);
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
        for r in &mut refs {
            if r.receiver_type.is_none()
                && let Some(receiver) = &r.receiver
                && let Some(ty) = receiver_type_at(receiver, r.start_byte, &bindings, lang)
            {
                r.receiver_type = Some(ty);
            }
        }
        if lang.semantics().supports_embedded_sql() {
            for guest in crate::languages::ALL_LANGS {
                guest
                    .semantics()
                    .embedded_refs(tree.root_node(), source_bytes, &mut refs);
            }
        }
        qualify_import_bound_receivers(&mut refs, lang);

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
        dedup_conditional_macros(&mut defs);
        FileParseResult {
            line_count,
            byte_size: source_bytes.len() as u64,
            defs,
            refs,
            foreign_exports: lang
                .semantics()
                .foreign_exports(tree.root_node(), source_bytes),
            callable_bindings: lang
                .semantics()
                .callable_bindings(tree.root_node(), source_bytes),
            implementations: lang
                .semantics()
                .implementations(tree.root_node(), source_bytes),
        }
    })
}

/// Give `alias.f()` the qualifier `alias` when this file's own imports bind `alias`.
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
    for r in refs.iter_mut() {
        if r.receiver.as_ref().is_some_and(|recv| bound.contains(recv)) {
            r.qualifier = r.receiver.take();
        }
    }
}
