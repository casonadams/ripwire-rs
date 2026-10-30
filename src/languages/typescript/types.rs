//! Declared-type facts TypeScript exposes to call resolution: class `implements` records, typed
//! binding types (parameters, annotated or `new`-initialized declarators), object-literal member
//! types for member-chain receivers such as `input.queue` where `input: { queue: TaskEnqueuer }`,
//! class field types for `this.client.m()` chains, and same-file function result types.

use crate::languages::{Implementation, is_plain_type_name};
use crate::languages::javascript::{javascript_binding_scope, javascript_binding_scope_in};
use crate::languages::reads::{ReadBindings, nodes};
use crate::languages::typed::implementation;
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

/// Bound on how many same-file functions a return chain may pass through.
const RETURN_CHAIN_DEPTH: usize = 4;

/// Every class (with its own method span) and every `implements` name it declares.
pub(super) fn class_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    nodes(root)
        .filter(|node| matches!(node.kind(), "class_declaration" | "abstract_class_declaration"))
        .filter_map(|class| {
            let subject = class.child_by_field_name("name")?.utf8_text(source).ok()?;
            let span = (class.start_byte(), class.end_byte());
            let own = implementation(subject, None, span);
            let implemented = heritage_names(class, source, &["implements_clause", "extends_clause"])
                .into_iter()
                .map(|name| implementation(subject, Some(name), span));
            Some(std::iter::once(own).chain(implemented).collect::<Vec<_>>())
        })
        .flatten()
        .collect()
}

fn heritage_names<'s>(class: Node<'_>, source: &'s [u8], clauses: &[&str]) -> Vec<&'s str> {
    let mut cursor = class.walk();
    class
        .children(&mut cursor)
        .filter(|heritage| heritage.kind() == "class_heritage")
        .flat_map(|heritage| {
            let mut inner = heritage.walk();
            heritage
                .children(&mut inner)
                .filter(|clause| clauses.contains(&clause.kind()))
                .flat_map(|clause| {
                    let mut types = clause.walk();
                    clause
                        .named_children(&mut types)
                        .filter_map(|ty| match ty.kind() {
                            "type_identifier" | "identifier" => ty.utf8_text(source).ok(),
                            "generic_type" => ty
                                .child_by_field_name("name")
                                .and_then(|name| name.utf8_text(source).ok()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Read bindings with declared types: typed parameters, annotated declarators, `const x = new C()`,
/// `this` inside a class body, class field types, and same-file function result types.
pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::collect_with_ancestors(root, source, |node, ancestors| {
        javascript_binding_scope_in(node, ancestors).map(|scope| scope.byte_range())
    });
    for node in nodes(root) {
        if matches!(node.kind(), "class_declaration" | "abstract_class_declaration") {
            insert_this_scope(&mut bindings, node, source);
            continue;
        }
        if matches!(node.kind(), "function_declaration" | "function_expression") {
            // A nested function has its own `this`, which is not the enclosing class.
            bindings.insert("this", node.byte_range(), None);
            continue;
        }
        let (name, annotation, value) = match node.kind() {
            "required_parameter" | "optional_parameter" => (
                node.child_by_field_name("pattern"),
                node.child_by_field_name("type"),
                None,
            ),
            "variable_declarator" => (
                node.child_by_field_name("name"),
                node.child_by_field_name("type"),
                node.child_by_field_name("value"),
            ),
            _ => continue,
        };
        let Some(name) = name.filter(|name| name.kind() == "identifier") else {
            continue;
        };
        let ty = annotation_text(annotation, source)
            .or_else(|| value.and_then(|value| constructed_type(value, source)));
        let (Some(ty), Ok(text), Some(scope)) = (
            ty,
            name.utf8_text(source),
            javascript_binding_scope(name).map(|scope| scope.byte_range()),
        ) else {
            continue;
        };
        bindings.insert(text, scope, Some(ty));
    }
    record_class_fields(&mut bindings, root, source);
    record_return_types(&mut bindings, root, source);
    bindings
}

fn insert_this_scope<'a>(bindings: &mut ReadBindings<'a>, class: Node<'_>, source: &'a [u8]) {
    if let Some(name) = class
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
    {
        bindings.insert("this", class.byte_range(), Some(name.to_string()));
    }
    if let Some(base) = heritage_names(class, source, &["extends_clause"]).first() {
        bindings.insert("super", class.byte_range(), Some((*base).to_string()));
    }
}

/// Field facts gathered per `(class, field)`: `None` marks fields given two different types.
type FieldFacts = HashMap<(String, String), Option<String>>;

fn record_fact(facts: &mut FieldFacts, owner: &str, field: &str, ty: String) {
    facts
        .entry((owner.to_string(), field.to_string()))
        .and_modify(|slot| {
            if slot.as_deref() != Some(ty.as_str()) {
                *slot = None;
            }
        })
        .or_insert(Some(ty));
}

/// Class field types: an explicit annotation (field or constructor parameter property) wins over
/// an initializer; `new C()` initializers and `this.f = new C()` in a constructor type the field
/// only when no annotation does. Fields whose facts disagree stay untyped.
fn record_class_fields(bindings: &mut ReadBindings<'_>, root: Node<'_>, source: &[u8]) {
    let mut explicit = FieldFacts::new();
    let mut constructed = FieldFacts::new();
    for class in nodes(root)
        .filter(|n| matches!(n.kind(), "class_declaration" | "abstract_class_declaration"))
    {
        let (Some(owner), Some(body)) = (
            class
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok()),
            class.child_by_field_name("body"),
        ) else {
            continue;
        };
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            record_member_fields(member, owner, source, &mut explicit, &mut constructed);
        }
    }
    insert_field_types(bindings, &explicit, &constructed);
}

fn record_member_fields(
    member: Node<'_>,
    owner: &str,
    source: &[u8],
    explicit: &mut FieldFacts,
    constructed: &mut FieldFacts,
) {
    match member.kind() {
        "public_field_definition" => {
            let Some(field) = member
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
            else {
                return;
            };
            if let Some(ty) = annotation_text(member.child_by_field_name("type"), source) {
                record_fact(explicit, owner, field, ty);
            } else if let Some(ty) = member
                .child_by_field_name("value")
                .and_then(|value| constructed_type(value, source))
            {
                record_fact(constructed, owner, field, ty);
            }
        }
        "method_definition" => {
            let is_constructor = member
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some("constructor");
            if is_constructor {
                record_constructor_fields(member, owner, source, explicit, constructed);
            }
        }
        _ => {}
    }
}

fn insert_field_types(
    bindings: &mut ReadBindings<'_>,
    explicit: &FieldFacts,
    constructed: &FieldFacts,
) {
    for ((owner, field), ty) in explicit {
        if let Some(ty) = ty {
            bindings.insert_member(owner, field, ty.clone());
        }
    }
    for ((owner, field), ty) in constructed {
        if explicit.contains_key(&(owner.clone(), field.clone())) {
            continue;
        }
        if let Some(ty) = ty {
            bindings.insert_member(owner, field, ty.clone());
        }
    }
}

/// Parameter properties (`private client: C`) are annotated fields; `this.f = new C()` in the
/// constructor body is an initializer fact for `f`.
fn record_constructor_fields(
    constructor: Node<'_>,
    owner: &str,
    source: &[u8],
    explicit: &mut FieldFacts,
    constructed: &mut FieldFacts,
) {
    if let Some(params) = constructor.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if !matches!(param.kind(), "required_parameter" | "optional_parameter")
                || !is_parameter_property(param)
            {
                continue;
            }
            let field = param
                .child_by_field_name("pattern")
                .filter(|pattern| pattern.kind() == "identifier")
                .and_then(|pattern| pattern.utf8_text(source).ok());
            if let (Some(field), Some(ty)) =
                (field, annotation_text(param.child_by_field_name("type"), source))
            {
                record_fact(explicit, owner, field, ty);
            }
        }
    }
    let Some(body) = constructor.child_by_field_name("body") else {
        return;
    };
    for assign in nodes(body).filter(|n| n.kind() == "assignment_expression") {
        let (Some(left), Some(right)) = (
            assign.child_by_field_name("left"),
            assign.child_by_field_name("right"),
        ) else {
            continue;
        };
        let is_this_field = left.kind() == "member_expression"
            && left
                .child_by_field_name("object")
                .is_some_and(|object| object.kind() == "this");
        let field = left
            .child_by_field_name("property")
            .and_then(|p| p.utf8_text(source).ok());
        if let (true, Some(field), Some(ty)) =
            (is_this_field, field, constructed_type(right, source))
        {
            record_fact(constructed, owner, field, ty);
        }
    }
}

/// True when a constructor parameter carries an accessibility modifier or `readonly`.
fn is_parameter_property(param: Node<'_>) -> bool {
    let mut cursor = param.walk();
    param
        .children(&mut cursor)
        .any(|child| child.kind() == "accessibility_modifier" || child.kind() == "readonly")
}

fn annotation_text(annotation: Option<Node<'_>>, source: &[u8]) -> Option<String> {
    annotation?
        .utf8_text(source)
        .ok()
        .map(|text| text.trim_start_matches(':').trim().to_string())
}

/// A module-level function or `const` arrow function: the node whose body yields its results.
struct Callable<'t> {
    node: Node<'t>,
    body: Node<'t>,
    expression_body: bool,
}

impl Callable<'_> {
    /// Async and generator functions do not return the value their body names directly.
    fn synchronous(&self) -> bool {
        let mut cursor = self.node.walk();
        self.node
            .children(&mut cursor)
            .all(|child| !matches!(child.kind(), "async" | "*"))
    }

    fn annotation(&self, source: &[u8]) -> Option<String> {
        annotation_text(self.node.child_by_field_name("return_type"), source)
    }
}

fn is_module_level(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| matches!(parent.kind(), "program" | "export_statement"))
}

/// The module-level function `node` declares, or the module-level `const` arrow it binds.
fn module_callable<'a, 't>(node: Node<'t>, source: &'a [u8]) -> Option<(&'a str, Callable<'t>)> {
    match node.kind() {
        "function_declaration" if is_module_level(node) => {
            let name = node.child_by_field_name("name")?.utf8_text(source).ok()?;
            let body = node.child_by_field_name("body")?;
            Some((
                name,
                Callable {
                    node,
                    body,
                    expression_body: false,
                },
            ))
        }
        "arrow_function" => {
            let declarator = node.parent().filter(|p| p.kind() == "variable_declarator")?;
            let lexical = declarator.parent()?;
            let is_const = lexical.kind() == "lexical_declaration"
                && lexical.child(0).is_some_and(|kw| kw.kind() == "const");
            if !is_const || !is_module_level(lexical) {
                return None;
            }
            let name = declarator.child_by_field_name("name")?;
            if name.kind() != "identifier" {
                return None;
            }
            let body = node.child_by_field_name("body")?;
            Some((
                name.utf8_text(source).ok()?,
                Callable {
                    node,
                    body,
                    expression_body: body.kind() != "statement_block",
                },
            ))
        }
        _ => None,
    }
}

/// Records each same-file function's result type for calls `f()`: an annotation as written, or a
/// type inferred from the body. Overloaded or duplicated names record nothing.
fn record_return_types<'a>(bindings: &mut ReadBindings<'a>, root: Node<'_>, source: &'a [u8]) {
    let mut functions: HashMap<&'a str, Vec<Callable<'_>>> = HashMap::new();
    for node in nodes(root) {
        if let Some((name, callable)) = module_callable(node, source) {
            functions.entry(name).or_default().push(callable);
        }
    }
    let known = local_class_names(root, source);
    let names: Vec<&'a str> = functions.keys().copied().collect();
    for name in names {
        let [callable] = functions[name].as_slice() else {
            continue;
        };
        if let Some(ann) = callable.annotation(source) {
            bindings.insert_return(name, ann);
            continue;
        }
        let returns = ReturnTypes {
            functions: &functions,
            known: &known,
            source,
        };
        if let Some(ty) = returns.result(name, RETURN_CHAIN_DEPTH, &mut Vec::new()) {
            bindings.insert_return(name, ty);
        }
    }
}

/// Class names this file can name directly: classes it declares and default or named imports from
/// a relative module. A `new` of any other name is not a type the index is known to hold.
fn local_class_names<'a>(root: Node<'_>, source: &'a [u8]) -> HashSet<&'a str> {
    let mut names = HashSet::new();
    for node in nodes(root) {
        match node.kind() {
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    names.extend(name.utf8_text(source).ok());
                }
            }
            "import_statement" => {
                let relative = node
                    .child_by_field_name("source")
                    .and_then(|s| s.utf8_text(source).ok())
                    .is_some_and(|s| {
                        let inner = s.trim_matches(|c| c == '"' || c == '\'');
                        inner.starts_with("./") || inner.starts_with("../")
                    });
                if relative {
                    names.extend(import_local_names(node, source));
                }
            }
            _ => {}
        }
    }
    names
}

/// Local names bound by an import's default and named specifiers (`A as B` binds `B`).
fn import_local_names<'a>(import: Node<'_>, source: &'a [u8]) -> Vec<&'a str> {
    nodes(import)
        .filter_map(|node| match node.kind() {
            "import_specifier" => node
                .child_by_field_name("alias")
                .or_else(|| node.child_by_field_name("name")),
            "import_clause" => {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .find(|child| child.kind() == "identifier")
            }
            _ => None,
        })
        .filter_map(|node| node.utf8_text(source).ok())
        .collect()
}

struct ReturnTypes<'a, 'f, 't> {
    functions: &'f HashMap<&'a str, Vec<Callable<'t>>>,
    known: &'f HashSet<&'a str>,
    source: &'a [u8],
}

impl ReturnTypes<'_, '_, '_> {
    /// The result type of same-file function `name`: a plain annotation, or an inferred class.
    /// Anything else, including recursion past `depth` or a cycle, is unknown.
    fn result(&self, name: &str, depth: usize, stack: &mut Vec<String>) -> Option<String> {
        let [callable] = self.functions.get(name)?.as_slice() else {
            return None;
        };
        if let Some(ann) = callable.annotation(self.source) {
            return is_plain_type_name(&ann).then_some(ann);
        }
        if depth == 0 || !callable.synchronous() || stack.iter().any(|seen| seen.as_str() == name)
        {
            return None;
        }
        let exprs = returned_expressions(callable)?;
        stack.push(name.to_string());
        let mut found: Option<String> = None;
        for expr in exprs {
            let ty = self.expression_type(expr, depth - 1, stack)?;
            if found.as_ref().is_some_and(|prev| *prev != ty) {
                return None;
            }
            found = Some(ty);
        }
        stack.pop();
        found
    }

    /// The class a returned expression produces: `new C(...)` of a known class, or a call to a
    /// same-file function whose result is itself known.
    fn expression_type(
        &self,
        expr: Node<'_>,
        depth: usize,
        stack: &mut Vec<String>,
    ) -> Option<String> {
        match expr.kind() {
            "new_expression" => {
                constructed_type(expr, self.source).filter(|ty| self.known.contains(ty.as_str()))
            }
            "call_expression" => {
                let callee = expr
                    .child_by_field_name("function")
                    .filter(|f| f.kind() == "identifier")?;
                let name = callee.utf8_text(self.source).ok()?;
                self.result(name, depth, stack)
            }
            _ => None,
        }
    }
}

/// Every value a function returns, or `None` when some `return` names no value.
fn returned_expressions<'t>(callable: &Callable<'t>) -> Option<Vec<Node<'t>>> {
    if callable.expression_body {
        return Some(vec![callable.body]);
    }
    let mut out = Vec::new();
    let mut cursor = callable.body.walk();
    for child in callable.body.children(&mut cursor) {
        collect_returns(child, &mut out)?;
    }
    (!out.is_empty()).then_some(out)
}

/// Collects the returned values under `node`, skipping nested functions and classes, whose
/// `return` statements belong to them.
fn collect_returns<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) -> Option<()> {
    match node.kind() {
        "function_declaration"
        | "function_expression"
        | "arrow_function"
        | "generator_function_declaration"
        | "method_definition"
        | "class_declaration"
        | "class" => return Some(()),
        "return_statement" => {
            out.push(node.named_child(0)?);
            return Some(());
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_returns(child, out)?;
    }
    Some(())
}

/// The class named by `new C(...)`, which is the declared type of the value it initializes.
fn constructed_type(value: Node<'_>, source: &[u8]) -> Option<String> {
    (value.kind() == "new_expression")
        .then(|| value.child_by_field_name("constructor"))
        .flatten()
        .filter(|ctor| ctor.kind() == "identifier")
        .and_then(|ctor| ctor.utf8_text(source).ok())
        .map(str::to_string)
}

/// Declared type of member `member` of an object-literal type `{ member: T; … }`, as written.
pub(super) fn object_member_type(receiver_type: &str, member: &str) -> Option<String> {
    let body = receiver_type
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?;
    top_level_members(body)
        .into_iter()
        .find_map(|part| {
            let (key, value) = part.split_once(':')?;
            (key.trim().trim_end_matches('?').trim() == member).then(|| value.trim().to_string())
        })
}

/// Splits an object type body at `;` and `,` outside nested brackets.
fn top_level_members(body: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, byte) in body.bytes().enumerate() {
        match byte {
            b'{' | b'(' | b'[' | b'<' => depth += 1,
            b'}' | b')' | b']' | b'>' => depth -= 1,
            b';' | b',' if depth == 0 => {
                parts.push(&body[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

/// A function type that is the declared type of a property of an interface body (`f: () => T;`).
pub(super) fn is_interface_function_property(node: Node<'_>) -> bool {
    node.kind() == "function_type"
        && node
            .parent()
            .filter(|ann| ann.kind() == "type_annotation")
            .and_then(|ann| ann.parent())
            .filter(|prop| prop.kind() == "property_signature")
            .and_then(|prop| prop.parent())
            .is_some_and(|body| body.kind() == "interface_body")
}
