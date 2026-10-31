use super::c::{C_FAMILY_KEYWORDS, CPP_STD_MEMBERS, CPP_STD_QUALIFIERS, resolve_c_family_include};
use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds};
use std::collections::HashMap;

pub struct ObjCLanguage;
pub static OBJC_LANG: ObjCLanguage = ObjCLanguage;

/// Classes an ObjC file declares or implements (`@interface X` / `@implementation X`): their methods
/// are the members a receiver typed `X` reaches.
fn collect_class_subjects(node: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<super::Implementation>) {
    if matches!(node.kind(), "class_interface" | "class_implementation") {
        let mut cursor = node.walk();
        let name = node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "identifier")
            .and_then(|c| c.utf8_text(source).ok());
        if let Some(subject) = name.filter(|n| crate::languages::is_plain_type_name(n)) {
            let span = (node.start_byte(), node.end_byte());
            out.push(super::typed::implementation(subject, None, span));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_class_subjects(child, source, out);
    }
}
/// The class name a declaration's type spells: `Store` in `Store *s` (a `type_specifier` wrapper or a
/// bare `type_identifier`).
fn plain_type_identifier(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "type_identifier" => Some(node),
        "type_specifier" => node.named_child(0).filter(|n| n.kind() == "type_identifier"),
        _ => None,
    }
}

/// The identifier a declarator binds: `s` in `s`, `*s = x`, or `*s[2]`.
fn declared_identifier(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "identifier" => Some(node),
        "init_declarator"
        | "pointer_declarator"
        | "parenthesized_declarator"
        | "array_declarator" => declared_identifier(node.child_by_field_name("declarator")?),
        _ => None,
    }
}

/// Records `Type *name` declarations (`Type` a plain class name) with the enclosing block as their
/// scope, so a later `[name msg]` receiver is typed by its declaration.
fn collect_declared_types<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
    scope: std::ops::Range<usize>,
    out: &mut super::reads::ReadBindings<'a>,
) {
    let scope = if node.kind() == "compound_statement" {
        node.byte_range()
    } else {
        scope
    };
    if node.kind() == "declaration"
        && let Some(ty) = node
            .child_by_field_name("type")
            .and_then(|t| plain_type_identifier(t))
            .and_then(|t| t.utf8_text(source).ok())
    {
        let mut cursor = node.walk();
        for decl in node.children_by_field_name("declarator", &mut cursor) {
            if let Some(name) = declared_identifier(decl).and_then(|n| n.utf8_text(source).ok()) {
                out.insert(name, scope.clone(), Some(ty.to_string()));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_declared_types(child, source, scope.clone(), out);
    }
}


const OBJC_FOUNDATION_MEMBERS: &[&str] = &[
    "alloc",
    "init",
    "new",
    "copy",
    "mutableCopy",
    "description",
    "hash",
    "isEqual",
    "count",
    "objectForKey",
    "setObject",
    "addObject",
    "removeObject",
    "removeAllObjects",
    "containsObject",
    "firstObject",
    "lastObject",
    "stringWithFormat",
    "UTF8String",
    "length",
    "boolValue",
    "intValue",
    "integerValue",
    "doubleValue",
];

const OBJC_FOUNDATION_QUALIFIERS: &[&str] = &[
    "NSObject",
    "NSString",
    "NSMutableString",
    "NSArray",
    "NSMutableArray",
    "NSDictionary",
    "NSMutableDictionary",
    "NSSet",
    "NSMutableSet",
    "NSNumber",
    "NSData",
    "NSMutableData",
    "NSURL",
    "NSURLRequest",
    "NSURLSession",
    "NSFileManager",
    "NSBundle",
    "NSThread",
    "NSNotificationCenter",
    "NSUserDefaults",
    "NSDate",
    "NSError",
    "NSJSONSerialization",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "catch_clause",
        "conditional_expression",
    ],
    ..ComplexityKinds::C_STYLE
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "class_declaration",
        "class_interface",
        "class_implementation",
        "struct_specifier",
        "struct_declaration",
        "protocol_declaration",
    ],
    member_access: &["field_expression", "message_expression"],
    field_access: &["field_expression"],
    body_blocks: &[
        "compound_statement",
        "declaration_list",
        "field_declaration_list",
        "implementation_definition",
    ],
    body_wrappers: &["property_declaration", "parenthesized_expression"],
    function_defs: &["function_definition", "method_definition"],
    function_declarators: &["function_declarator"],
    non_local_declarations: &["class_declaration"],
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

impl LanguageSemantics for ObjCLanguage {
    crate::languages::c::flags::c_family_hooks!();

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }
    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }
    fn indexes_implementations(&self) -> bool {
        true
    }
    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::Implementation> {
        let mut out = Vec::new();
        collect_class_subjects(root, source, &mut out);
        out
    }
    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        let mut bindings = super::reads::ReadBindings::default();
        collect_declared_types(root, source, root.byte_range(), &mut bindings);
        bindings
    }
    fn has_declarations(&self) -> bool {
        true
    }
    fn declaration_ties(&self, decl: &crate::domain::Symbol, def: &crate::domain::Symbol) -> bool {
        super::c::c_family_declaration_ties(decl, def)
    }
    fn tag(&self) -> &'static str {
        "m"
    }
    super::impl_grammar_method!(crate tree_sitter_objc::LANGUAGE, "tags.scm");

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Cast, G::Goto, G::DoWhile, G::Insecure, G::Parens, G::CFamily, G::SelfAssign, G::Unreachable, G::Naming]
    }

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef", "enum"]
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["m", "mm"]
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
    }

    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        Some(super::c::layout::parse_struct_fields)
    }

    fn is_builtin_keyword(&self, name: &str) -> bool {
        C_FAMILY_KEYWORDS.contains(&name)
    }

    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        CPP_STD_MEMBERS.contains(&name) || OBJC_FOUNDATION_MEMBERS.contains(&name)
    }

    fn is_std_or_builtin_qualifier(&self, qualifier: &str) -> bool {
        CPP_STD_QUALIFIERS.contains(&qualifier) || OBJC_FOUNDATION_QUALIFIERS.contains(&qualifier)
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
        super::c::is_c_internal_function(root, name_byte, source, false)
    }
}
