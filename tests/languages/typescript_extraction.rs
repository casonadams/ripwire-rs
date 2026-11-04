use crate::support::index::with_typescript_module;
use ripwire_rs::SymKind;

fn symbols_of(source: &str) -> Vec<(String, SymKind)> {
    with_typescript_module(source, |ingest, interner| {
        ingest
            .symbols
            .iter()
            .map(|sym| (interner.resolve(&sym.name).to_string(), sym.kind))
            .collect()
    })
}

#[test]
fn fluent_builder_handler_is_a_module_function() {
    let source = "declare function createServerFn(o: object): any;\n\
        export const resumeSession = createServerFn({ method: \"POST\" })\n\
          .handler(async ({ data }) => data);\n\
        export const getThing = createServerFn({ method: \"GET\" }).handler(function () {\n\
          return 1;\n\
        });\n\
        export const chained = createServerFn({ method: \"PUT\" })\n\
          .inputValidator((d: unknown) => d)\n\
          .handler(async ({ data }) => data);\n";
    let symbols = symbols_of(source);
    assert!(symbols.contains(&("resumeSession".to_string(), SymKind::Function)));
    assert!(symbols.contains(&("getThing".to_string(), SymKind::Function)));
    assert!(symbols.contains(&("chained".to_string(), SymKind::Function)));
}

#[test]
fn method_chains_over_data_are_not_definitions() {
    let source = "export const doubled = [1, 2].filter((n) => n > 1).map((n) => n * 2);\n\
        const promised = fetchish(1).then((r) => r);\n\
        export function run(): void {\n\
          const local = makeBuilder().handler(() => 1);\n\
          void local;\n\
        }\n";
    let symbols = symbols_of(source);
    let names: Vec<&str> = symbols.iter().map(|(name, _)| name.as_str()).collect();
    assert!(!names.contains(&"doubled"), "{names:?}");
    assert!(!names.contains(&"promised"), "{names:?}");
    assert!(!names.contains(&"local"), "{names:?}");
}

#[test]
fn module_let_is_a_variable_definition() {
    let symbols = symbols_of("let depth = 0;\nexport let label = 'a';\n");
    assert!(symbols.contains(&("depth".to_string(), SymKind::Var)));
    assert!(symbols.contains(&("label".to_string(), SymKind::Var)));
}

fn signature_of(source: &str, name: &str) -> String {
    with_typescript_module(source, |ingest, interner| {
        let sym = ingest
            .symbols
            .iter()
            .find(|sym| interner.resolve(&sym.name) == name)
            .expect("symbol");
        sym.extract_signature(source.as_bytes(), name)
    })
}

#[test]
fn multiline_return_type_keeps_nested_object_and_generics() {
    let source = "export async function loadThing(id: string): Promise<\n  { id: string; items: Array<{ key: string }> }\n> {\n  return { id, items: [] };\n}\n";
    assert_eq!(
        signature_of(source, "loadThing"),
        "export async function loadThing(id: string): Promise< { id: string; items: Array<{ key: string }> } >"
    );
}

#[test]
fn same_line_body_is_not_part_of_the_signature() {
    let source =
        "export function single(a: number): Promise<number> { return Promise.resolve(a); }\n";
    assert_eq!(
        signature_of(source, "single"),
        "export function single(a: number): Promise<number>"
    );
}

#[test]
fn object_literal_return_type_ends_at_the_body() {
    let source = "export function point(a: number): { x: number; y: number } {\n  return { x: a, y: a };\n}\n";
    assert_eq!(
        signature_of(source, "point"),
        "export function point(a: number): { x: number; y: number }"
    );
}

fn has_body_of(source: &str, name: &str) -> bool {
    with_typescript_module(source, |ingest, interner| {
        let found: Vec<bool> = ingest
            .symbols
            .iter()
            .filter(|sym| interner.resolve(&sym.name) == name)
            .map(|sym| sym.has_body())
            .collect();
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    })
}

#[test]
fn fluent_builder_declaration_has_its_handler_callback_as_body() {
    let source = "declare function createServerFn(o: object): any;\n\
        export const resumeSession = createServerFn({ method: \"POST\" })\n\
          .inputValidator((d: unknown) => d)\n\
          .handler(async ({ data }) => {\n\
            return reconcileLedger(data);\n\
          });\n";
    assert!(has_body_of(source, "resumeSession"));
}

#[test]
fn unrelated_bodyless_declaration_keeps_the_stub_shape() {
    let source = "declare function createServerFn(o: object): any;\n\
        export declare function resolveRoute(path: string): string;\n";
    assert!(!has_body_of(source, "resolveRoute"));
}

#[test]
fn chain_whose_last_call_is_not_a_handler_takes_no_callback_body() {
    let source = "declare function createServerFn(o: object): any;\n\
        export const notAHandler = createServerFn({ method: \"GET\" })\n\
          .handler(async () => {\n\
            return reconcileLedger();\n\
          })\n\
          .finish(1);\n";
    let names: Vec<String> = symbols_of(source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(!names.contains(&"notAHandler".to_string()), "{names:?}");
}
