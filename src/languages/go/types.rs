//! Go's declared types: locals, parameters, receivers, struct fields and function results, typed from their
//! declarations. A type of a standard-library package is `pkg::T` and a predeclared type is `builtin::T`; neither
//! names an in-tree type, so a call on either is external. A type of any other package is left untyped, because its
//! import path cannot be mapped to a directory of the tree.

use crate::languages::Implementation;
use crate::languages::reads::{EXTENDS, ReadBindings, enclosing_scope, nodes};
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

const PREDECLARED_TYPES: &[&str] = &[
    "any",
    "bool",
    "byte",
    "comparable",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
];

/// What a `:=` or `var` initializer gives its binding: a declared type, or the result of a function to type later.
enum InitType {
    Typed(String),
    Result(String),
}

/// The file facts a declared type is read against: its imports, and the type parameters it declares.
struct GoScope<'a> {
    source: &'a [u8],
    /// Each name a file import binds, `true` when the imported package is the standard library.
    packages: HashMap<&'a str, bool>,
    generics: HashSet<&'a str>,
}

impl<'a> GoScope<'a> {
    fn of(root: Node<'_>, source: &'a [u8]) -> Self {
        let mut scope = Self {
            source,
            packages: HashMap::new(),
            generics: HashSet::new(),
        };
        for node in nodes(root) {
            match node.kind() {
                "import_spec" => scope.record_import(node),
                "type_parameter_declaration" => {
                    let mut cursor = node.walk();
                    for name in node.children_by_field_name("name", &mut cursor) {
                        scope.generics.extend(go_text(name, source));
                    }
                }
                _ => {}
            }
        }
        scope
    }

    fn record_import(&mut self, spec: Node<'_>) {
        let Some(path) = spec
            .child_by_field_name("path")
            .and_then(|path| go_text(path, self.source))
            .map(|path| path.trim_matches(|c| c == '"' || c == '`'))
        else {
            return;
        };
        let bound = match spec.child_by_field_name("name") {
            Some(alias) => match go_text(alias, self.source) {
                Some("_" | ".") | None => return,
                Some(name) => name,
            },
            None => package_name(path),
        };
        let standard = !path.split('/').next().unwrap_or("").contains('.');
        self.packages.insert(bound, standard);
    }

    fn is_standard(&self, package: &str) -> bool {
        self.packages.get(package) == Some(&true)
    }

    fn is_third_party(&self, package: &str) -> bool {
        self.packages.get(package) == Some(&false)
    }

    /// The type a declared type expression names, as the resolver reads it; `None` when it names no type it can say.
    fn type_text(&self, ty: Node<'_>) -> Option<String> {
        match ty.kind() {
            "type_identifier" => {
                let name = go_text(ty, self.source)?;
                if self.generics.contains(name) {
                    None
                } else if PREDECLARED_TYPES.contains(&name) {
                    Some(format!("builtin::{name}"))
                } else {
                    Some(name.to_string())
                }
            }
            "pointer_type" => self.type_text(ty.named_child(0)?),
            "generic_type" => self.type_text(ty.child_by_field_name("type")?),
            "qualified_type" => {
                let package = go_text(ty.child_by_field_name("package")?, self.source)?;
                let name = go_text(ty.child_by_field_name("name")?, self.source)?;
                self.is_standard(package)
                    .then(|| format!("{package}::{name}"))
            }
            _ => None,
        }
    }
}

/// The name a package import binds when it has no alias: the last path element, past a major-version suffix.
fn package_name(path: &str) -> &str {
    let mut parts = path.rsplit('/');
    let last = parts.next().unwrap_or(path);
    let is_version =
        last.len() > 1 && last.starts_with('v') && last[1..].bytes().all(|b| b.is_ascii_digit());
    match parts.next() {
        Some(previous) if is_version => previous,
        _ => last,
    }
}

fn go_text<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    node.utf8_text(source).ok()
}

fn go_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn go_child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    go_children(node)
        .into_iter()
        .find(|child| child.kind() == kind)
}

/// Reads the type facts of one declaration into the bindings, against the file's scope.
type Binder = for<'a> fn(&mut ReadBindings<'a>, Node<'_>, &GoScope<'a>);

/// Each declaration kind that declares a type fact, with the binder that reads it.
const BINDERS: &[(&str, Binder)] = &[
    ("type_spec", struct_fields),
    ("function_declaration", bind_signature),
    ("method_declaration", bind_signature),
    ("func_literal", bind_signature),
    ("short_var_declaration", bind_short_vars),
    ("var_spec", bind_var_spec),
];

pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let scope = GoScope::of(root, source);
    let mut bindings = ReadBindings::default();
    for node in nodes(root) {
        if let Some((_, bind)) = BINDERS.iter().find(|(kind, _)| *kind == node.kind()) {
            bind(&mut bindings, node, &scope);
        }
    }
    bindings
}

/// The declared result type of each free function whose result is one type.
pub(super) fn function_returns(root: Node<'_>, source: &[u8]) -> Vec<(String, String)> {
    let scope = GoScope::of(root, source);
    nodes(root)
        .filter(|node| node.kind() == "function_declaration")
        .filter_map(|function| {
            let name = go_text(function.child_by_field_name("name")?, source)?;
            let result = function.child_by_field_name("result")?;
            let ty = scope.type_text(result)?;
            Some((name.to_string(), ty))
        })
        .collect()
}

/// Fields of a struct type: each named field by its type, and an embedded field under the name of its type.
fn struct_fields<'a>(bindings: &mut ReadBindings<'a>, spec: Node<'_>, scope: &GoScope<'a>) {
    let (Some(owner), Some(ty)) = (
        spec.child_by_field_name("name")
            .and_then(|name| go_text(name, scope.source)),
        spec.child_by_field_name("type"),
    ) else {
        return;
    };
    if ty.kind() != "struct_type" {
        return;
    }
    let Some(list) = go_child_of_kind(ty, "field_declaration_list") else {
        return;
    };
    for field in go_children(list) {
        if field.kind() == "field_declaration" {
            struct_field(bindings, owner, field, scope);
        }
    }
}

fn struct_field<'a>(
    bindings: &mut ReadBindings<'a>,
    owner: &str,
    field: Node<'_>,
    scope: &GoScope<'a>,
) {
    let Some(ty_node) = field.child_by_field_name("type") else {
        return;
    };
    let ty = scope.type_text(ty_node);
    let mut cursor = field.walk();
    let names: Vec<&str> = field
        .children_by_field_name("name", &mut cursor)
        .filter_map(|name| go_text(name, scope.source))
        .collect();
    if names.is_empty() {
        let Some(base) = embedded_type_name(ty_node, scope.source) else {
            return;
        };
        if let Some(ty) = ty {
            if !ty.contains("::") && !ty.starts_with("builtin::") {
                bindings.insert_member(owner, EXTENDS, ty.clone());
            }
            bindings.insert_member(owner, base, ty);
        }
        return;
    }
    if let Some(ty) = ty {
        for name in names {
            bindings.insert_member(owner, name, ty.clone());
        }
    }
}

/// The name an embedded field takes: its type's name, past a pointer, type arguments and a package qualifier.
fn embedded_type_name<'a>(ty: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    match ty.kind() {
        "type_identifier" => go_text(ty, source),
        "pointer_type" => embedded_type_name(ty.named_child(0)?, source),
        "generic_type" => embedded_type_name(ty.child_by_field_name("type")?, source),
        "qualified_type" => go_text(ty.child_by_field_name("name")?, source),
        _ => None,
    }
}

/// Parameters, and the receiver of a method, are bound over the whole function.
fn bind_signature<'a>(bindings: &mut ReadBindings<'a>, function: Node<'_>, scope: &GoScope<'a>) {
    let span = function.byte_range();
    for field in ["receiver", "parameters"] {
        if let Some(list) = function.child_by_field_name(field) {
            bind_parameters(bindings, list, span.clone(), scope);
        }
    }
}

fn bind_parameters<'a>(
    bindings: &mut ReadBindings<'a>,
    list: Node<'_>,
    span: std::ops::Range<usize>,
    scope: &GoScope<'a>,
) {
    for decl in go_children(list)
        .into_iter()
        .filter(|decl| decl.kind() == "parameter_declaration")
    {
        let ty = decl
            .child_by_field_name("type")
            .and_then(|ty| scope.type_text(ty));
        let mut cursor = decl.walk();
        for name in decl.children_by_field_name("name", &mut cursor) {
            if let Some(name) = go_text(name, scope.source).filter(|name| *name != "_") {
                bindings.insert(name, span.clone(), ty.clone());
            }
        }
    }
}

fn bind_short_vars<'a>(bindings: &mut ReadBindings<'a>, statement: Node<'_>, scope: &GoScope<'a>) {
    let (Some(left), Some(right)) = (
        statement.child_by_field_name("left"),
        statement.child_by_field_name("right"),
    ) else {
        return;
    };
    let names = go_children(left);
    let values = go_children(right);
    if names.len() != values.len() {
        return;
    }
    let Some(block) = enclosing_scope(statement, |kind| kind == "block") else {
        return;
    };
    let span = statement.end_byte()..block.end_byte();
    for (name, value) in names.into_iter().zip(values) {
        let Some(name) = go_text(name, scope.source).filter(|name| *name != "_") else {
            continue;
        };
        bind_value(bindings, name, span.clone(), initial_type(value, scope));
    }
}

fn bind_var_spec<'a>(bindings: &mut ReadBindings<'a>, spec: Node<'_>, scope: &GoScope<'a>) {
    let declared = spec
        .child_by_field_name("type")
        .and_then(|ty| scope.type_text(ty));
    let values = spec
        .child_by_field_name("value")
        .map(go_children)
        .unwrap_or_default();
    let mut cursor = spec.walk();
    let names: Vec<&str> = spec
        .children_by_field_name("name", &mut cursor)
        .filter_map(|name| go_text(name, scope.source))
        .collect();
    let block = enclosing_scope(spec, |kind| kind == "block");
    for (index, name) in names.into_iter().enumerate() {
        if name == "_" {
            continue;
        }
        let init = match &declared {
            Some(ty) => Some(InitType::Typed(ty.clone())),
            None => values
                .get(index)
                .and_then(|value| initial_type(*value, scope)),
        };
        match block {
            Some(block) => bind_value(bindings, name, spec.end_byte()..block.end_byte(), init),
            None => {
                if let Some(InitType::Typed(ty)) = init {
                    bindings.insert_module(name, Some(ty));
                }
            }
        }
    }
}

fn bind_value<'a>(
    bindings: &mut ReadBindings<'a>,
    name: &'a str,
    span: std::ops::Range<usize>,
    init: Option<InitType>,
) {
    match init {
        Some(InitType::Typed(ty)) => bindings.insert(name, span, Some(ty)),
        Some(InitType::Result(callee)) => bindings.insert_call(name, span, callee),
        None => bindings.insert(name, span, None),
    }
}

/// The type a value expression gives its binding: a composite literal (`T{}`, `&T{}`), `new(T)`, or the result of a
/// function call, which the cross-file pass types.
fn initial_type(value: Node<'_>, scope: &GoScope<'_>) -> Option<InitType> {
    match value.kind() {
        "composite_literal" => {
            let ty = value.child_by_field_name("type")?;
            scope.type_text(ty).map(InitType::Typed)
        }
        "unary_expression" if go_text(value, scope.source)?.starts_with('&') => {
            initial_type(value.named_child(0)?, scope)
        }
        "call_expression" => initial_call(value, scope),
        _ => None,
    }
}

fn initial_call(call: Node<'_>, scope: &GoScope<'_>) -> Option<InitType> {
    let function = call.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => {
            let name = go_text(function, scope.source)?;
            if name != "new" {
                return Some(InitType::Result(name.to_string()));
            }
            let argument = go_children(call.child_by_field_name("arguments")?)
                .into_iter()
                .next()?;
            scope.type_text(argument).map(InitType::Typed)
        }
        "selector_expression" => {
            let operand = function.child_by_field_name("operand")?;
            let package = go_text(operand, scope.source)?;
            if operand.kind() != "identifier" || !scope.is_third_party(package) {
                return None;
            }
            let field = go_text(function.child_by_field_name("field")?, scope.source)?;
            Some(InitType::Result(field.to_string()))
        }
        _ => None,
    }
}

/// Each method is a member of its receiver's type: a span of its own, with the receiver's base type as subject.
pub(super) fn method_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    nodes(root)
        .filter(|node| node.kind() == "method_declaration")
        .filter_map(|method| {
            let receiver = go_children(method.child_by_field_name("receiver")?)
                .into_iter()
                .find(|decl| decl.kind() == "parameter_declaration")?;
            let subject = embedded_type_name(receiver.child_by_field_name("type")?, source)?;
            Some(Implementation {
                subject: subject.to_string(),
                trait_path: None,
                subject_bound: None,
                generic_subject: false,
                start_byte: method.start_byte(),
                end_byte: method.end_byte(),
            })
        })
        .collect()
}

/// A struct that embeds an in-tree type takes it as its superclass, so a method or field the struct lacks falls
/// through to the embedded type.
pub(super) fn embedded_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    nodes(root)
        .filter(|node| node.kind() == "type_spec")
        .flat_map(|spec| embedded_bases(spec, source))
        .collect()
}

fn embedded_bases(spec: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    let (Some(owner), Some(ty)) = (
        spec.child_by_field_name("name")
            .and_then(|name| go_text(name, source)),
        spec.child_by_field_name("type")
            .filter(|ty| ty.kind() == "struct_type"),
    ) else {
        return Vec::new();
    };
    let Some(list) = go_child_of_kind(ty, "field_declaration_list") else {
        return Vec::new();
    };
    go_children(list)
        .into_iter()
        .filter(|field| {
            field.kind() == "field_declaration" && field.child_by_field_name("name").is_none()
        })
        .filter_map(|field| in_tree_embedded(field.child_by_field_name("type")?, source))
        .map(|base| Implementation {
            subject: owner.to_string(),
            trait_path: Some(base.to_string()),
            subject_bound: None,
            generic_subject: false,
            start_byte: spec.start_byte(),
            end_byte: spec.end_byte(),
        })
        .collect()
}

/// The name of an embedded type declared in the tree; a qualified type names another package's.
fn in_tree_embedded<'a>(ty: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    match ty.kind() {
        "type_identifier" => go_text(ty, source),
        "pointer_type" => in_tree_embedded(ty.named_child(0)?, source),
        "generic_type" => in_tree_embedded(ty.child_by_field_name("type")?, source),
        _ => None,
    }
}
