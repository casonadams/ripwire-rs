use crate::languages::reads::{ReadBindings, named_children_of_kind, nodes};
use crate::languages::typed::AgreedReturns;
use crate::languages::{Implementation, is_plain_type_name};
use tree_sitter::Node;

pub(super) fn class_implementations(root: Node<'_>, source: &[u8]) -> Vec<Implementation> {
    let mut implementations = crate::languages::typed::class_implementations(
        root,
        source,
        &["class_definition", "mixin_declaration"],
        dart_class_name,
        dart_bases,
    );
    let superclasses: Vec<(String, String)> = nodes(root)
        .filter(|n| n.kind() == "class_definition")
        .filter_map(|class| {
            let subject = dart_class_name(class, source)?;
            let superclass = class
                .child_by_field_name("superclass")
                .and_then(|clause| declared_type(clause, source))?;
            Some((subject.to_string(), superclass))
        })
        .collect();
    for imp in &mut implementations {
        imp.extends = imp.trait_path.as_ref().is_some_and(|path| {
            superclasses
                .iter()
                .any(|(subject, superclass)| *subject == imp.subject && superclass == path)
        });
    }
    implementations
}

/// A class's name is its `name` field; a mixin names itself with a bare identifier.
fn dart_class_name<'s>(class: Node<'_>, source: &'s [u8]) -> Option<&'s str> {
    let name = class.child_by_field_name("name").or_else(|| {
        let mut cursor = class.walk();
        class
            .named_children(&mut cursor)
            .find(|child| child.kind() == "identifier")
    })?;
    name.utf8_text(source).ok()
}

/// Dart names the superclass and the interfaces as `type_identifier`s under those fields.
fn dart_bases<'s>(class: Node<'_>, source: &'s [u8]) -> Vec<&'s str> {
    let direct = ["superclass", "interfaces"]
        .into_iter()
        .filter_map(|field| class.child_by_field_name(field))
        .flat_map(|clause| named_children_of_kind(clause, "type_identifier"));
    // `extends B with M` nests the mixins under the superclass clause (or beside it, by grammar version).
    let mixins = class
        .child_by_field_name("superclass")
        .into_iter()
        .chain(std::iter::once(class))
        .flat_map(|parent| named_children_of_kind(parent, "mixins"))
        .flat_map(|list| named_children_of_kind(list, "type_identifier"));
    direct
        .chain(mixins)
        .filter_map(|ty| ty.utf8_text(source).ok())
        .collect()
}

pub(super) fn typed_read_bindings<'a>(root: Node<'_>, source: &'a [u8]) -> ReadBindings<'a> {
    let mut bindings = ReadBindings::default();
    let mut returns = AgreedReturns::default();
    for class in nodes(root).filter(|n| n.kind() == "class_definition") {
        if let Some((name, body)) = class_name_body(class, source) {
            record_method_returns(&mut bindings, name, body, source);
        }
    }
    for node in nodes(root) {
        match node.kind() {
            "class_definition" => record_class(&mut bindings, node, source),
            "formal_parameter" => record_parameter(&mut bindings, node, source),
            "initialized_variable_definition" => record_local(&mut bindings, node, source),
            "function_signature" => record_return(&mut returns, node, source),
            _ => {}
        }
    }
    returns.insert_into(&mut bindings);
    bindings
}

fn record_return<'a>(returns: &mut AgreedReturns<'a>, signature: Node<'_>, source: &'a [u8]) {
    let Some(name) = signature
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return;
    };
    let ty = return_type(signature, source);
    returns.note(name, ty);
}

/// The declared return type. A generic return keeps its base class unless the base is a std
/// collection or future, which stays untyped (`XmlNodeList<XmlNode>` is `XmlNodeList`).
fn return_type(signature: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = signature.walk();
    let generic = signature
        .children(&mut cursor)
        .any(|child| child.kind() == "type_arguments");
    declared_type(signature, source)
        .filter(|ty| !generic || !super::DART_STD_QUALIFIERS.contains(&ty.as_str()))
}

fn record_class<'a>(bindings: &mut ReadBindings<'a>, class: Node<'_>, source: &'a [u8]) {
    let Some((name, body)) = class_name_body(class, source) else {
        return;
    };
    bindings.insert("this", body.byte_range(), Some(name.to_string()));
    if let Some(base) = class
        .child_by_field_name("superclass")
        .and_then(|clause| declared_type(clause, source))
    {
        bindings.insert("super", body.byte_range(), Some(base));
    }
    let mut cursor = body.walk();
    for declaration in body
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "declaration")
    {
        let Some(ty) = declared_type(declaration, source) else {
            continue;
        };
        for field in field_names(declaration, source) {
            bindings.insert(field, body.byte_range(), Some(ty.clone()));
            bindings.insert_member(name, field, ty.clone());
            if let Some(element) = declared_element(declaration, source) {
                bindings.insert_element(field, body.byte_range(), element);
            }
        }
    }
    for getter in body
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "method_signature")
        .filter_map(|method| method.named_child(0))
        .filter(|signature| signature.kind() == "getter_signature")
    {
        let mut inner = getter.walk();
        let member = getter
            .named_children(&mut inner)
            .find(|child| child.kind() == "identifier")
            .and_then(|child| child.utf8_text(source).ok());
        if let (Some(member), Some(ty)) = (member, return_type(getter, source)) {
            bindings.insert_member(name, member, ty);
        }
    }
}

/// Each method of a class is a member typed by its declared return, so `d.holder()` reaches its class.
fn record_method_returns<'a>(
    bindings: &mut ReadBindings<'a>,
    owner: &'a str,
    body: Node<'_>,
    source: &'a [u8],
) {
    let mut cursor = body.walk();
    for method in body
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "method_signature")
    {
        let Some(signature) = method
            .named_child(0)
            .filter(|s| s.kind() == "function_signature")
        else {
            continue;
        };
        let member = signature
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok());
        if let (Some(member), Some(ty)) = (member, return_type(signature, source)) {
            bindings.insert_member(owner, &format!("{member}()"), ty);
        }
        if let (Some(member), Some(element)) = (member, declared_element(signature, source)) {
            bindings.insert_member_element(owner, &format!("{member}()"), element);
        }
    }
 }

fn field_names<'a>(declaration: Node<'_>, source: &'a [u8]) -> Vec<&'a str> {
    let mut cursor = declaration.walk();
    declaration
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "initialized_identifier_list")
        .flat_map(|list| {
            let mut inner = list.walk();
            list.named_children(&mut inner)
                .filter_map(|item| item.named_child(0))
                .filter(|ident| ident.kind() == "identifier")
                .filter_map(|ident| ident.utf8_text(source).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A parameter's name: its `name` field, or the identifier that ends a parameter without a declared
/// type (`k` in the lambda parameter `(k)`).
fn parameter_name<'a>(param: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = param.walk();
    let identifier = param
        .child_by_field_name("name")
        .filter(|n| n.kind() == "identifier")
        .or_else(|| {
            param
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "identifier")
                .last()
        })?;
    identifier.utf8_text(source).ok()
}

fn record_parameter<'a>(bindings: &mut ReadBindings<'a>, param: Node<'_>, source: &'a [u8]) {
    let Some(name) = parameter_name(param, source) else {
        return;
    };
    let Some(scope) = parameter_scope(param) else {
        return;
    };
    let ty = declared_type(param, source).or_else(|| lambda_element(param, bindings, source));
    bindings.insert(name, scope.clone(), ty);
    if let Some(element) = declared_element(param, source) {
        bindings.insert_element(name, scope, element);
    }
}

fn parameter_scope(param: Node<'_>) -> Option<std::ops::Range<usize>> {
    let mut owner = param.parent()?;
    while matches!(
        owner.kind(),
        "formal_parameter_list" | "optional_formal_parameters"
    ) {
        owner = owner.parent()?;
    }
    if owner.kind() == "function_expression" {
        return Some(owner.byte_range());
    }
    let holder = match owner.parent() {
        Some(parent) if parent.kind() == "method_signature" => parent,
        _ => owner,
    };
    let body = super::dart_following_body(holder)?;
    Some(holder.start_byte()..body.end_byte())
}

fn record_local<'a>(bindings: &mut ReadBindings<'a>, definition: Node<'_>, source: &'a [u8]) {
    let Some(name) = definition
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return;
    };
    let Some(block) = crate::languages::reads::enclosing_scope(definition, |kind| kind == "block")
    else {
        return;
    };
    let ty = declared_type(definition, source).or_else(|| constructed_type(definition, source));
    bindings.insert(name, definition.start_byte()..block.end_byte(), ty);
    if let Some(element) = declared_element(definition, source) {
        bindings.insert_element(name, definition.start_byte()..block.end_byte(), element);
    }
}

fn declared_type(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let ty = node
        .children(&mut cursor)
        .find(|child| child.kind() == "type_identifier")?
        .utf8_text(source)
        .ok()?;
    is_plain_type_name(ty).then(|| ty.to_string())
}

fn constructed_type(definition: Node<'_>, source: &[u8]) -> Option<String> {
    let value = definition.child_by_field_name("value")?;
    let call = value.next_named_sibling()?;
    let name = value.utf8_text(source).ok()?;
    (value.kind() == "identifier"
        && call.kind() == "selector"
        && call.utf8_text(source).ok()?.starts_with('(')
        && name.starts_with(char::is_uppercase)
        && is_plain_type_name(name))
    .then(|| name.to_string())
}

/// The element type of a collection declared over one plain, non-std type (`List<Kids> xs` is `Kids`).
fn declared_element(node: Node<'_>, source: &[u8]) -> Option<String> {
    if !matches!(declared_type(node, source)?.as_str(), "List" | "Set" | "Iterable") {
        return None;
    }
    let mut cursor = node.walk();
    let arguments = node
        .children(&mut cursor)
        .find(|child| child.kind() == "type_arguments")?;
    let mut inner = arguments.walk();
    let mut types = arguments.named_children(&mut inner);
    let element = types.next()?.utf8_text(source).ok()?.trim();
    if types.next().is_some() || !is_plain_type_name(element) {
        return None;
    }
    (!super::DART_STD_QUALIFIERS.contains(&element)).then(|| element.to_string())
}

/// Member calls that hand each element of the receiver to their first (only) lambda parameter.
const ELEMENT_CALLBACKS: &[&str] = &[
    "forEach",
    "map",
    "where",
    "any",
    "every",
    "firstWhere",
    "lastWhere",
    "singleWhere",
    "takeWhile",
    "skipWhile",
    "removeWhere",
    "retainWhere",
    "expand",
];

/// The element type a lambda's first parameter takes: `xs.forEach((c) => ...)` with `xs` a collection of
/// `Kids` types `c` as `Kids`. The lambda must be the argument of an element callback on a bound identifier.
fn lambda_element(param: Node<'_>, bindings: &ReadBindings<'_>, source: &[u8]) -> Option<String> {
    let mut function = param.parent()?;
    while matches!(
        function.kind(),
        "formal_parameter_list" | "optional_formal_parameters"
    ) {
        function = function.parent()?;
    }
    if function.kind() != "function_expression" || param.prev_named_sibling().is_some() {
        return None;
    }
    let mut call = function.parent()?;
    while call.kind() != "argument_part" {
        if !matches!(call.kind(), "argument" | "arguments") {
            return None;
        }
        call = call.parent()?;
    }
    let method = call.parent()?.prev_named_sibling()?;
    let member = method.named_child(0)?;
    let callback = member.named_child(0)?.utf8_text(source).ok()?;
    if member.kind() != "unconditional_assignable_selector"
        || !ELEMENT_CALLBACKS.contains(&callback)
    {
        return None;
    }
    let receiver = method.prev_named_sibling()?;
    match receiver.kind() {
        "identifier" => {
            let name = receiver.utf8_text(source).ok()?;
            bindings
                .element_at(name, param.start_byte())
                .map(str::to_string)
        }
        "selector" => call_element(receiver, param.start_byte(), bindings, source),
        _ => None,
    }
}

/// The element type a call's result holds: `m(..)` on the enclosing class, or `x.m(..)` on the type `x` is
/// bound to, when `m` declares a collection of one plain type (`Iterable<Kid> m()` holds `Kid`).
fn call_element(
    selector: Node<'_>,
    byte: usize,
    bindings: &ReadBindings<'_>,
    source: &[u8],
) -> Option<String> {
    if selector.named_child(0)?.kind() != "argument_part" {
        return None;
    }
    let callee = selector.prev_named_sibling()?;
    let (owner, member) = match callee.kind() {
        "identifier" => (bindings.type_at("this", byte)?, callee),
        "selector" => {
            let access = callee.named_child(0)?;
            let base = callee.prev_named_sibling()?;
            if access.kind() != "unconditional_assignable_selector" || base.kind() != "identifier" {
                return None;
            }
            let base_name = base.utf8_text(source).ok()?;
            (bindings.type_at(base_name, byte)?, access.named_child(0)?)
        }
        _ => return None,
    };
    let member = member.utf8_text(source).ok()?;
    bindings
        .member_element(owner, &format!("{member}()"))
        .map(str::to_string)
}

/// A class's name and body, when it has both.
fn class_name_body<'t, 's>(class: Node<'t>, source: &'s [u8]) -> Option<(&'s str, Node<'t>)> {
    let name = class.child_by_field_name("name")?.utf8_text(source).ok()?;
    Some((name, class.child_by_field_name("body")?))
}
