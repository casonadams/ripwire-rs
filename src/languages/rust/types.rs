use super::names::{UNWRAP_PATHS, WRAPPER_PATHS};
use super::scope::{LexicalNames, NameBinding, collect_lexical_names, field_text, note_agreed};

fn is_unit_struct(node: tree_sitter::Node<'_>) -> bool {
    let mut cursor = node.walk();
    !node.children(&mut cursor).any(|child| {
        matches!(
            child.kind(),
            "field_declaration_list" | "ordered_field_declaration_list"
        )
    })
}

struct AliasDecl<'t> {
    count: usize,
    value: Option<tree_sitter::Node<'t>>,
}

#[derive(Default)]
struct TypeDecls<'s, 't> {
    nominal: std::collections::HashSet<&'s str>,
    unit: std::collections::HashSet<&'s str>,
    aliases: std::collections::HashMap<&'s str, AliasDecl<'t>>,
}

fn collect_type_decls<'s, 't>(root: tree_sitter::Node<'t>, source: &'s [u8]) -> TypeDecls<'s, 't> {
    let mut decls = TypeDecls::default();
    for node in super::super::reads::nodes(root) {
        if !matches!(
            node.kind(),
            "struct_item" | "enum_item" | "union_item" | "trait_item" | "type_item"
        ) {
            continue;
        }
        let Some(name) = field_text(node, "name", source) else {
            continue;
        };
        match node.kind() {
            "struct_item" => {
                if is_unit_struct(node) {
                    decls.unit.insert(name);
                }
                decls.nominal.insert(name);
            }
            "type_item" => {
                let value = if node.child_by_field_name("type_parameters").is_some() {
                    None
                } else {
                    node.child_by_field_name("type")
                };
                let alias = decls.aliases.entry(name).or_insert(AliasDecl {
                    count: 0,
                    value: None,
                });
                alias.count += 1;
                alias.value = value;
            }
            _ => {
                decls.nominal.insert(name);
            }
        }
    }
    decls
}

fn resolve_aliases<'s>(
    cx: &TypeCx<'s>,
    decls: &TypeDecls<'s, '_>,
) -> std::collections::HashMap<&'s str, Option<String>> {
    decls
        .aliases
        .iter()
        .map(|(&name, alias)| {
            let target = alias
                .value
                .filter(|_| alias.count == 1 && !decls.nominal.contains(name))
                .and_then(|ty| extract_type_repr(cx, ty))
                .filter(|repr| !decls.aliases.contains_key(repr.as_str()));
            (name, target)
        })
        .collect()
}

/// Per-file type facts shared by receiver extraction; `None` entries mark non-authoritative names.
pub(super) struct TypeCx<'s> {
    pub(super) source: &'s [u8],
    lexical: LexicalNames,
    aliases: std::collections::HashMap<&'s str, Option<String>>,
    pub(super) unit_structs: std::collections::HashSet<&'s str>,
    pub(super) assoc_returns: std::collections::HashMap<&'s str, Assoc<'s>>,
    pub(super) free_returns: std::collections::HashMap<&'s str, Option<Returned>>,
}

impl<'s> TypeCx<'s> {
    pub(super) fn new(root: tree_sitter::Node<'_>, source: &'s [u8]) -> Self {
        let decls = collect_type_decls(root, source);
        let mut cx = Self {
            source,
            lexical: collect_lexical_names(root, source),
            aliases: std::collections::HashMap::new(),
            unit_structs: std::collections::HashSet::new(),
            assoc_returns: std::collections::HashMap::new(),
            free_returns: std::collections::HashMap::new(),
        };
        cx.aliases = resolve_aliases(&cx, &decls);
        cx.assoc_returns = collect_assoc_returns(&cx, root);
        cx.free_returns = collect_free_returns(&cx, root);
        cx.unit_structs = decls.unit;
        cx
    }
}

/// Bounds gathered for one type-parameter name; a name with several constraints has no unique bound.
#[derive(Default)]
pub(super) struct Constraints<'s> {
    count: usize,
    bound: Option<&'s str>,
}

impl<'s> Constraints<'s> {
    fn add(&mut self, bound: Option<&'s str>) {
        self.count += 1;
        self.bound = if self.count == 1 { bound } else { None };
    }

    pub(super) fn unique(self) -> Option<&'s str> {
        self.bound
    }
}

enum ParamLookup {
    Bound(String),
    Declared,
    Absent,
}

fn strip_generic_args(text: &str) -> &str {
    text.split(['<', '(']).next().unwrap_or(text).trim()
}

/// The single positive trait bound of a parameter; `None` when there are several or none.
fn extract_single_bound<'s>(bounds: tree_sitter::Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    let mut trait_bound = None;
    let mut cursor = bounds.walk();
    for child in bounds.children(&mut cursor) {
        if matches!(child.kind(), "lifetime" | ":" | "+") {
            continue;
        }
        let target = if child.kind() == "higher_ranked_trait_bound" {
            child.child_by_field_name("type")?
        } else {
            child
        };
        let text = target.utf8_text(source).ok()?.trim();
        if text.starts_with('?') || text.is_empty() {
            continue;
        }
        if trait_bound.is_some() {
            return None;
        }
        trait_bound = Some(strip_generic_args(text));
    }
    trait_bound
}

fn where_clause_of(scope: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = scope.walk();
    scope
        .children(&mut cursor)
        .find(|child| child.kind() == "where_clause")
}

fn add_bounds<'s>(owner: tree_sitter::Node<'_>, source: &'s [u8], acc: &mut Constraints<'s>) {
    if let Some(bounds) = owner.child_by_field_name("bounds") {
        acc.add(extract_single_bound(bounds, source));
    }
}

fn declared_parameter_bounds<'s>(
    scope: tree_sitter::Node<'_>,
    name: &str,
    source: &'s [u8],
    acc: &mut Constraints<'s>,
) -> bool {
    let Some(params) = scope.child_by_field_name("type_parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    let mut declared = false;
    for param in params.children(&mut cursor).filter(|param| {
        param.kind() == "type_parameter" && field_text(*param, "name", source) == Some(name)
    }) {
        declared = true;
        add_bounds(param, source, acc);
    }
    declared
}

/// Adds the bounds `scope` places on `name`; returns whether `scope` declares `name` as a type parameter.
pub(super) fn scope_constraints<'s>(
    scope: tree_sitter::Node<'_>,
    name: &str,
    source: &'s [u8],
    acc: &mut Constraints<'s>,
) -> bool {
    let declared = declared_parameter_bounds(scope, name, source, acc);
    if let Some(clause) = where_clause_of(scope) {
        let mut cursor = clause.walk();
        for pred in clause.children(&mut cursor).filter(|pred| {
            pred.kind() == "where_predicate" && field_text(*pred, "left", source) == Some(name)
        }) {
            add_bounds(pred, source, acc);
        }
    }
    declared
}

fn resolve_type_param(cx: &TypeCx<'_>, node: tree_sitter::Node<'_>, name: &str) -> ParamLookup {
    let mut constraints = Constraints::default();
    let mut curr = Some(node);
    while let Some(n) = curr {
        if matches!(
            n.kind(),
            "function_item" | "function_signature_item" | "impl_item" | "trait_item"
        ) && scope_constraints(n, name, cx.source, &mut constraints)
        {
            return match constraints.unique() {
                Some(bound) => ParamLookup::Bound(bound.to_string()),
                None => ParamLookup::Declared,
            };
        }
        curr = n.parent();
    }
    ParamLookup::Absent
}

fn name_repr(cx: &TypeCx<'_>, at: tree_sitter::Node<'_>, name: &str) -> Option<String> {
    match resolve_type_param(cx, at, name) {
        ParamLookup::Bound(bound) => Some(bound),
        ParamLookup::Declared => Some(name.to_string()),
        ParamLookup::Absent => match cx.aliases.get(name) {
            Some(resolved) => resolved.clone(),
            None => Some(name.to_string()),
        },
    }
}

/// The nominal name of an impl subject or bound, with references and generic arguments removed but no wrapper unwrapping.
pub(super) fn nominal_name<'s>(source: &'s [u8], ty: tree_sitter::Node<'_>) -> Option<&'s str> {
    match ty.kind() {
        "reference_type" | "generic_type" => nominal_name(source, ty.child_by_field_name("type")?),
        "type_identifier" | "scoped_type_identifier" => Some(ty.utf8_text(source).ok()?.trim()),
        _ => None,
    }
}

fn enclosing_container(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    crate::languages::reads::enclosing_scope(node, |kind| matches!(kind, "impl_item" | "trait_item"))
}

pub(super) fn enclosing_self_type(cx: &TypeCx<'_>, node: tree_sitter::Node<'_>) -> Option<String> {
    let container = enclosing_container(node)?;
    if container.kind() == "impl_item" {
        let subject = nominal_name(cx.source, container.child_by_field_name("type")?)?;
        return name_repr(cx, container, subject);
    }
    field_text(container, "name", cx.source).map(str::to_string)
}

pub(super) fn self_or_name<'a>(
    cx: &TypeCx<'_>,
    at: tree_sitter::Node<'_>,
    name: &'a str,
) -> Option<std::borrow::Cow<'a, str>> {
    if name == "Self" {
        enclosing_self_type(cx, at).map(std::borrow::Cow::Owned)
    } else {
        Some(std::borrow::Cow::Borrowed(name))
    }
}

fn first_type_argument(args: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if !matches!(child.kind(), "<" | ">" | "," | "lifetime") {
            return Some(child);
        }
    }
    None
}

/// Whether `path`, as written at `at`, denotes one of the std `paths` types. Qualified paths name std
/// directly; bare names resolve lexically, and an unshadowed bare name is std only when `prelude` admits it.
fn names_std(
    cx: &TypeCx<'_>,
    at: tree_sitter::Node<'_>,
    path: &str,
    paths: &[&str],
    prelude: fn(&str) -> bool,
) -> bool {
    let path = path.trim_start_matches("::");
    if path.contains("::") {
        return paths.contains(&path);
    }
    match cx.lexical.lookup(at, path) {
        Some(NameBinding::Import(target)) => paths.contains(&target.as_str()),
        Some(NameBinding::Local) => false,
        None => prelude(path),
    }
}

/// Whether `path`, as written at `at`, denotes the std `Box`, `Arc`, or `Rc`.
pub(super) fn names_std_wrapper(cx: &TypeCx<'_>, at: tree_sitter::Node<'_>, path: &str) -> bool {
    names_std(cx, at, path, WRAPPER_PATHS, |bare| bare == "Box")
}

/// Whether `path`, as written at `at`, denotes the std `Result` or `Option`, whose `Ok` or `Some` value
/// `unwrap`, `expect`, and `?` reach.
fn names_std_unwrap(cx: &TypeCx<'_>, at: tree_sitter::Node<'_>, path: &str) -> bool {
    names_std(cx, at, path, UNWRAP_PATHS, |bare| {
        matches!(bare, "Result" | "Option")
    })
}

fn generic_type_repr(cx: &TypeCx<'_>, ty: tree_sitter::Node<'_>) -> Option<String> {
    let outer = ty.child_by_field_name("type")?;
    let outer_text = outer.utf8_text(cx.source).ok()?.trim();
    if names_std_wrapper(cx, ty, outer_text)
        && let Some(inner) = ty
            .child_by_field_name("type_arguments")
            .and_then(first_type_argument)
    {
        return extract_type_repr(cx, inner);
    }
    if outer.kind() == "type_identifier" {
        name_repr(cx, ty, outer_text)
    } else {
        Some(outer_text.to_string())
    }
}

pub(super) fn extract_type_repr(cx: &TypeCx<'_>, ty: tree_sitter::Node<'_>) -> Option<String> {
    match ty.kind() {
        "reference_type" => extract_type_repr(cx, ty.child_by_field_name("type")?),
        "dynamic_type" | "abstract_type" => extract_type_repr(cx, ty.child_by_field_name("trait")?),
        "generic_type" => generic_type_repr(cx, ty),
        "type_identifier" => {
            let text = ty.utf8_text(cx.source).ok()?.trim();
            if text == "Self" {
                enclosing_self_type(cx, ty)
            } else {
                name_repr(cx, ty, text)
            }
        }
        "scoped_type_identifier" => {
            let text = ty.utf8_text(cx.source).ok()?.trim();
            let in_crate = matches!(text.split("::").next(), Some("crate" | "self" | "super"));
            let last = text.rsplit("::").next().unwrap_or(text);
            Some(if in_crate { last } else { text }.to_string())
        }
        _ => None,
    }
}

/// Strips generic arguments from a path so `Type::<T>::f` and `Type::f` name the same subject.
pub(super) fn bare_path(text: &str) -> &str {
    text.split('<')
        .next()
        .unwrap_or(text)
        .trim()
        .trim_end_matches("::")
}

pub(super) type Assoc<'s> = std::collections::HashMap<&'s str, Option<Returned>>;

/// What a call to an in-tree function yields: its declared type, and for a `Result<T, _>` or `Option<T>`
/// return the `T` that `unwrap`, `expect`, or `?` reach.
#[derive(Clone, PartialEq, Default)]
pub(super) struct Returned {
    pub(super) ty: Option<String>,
    pub(super) ok: Option<String>,
}

fn returned_of(cx: &TypeCx<'_>, ret: Option<tree_sitter::Node<'_>>) -> Returned {
    let Some(ty) = ret else {
        return Returned::default();
    };
    Returned {
        ty: extract_type_repr(cx, ty),
        ok: unwrapped_repr(cx, ty),
    }
}

/// The `T` of a `Result<T, _>` or `Option<T>` return type naming the std types.
fn unwrapped_repr(cx: &TypeCx<'_>, ty: tree_sitter::Node<'_>) -> Option<String> {
    if ty.kind() != "generic_type" {
        return None;
    }
    let outer = ty.child_by_field_name("type")?.utf8_text(cx.source).ok()?;
    if !names_std_unwrap(cx, ty, outer.trim()) {
        return None;
    }
    let inner = ty
        .child_by_field_name("type_arguments")
        .and_then(first_type_argument)?;
    extract_type_repr(cx, inner)
}

/// Associated function returns keyed by `(impl subject, fn name)`; conflicting declarations become `None`.
fn collect_assoc_returns<'s>(
    cx: &TypeCx<'s>,
    root: tree_sitter::Node<'_>,
) -> std::collections::HashMap<&'s str, Assoc<'s>> {
    let mut out = std::collections::HashMap::new();
    for imp in super::super::reads::nodes(root).filter(|node| node.kind() == "impl_item") {
        let Some(subject) = imp
            .child_by_field_name("type")
            .and_then(|ty| nominal_name(cx.source, ty))
        else {
            continue;
        };
        let Some(body) = imp.child_by_field_name("body") else {
            continue;
        };
        let mut cursor = body.walk();
        for item in body
            .children(&mut cursor)
            .filter(|child| child.kind() == "function_item")
        {
            let Some(name) = field_text(item, "name", cx.source) else {
                continue;
            };
            let fns: &mut Assoc<'s> = out.entry(subject).or_default();
            note_agreed(
                fns,
                name,
                Some(returned_of(cx, item.child_by_field_name("return_type"))),
                None,
            );
        }
    }
    out
}

/// Free function returns keyed by name (methods excluded); conflicting declarations become `None`.
fn collect_free_returns<'s>(
    cx: &TypeCx<'s>,
    root: tree_sitter::Node<'_>,
) -> std::collections::HashMap<&'s str, Option<Returned>> {
    let mut out = std::collections::HashMap::new();
    for item in super::super::reads::nodes(root).filter(|node| node.kind() == "function_item") {
        let in_impl = item
            .parent()
            .and_then(|body| body.parent())
            .is_some_and(|owner| matches!(owner.kind(), "impl_item" | "trait_item"));
        let Some(name) = field_text(item, "name", cx.source).filter(|_| !in_impl) else {
            continue;
        };
        note_agreed(
            &mut out,
            name,
            Some(returned_of(cx, item.child_by_field_name("return_type"))),
            None,
        );
    }
    out
}
