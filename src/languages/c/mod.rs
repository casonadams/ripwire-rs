pub mod layout;
pub mod flags;
pub(crate) mod lint;

use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    is_screaming_snake, resolve_relative_or_exact, strip_import_quotes,
};
use crate::domain::SymKind;
use std::collections::HashMap;

pub struct CLanguage;
pub static C_LANG: CLanguage = CLanguage;

pub(super) const C_FAMILY_KEYWORDS: &[&str] = &[
    "static_cast",
    "dynamic_cast",
    "const_cast",
    "reinterpret_cast",
    "sizeof",
    "alignof",
    "decltype",
    "typeid",
    "static_assert",
    "noexcept",
];

pub(super) const CPP_STD_MEMBERS: &[&str] = &[
    "append",
    "append_range",
    "assign",
    "assign_range",
    "at",
    "back",
    "before_begin",
    "begin",
    "bucket_count",
    "c_str",
    "capacity",
    "cbefore_begin",
    "cbegin",
    "cend",
    "clear",
    "contains",
    "crbegin",
    "crend",
    "data",
    "emplace",
    "emplace_after",
    "emplace_back",
    "emplace_front",
    "emplace_hint",
    "empty",
    "end",
    "ends_with",
    "equal_range",
    "erase",
    "erase_after",
    "find",
    "find_first_not_of",
    "find_first_of",
    "find_last_not_of",
    "find_last_of",
    "front",
    "get_allocator",
    "has_value",
    "hash_function",
    "insert",
    "insert_after",
    "insert_or_assign",
    "insert_range",
    "insert_range_after",
    "key_comp",
    "key_eq",
    "length",
    "load_factor",
    "lower_bound",
    "max_bucket_count",
    "max_load_factor",
    "max_size",
    "owner_before",
    "pop_back",
    "pop_front",
    "prepend_range",
    "push_back",
    "push_front",
    "rbegin",
    "rehash",
    "remove_prefix",
    "remove_suffix",
    "rend",
    "reserve",
    "resize",
    "resize_and_overwrite",
    "rfind",
    "shrink_to_fit",
    "size",
    "size_bytes",
    "starts_with",
    "string",
    "subspan",
    "substr",
    "to_string",
    "try_emplace",
    "upper_bound",
    "use_count",
    "value_comp",
    "value_or",
];

pub(super) const CPP_STD_QUALIFIERS: &[&str] = &[
    "std",
    "boost",
    "chrono",
    "filesystem",
    "fs",
    "ranges",
    "views",
    "this_thread",
    "metal",
];

const TEST_BLOCK_MACRO_NAMES: &[&str] = &[
    "TEST_CASE",
    "TEST_CASE_FIXTURE",
    "TEST_CASE_METHOD",
    "SCENARIO",
    "TEST_SUITE",
];

pub(super) fn resolve_c_family_include(
    importer_path: &str,
    raw_target: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let trimmed = raw_target.trim();
    if trimmed.starts_with('<') {
        return None;
    }
    let clean = strip_import_quotes(trimmed);
    resolve_relative_or_exact(importer_path, clean, file_index, &[".h", ".hpp", ".hh"])
}

fn strip_cpp_attribute_prefix(slice: &[u8]) -> &[u8] {
    let s = slice.trim_ascii_start();
    if s.starts_with(b"[[")
        && let Some(close) = s.windows(2).position(|w| w == b"]]")
    {
        return s[close + 2..].trim_ascii_start();
    }
    s
}

fn trailing_macro_before_semicolon(line: &[u8], trimmed_end: usize) -> Option<(usize, usize)> {
    if trimmed_end < 4 || line[trimmed_end - 1] != b';' {
        return None;
    }
    let before_semi = &line[..trimmed_end - 1];
    let tok_end = before_semi.iter().rposition(|&b| b != b' ' && b != b'\t')? + 1;
    let tok_start = before_semi[..tok_end]
        .iter()
        .rposition(|&b| !b.is_ascii_alphanumeric() && b != b'_')
        .map_or(0, |p| p + 1);
    let prefix = before_semi[..tok_start].trim_ascii_end();
    let after_sig = prefix.ends_with(b")") || prefix.ends_with(b"noexcept");
    let tok = std::str::from_utf8(&before_semi[tok_start..tok_end]).ok()?;
    (after_sig && is_screaming_snake(tok)).then_some((tok_start, tok_end))
}

fn is_semicolonless_macro_line(line: &[u8]) -> Option<(usize, usize)> {
    let start = line.iter().position(|&b| b != b' ' && b != b'\t')?;
    let trimmed_end = line
        .iter()
        .rposition(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))?
        + 1;
    let slice = &line[start..trimmed_end];
    if slice.first() == Some(&b'#') {
        return None;
    }
    if let Some(span) = trailing_macro_before_semicolon(line, trimmed_end) {
        return Some(span);
    }
    let attr_tail = strip_cpp_attribute_prefix(slice);
    if let Ok(tok) = std::str::from_utf8(attr_tail)
        && is_screaming_snake(tok.trim())
    {
        let tail_start = trimmed_end - attr_tail.len();
        return Some((tail_start, trimmed_end));
    }
    if !slice.ends_with(b")") {
        return None;
    }
    let open_pos = slice.iter().position(|&b| b == b'(')?;
    let macro_name = std::str::from_utf8(&slice[..open_pos]).ok()?.trim();
    is_screaming_snake(macro_name).then_some((start, trimmed_end))
}

pub(super) fn blank_c_family_macro_lines(source_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = source_bytes.to_vec();
    let mut changed = false;
    let mut offset = 0usize;
    for line in source_bytes.split_inclusive(|&b| b == b'\n') {
        if let Some((s, e)) = is_semicolonless_macro_line(line) {
            out[offset + s..offset + e].fill(b' ');
            changed = true;
        }
        offset += line.len();
    }
    changed.then_some(out)
}

fn matches_child_token(
    ch: tree_sitter::Node<'_>,
    src: &[u8],
    kind_filter: &str,
    allow_anon: bool,
    tokens: &[&str],
) -> bool {
    let named = ch.is_named();
    if (named && ch.kind() != kind_filter) || (!named && !allow_anon) {
        return false;
    }
    ch.utf8_text(src).is_ok_and(|t| tokens.contains(&t))
}

fn has_child_token(
    node: tree_sitter::Node<'_>,
    src: &[u8],
    kind_filter: &str,
    allow_anon: bool,
    tokens: &[&str],
) -> bool {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .any(|ch| matches_child_token(ch, src, kind_filter, allow_anon, tokens))
}

fn has_const_qualifier(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    has_child_token(
        node,
        src,
        "type_qualifier",
        false,
        &["const", "constexpr", "constinit"],
    )
}

fn has_static_storage(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    has_child_token(node, src, "storage_class_specifier", false, &["static"])
}

fn function_declarator_node(mut node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    loop {
        if node.kind() == "function_declarator" {
            return Some(node);
        }
        node = node.child_by_field_name("declarator")?;
    }
}

fn matching_function_declaration(
    declaration: tree_sitter::Node<'_>,
    definition: tree_sitter::Node<'_>,
    source: &[u8],
    overloads: bool,
) -> bool {
    let Some(signature) = definition
        .child_by_field_name("declarator")
        .and_then(function_declarator_node)
    else {
        return false;
    };
    let fields: &[&str] = if overloads {
        &["declarator", "parameters"]
    } else {
        &["declarator"]
    };
    let mut cursor = declaration.walk();
    declaration
        .children_by_field_name("declarator", &mut cursor)
        .filter_map(function_declarator_node)
        .any(|candidate| {
            fields.iter().all(|field| {
                candidate
                    .child_by_field_name(field)
                    .zip(signature.child_by_field_name(field))
                    .is_some_and(|(left, right)| {
                        source[left.byte_range()] == source[right.byte_range()]
                    })
            })
        })
}

fn is_enclosing_file_scope(decl_node: tree_sitter::Node<'_>) -> bool {
    let mut p = decl_node.parent();
    while let Some(parent) = p {
        match parent.kind() {
            "compound_statement"
            | "field_declaration_list"
            | "struct_specifier"
            | "class_specifier"
            | "interface_definition" => return false,
            "translation_unit" => return true,
            _ => p = parent.parent(),
        }
    }
    false
}

pub(super) fn is_c_internal_function(
    root: tree_sitter::Node<'_>,
    name_byte: usize,
    source: &[u8],
    overloads: bool,
) -> bool {
    if name_byte >= source.len() {
        return false;
    }
    let Some(name_node) = root.descendant_for_byte_range(name_byte, name_byte) else {
        return false;
    };
    let mut curr = Some(name_node);
    let mut decl = None;
    while let Some(n) = curr {
        match n.kind() {
            "function_definition" | "declaration" => {
                decl = Some(n);
                break;
            }
            _ => curr = n.parent(),
        }
    }
    let Some(decl_node) = decl else {
        return false;
    };
    if !is_enclosing_file_scope(decl_node) {
        return false;
    }
    if has_static_storage(decl_node, source) {
        return true;
    }
    if let Some(parent) = decl_node.parent()
        && parent.kind() == "template_declaration"
        && has_static_storage(parent, source)
    {
        return true;
    }
    let mut previous = decl_node.prev_named_sibling();
    while let Some(sibling) = previous {
        if sibling.kind() == "declaration"
            && has_static_storage(sibling, source)
            && matching_function_declaration(sibling, decl_node, source, overloads)
        {
            return true;
        }
        previous = sibling.prev_named_sibling();
    }
    false
}

fn is_bound_by_init_declarator(
    name_node: Option<tree_sitter::Node<'_>>,
    def_node: tree_sitter::Node<'_>,
) -> bool {
    let mut curr = name_node.and_then(|n| n.parent());
    while let Some(n) = curr {
        if n == def_node {
            break;
        }
        if n.kind() == "init_declarator" {
            return true;
        }
        curr = n.parent();
    }
    false
}

fn is_cpp_global_constant_allowed(
    dn: tree_sitter::Node<'_>,
    name_node: Option<tree_sitter::Node<'_>>,
    name: &str,
    src: &[u8],
) -> bool {
    let initialized = is_bound_by_init_declarator(name_node, dn);
    if initialized {
        return is_screaming_snake(name)
            || has_const_qualifier(dn, src)
            || has_child_token(dn, src, "type_qualifier", true, &["__constant__"]);
    }
    has_child_token(dn, src, "type_qualifier", true, &["__constant__"])
        || (is_screaming_snake(name)
            && has_child_token(
                dn,
                src,
                "type_qualifier",
                true,
                &["__device__", "__managed__"],
            ))
}

fn is_c_constant_allowed(ctx: &DefCaptureGateContext<'_>, is_cpp: bool) -> bool {
    let Some(dn) = ctx.def_node else {
        return false;
    };
    if !is_cpp {
        return is_screaming_snake(ctx.name) || has_const_qualifier(dn, ctx.source_bytes);
    }
    if dn.kind() == "field_declaration" {
        return has_static_storage(dn, ctx.source_bytes)
            && has_const_qualifier(dn, ctx.source_bytes);
    }
    is_cpp_global_constant_allowed(dn, ctx.name_node, ctx.name, ctx.source_bytes)
}

fn has_pre_body_error(node: tree_sitter::Node<'_>) -> bool {
    for i in 0..node.child_count() {
        let Some(ch) = node.child(i) else {
            continue;
        };
        if ch.kind() == "compound_statement" {
            break;
        }
        if ch.is_error() || ch.kind() == "ERROR" {
            return true;
        }
    }
    false
}

fn unwrap_declarator_container(fn_decl: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut decl = fn_decl.parent();
    while let Some(p) = decl {
        if matches!(
            p.kind(),
            "pointer_declarator"
                | "reference_declarator"
                | "parenthesized_declarator"
                | "attributed_declarator"
                | "function_declarator"
        ) {
            decl = p.parent();
        } else {
            return Some(p);
        }
    }
    None
}

fn is_invalid_c_callable_scope(fn_decl: tree_sitter::Node<'_>) -> bool {
    if fn_decl.has_error() {
        return true;
    }
    let Some(container) = unwrap_declarator_container(fn_decl) else {
        return true;
    };
    if !matches!(
        container.kind(),
        "function_definition" | "declaration" | "field_declaration" | "template_declaration"
    ) || has_pre_body_error(container)
    {
        return true;
    }
    let mut curr = container.parent();
    while let Some(p) = curr {
        match p.kind() {
            "compound_statement" | "ERROR" | "if_statement" | "for_statement"
            | "while_statement" => return true,
            "field_declaration_list" | "declaration_list" | "translation_unit" => return false,
            _ => curr = p.parent(),
        }
    }
    false
}

fn has_cpp_header_markers(text: &str) -> bool {
    [
        "namespace ",
        "template<",
        "template <",
        "class ",
        "constexpr",
        "std::",
        "::",
        "public:",
        "private:",
        "noexcept",
        "extern \"C\"",
        "virtual ",
        "typename",
        "nullptr",
        "operator",
        "friend ",
        "using ",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// True when the C grammar parses the header with no error. C++-only spellings the markers miss (`const T& x`)
/// leave an error here, and such a header stays on the C++ grammar, which parses C as well.
fn parses_as_c(source: &[u8]) -> bool {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .is_ok_and(|()| {
            parser
                .parse(source, None)
                .is_some_and(|tree| !tree.root_node().has_error())
        })
}

pub(super) fn refine_c_header_dialect(
    base_lang: crate::domain::Lang,
    rel_path: &str,
    source_bytes: &[u8],
) -> crate::domain::Lang {
    if !rel_path.ends_with(".h") {
        return base_lang;
    }
    if let Ok(text) = std::str::from_utf8(source_bytes) {
        if text.contains("@interface ") || text.contains("@protocol ") || text.contains("@property")
        {
            return crate::domain::Lang::ObjC;
        }
        let c_base = matches!(base_lang, crate::domain::Lang::C | crate::domain::Lang::Cpp);
        if c_base && !has_cpp_header_markers(text) && parses_as_c(source_bytes) {
            return crate::domain::Lang::C;
        }
    }
    crate::domain::Lang::Cpp
}

fn is_empty_preproc_function_macro(name_node: Option<tree_sitter::Node<'_>>) -> bool {
    let Some(define_node) = name_node.and_then(|n| n.parent()) else {
        return false;
    };
    define_node.kind() == "preproc_function_def"
        && define_node.child_by_field_name("value").is_none()
}

pub(super) fn is_c_family_def_allowed(ctx: &DefCaptureGateContext<'_>, is_cpp: bool) -> bool {
    match ctx.sub {
        Some("testmacroblock") => TEST_BLOCK_MACRO_NAMES.contains(&ctx.name),
        Some("constant") => is_c_constant_allowed(ctx, is_cpp),
        Some("specialization") => false,
        _ => match ctx.kind {
            SymKind::Function | SymKind::Method => {
                !ctx.def_node.is_some_and(is_invalid_c_callable_scope)
            }
            SymKind::Field => ctx
                .def_node
                .is_none_or(|dn| !has_static_storage(dn, ctx.source_bytes)),
            SymKind::Macro => !is_empty_preproc_function_macro(ctx.name_node),
            _ => true,
        },
    }
}

/// A declaration ties to the definition with its kind and scope, and its arity when callable (the
/// edit-check identity rule): a prototype, a member declared in its class, an `@interface`.
pub(super) fn c_family_declaration_ties(
    decl: &crate::domain::Symbol,
    def: &crate::domain::Symbol,
) -> bool {
    decl.kind == def.kind
        && decl.scope == def.scope
        && (!decl.kind.is_callable() || decl.params == def.params)
}

pub(super) fn resolve_c_family_def_body_and_end<'a>(
    def_node: Option<tree_sitter::Node<'a>>,
    default_end: usize,
) -> (Option<tree_sitter::Node<'a>>, usize) {
    let Some(dn) = def_node else {
        return (None, default_end);
    };
    if dn.kind() == "expression_statement"
        && let Some(sib) = dn.next_named_sibling()
        && sib.kind() == "compound_statement"
    {
        return (Some(sib), sib.end_byte());
    }
    if let Some(parent) = super::find_enclosing_definition_node(dn) {
        return (Some(parent), parent.end_byte());
    }
    (Some(dn), default_end)
}

const KINDS: ComplexityKinds = ComplexityKinds {
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "conditional_expression",
    ],
    jumps: &[
        "break_statement",
        "continue_statement",
        "goto_statement",
        "return_statement",
    ],
    ..ComplexityKinds::C_STYLE
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["struct_specifier"],
    member_access: &["field_expression"],
    field_access: &["field_expression"],
    body_blocks: &[
        "compound_statement",
        "declaration_list",
        "field_declaration_list",
    ],
    body_wrappers: &["parenthesized_expression"],
    function_defs: &["function_definition"],
    function_declarators: &["function_declarator"],
    identifiers: &["identifier"],
    callables: &["function_declarator"],
    callable_wrappers: &["parenthesized_expression"],
    signature_holders: &["declaration"],
    binding_patterns: &[
        "pointer_declarator",
        "array_declarator",
        "parenthesized_declarator",
    ],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for CLanguage {
    crate::languages::c::flags::c_family_hooks!();

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }
    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }
    fn has_declarations(&self) -> bool {
        true
    }
    fn declaration_ties(&self, decl: &crate::domain::Symbol, def: &crate::domain::Symbol) -> bool {
        c_family_declaration_ties(decl, def)
    }
    fn tag(&self) -> &'static str {
        "c"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Cast, G::Goto, G::DoWhile, G::Insecure, G::Parens, G::CFamily, G::SelfAssign, G::NativeOnly, G::Unreachable, G::Naming]
    }
    super::impl_grammar_method!(crate tree_sitter_c::LANGUAGE, "tags.scm");

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef", "enum"]
    }

    fn display_color(&self) -> &'static str {
        "#555555"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
    }

    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        Some(layout::parse_struct_fields)
    }

    fn refine_dialect(
        &self,
        base_lang: crate::domain::Lang,
        rel_path: &str,
        source_bytes: &[u8],
    ) -> crate::domain::Lang {
        refine_c_header_dialect(base_lang, rel_path, source_bytes)
    }

    /// In C a member call `x->f(...)` or `x.f(...)` can only reach a function-pointer field, never a
    /// free function, so the call is declined rather than bound by name to a same-named function.
    fn is_callable_field_access(&self, name_node: tree_sitter::Node<'_>) -> bool {
        let Some(member) = name_node.parent().filter(|p| p.kind() == "field_expression") else {
            return false;
        };
        member.parent().is_some_and(|call| {
            call.kind() == "call_expression" && call.child_by_field_name("function") == Some(member)
        })
    }

    fn is_builtin_keyword(&self, name: &str) -> bool {
        C_FAMILY_KEYWORDS.contains(&name)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        CPP_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        CPP_STD_QUALIFIERS
    }

    fn blank_error_macros(&self, source_bytes: &[u8]) -> Option<Vec<u8>> {
        blank_c_family_macro_lines(source_bytes)
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        is_c_family_def_allowed(ctx, false)
    }

    fn resolve_def_body_and_end<'a>(
        &self,
        def_node: Option<tree_sitter::Node<'a>>,
        default_end: usize,
    ) -> (Option<tree_sitter::Node<'a>>, usize) {
        resolve_c_family_def_body_and_end(def_node, default_end)
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        resolve_c_family_include(importer_path, raw_target, file_index)
    }
    fn supports_internal_linkage(&self) -> bool {
        true
    }

    fn is_internal_function(
        &self,
        root: tree_sitter::Node<'_>,
        name_byte: usize,
        source: &[u8],
    ) -> bool {
        is_c_internal_function(root, name_byte, source, false)
    }
}
