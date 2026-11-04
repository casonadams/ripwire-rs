use crate::support::index::with_typescript_module;
use ripwire_rs::query::edit::{EditOp, splice_file_bytes};
use ripwire_rs::{Interner, Symbol};

const SOURCE: &str = "/** Doc for make. */\nexport const make = (x: number): number => {\n  return x + 1;\n};\n\n// plain note\nconst helper = (): number => 2;\n\n/**\n * Class doc.\n */\nexport class Box {\n  /** Method doc. */\n  open(): void {\n    helper();\n  }\n  close(): void {}\n}\n\nexport function plain(): void {}\n\n/** Detached doc. */\n\nfunction gapped(): void {}\n";

fn with_symbols<T>(source: &str, f: impl FnOnce(&[Symbol], &Interner) -> T) -> T {
    with_typescript_module(source, |ingest, interner| f(&ingest.symbols, interner))
}

/// The definition text (doc, export and body included) of every symbol named `name`, in source order.
fn definitions(source: &str, name: &str) -> Vec<String> {
    with_symbols(source, |symbols, interner| {
        symbols
            .iter()
            .filter(|sym| interner.resolve(&sym.name) == name)
            .filter_map(|sym| sym.source_text(source.as_bytes()).map(|t| t.into_owned()))
            .collect()
    })
}

fn only(source: &str, name: &str) -> String {
    let found = definitions(source, name);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found.into_iter().next().unwrap()
}

#[test]
fn exported_arrow_const_keeps_export_and_jsdoc() {
    assert_eq!(
        only(SOURCE, "make"),
        "/** Doc for make. */\nexport const make = (x: number): number => {\n  return x + 1;\n};"
    );
}

#[test]
fn non_exported_const_without_doc_is_exact_and_ignores_line_comments() {
    assert_eq!(only(SOURCE, "helper"), "const helper = (): number => 2;");
}

#[test]
fn exported_class_keeps_export_and_block_doc() {
    let text = only(SOURCE, "Box");
    assert!(
        text.starts_with("/**\n * Class doc.\n */\nexport class Box {"),
        "{text}"
    );
    assert!(text.ends_with("close(): void {}\n}"), "{text}");
}

#[test]
fn method_keeps_its_own_jsdoc_only() {
    assert_eq!(
        only(SOURCE, "open"),
        "/** Method doc. */\n  open(): void {\n    helper();\n  }"
    );
}

#[test]
fn exported_function_without_doc_starts_at_export() {
    assert_eq!(only(SOURCE, "plain"), "export function plain(): void {}");
}

#[test]
fn doc_separated_by_a_blank_line_is_not_attached() {
    assert_eq!(only(SOURCE, "gapped"), "function gapped(): void {}");
}

#[test]
fn signature_keeps_export_and_drops_doc() {
    with_symbols(SOURCE, |symbols, interner| {
        let make = symbols
            .iter()
            .find(|sym| interner.resolve(&sym.name) == "make")
            .expect("make");
        let sig = make.extract_signature(SOURCE.as_bytes(), "make");
        assert!(sig.starts_with("export const make"), "{sig}");
        assert!(!sig.contains("Doc for make"), "{sig}");
    });
}

#[test]
fn overloads_each_keep_their_own_declaration_text() {
    let source = "export function pick(a: string): string;\nexport function pick(a: number): number;\nexport function pick(a: string | number): string | number {\n  return a;\n}\n";
    let texts = definitions(source, "pick");
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert_eq!(texts[0], "export function pick(a: string): string;");
    assert_eq!(texts[1], "export function pick(a: number): number;");
    assert!(texts[2].starts_with("export function pick(a: string | number)"));
    assert!(texts[2].ends_with("return a;\n}"));
}

#[test]
fn replacing_each_definition_with_its_own_text_round_trips_the_file() {
    let names = ["make", "helper", "Box", "open", "plain", "gapped"];
    for name in names {
        let (start, end, text) = with_symbols(SOURCE, |symbols, interner| {
            let sym = symbols
                .iter()
                .find(|sym| interner.resolve(&sym.name) == name)
                .expect("symbol");
            let text = sym
                .source_text(SOURCE.as_bytes())
                .expect("text")
                .into_owned();
            (sym.def_start_byte() as usize, sym.end_byte as usize, text)
        });
        let (out, _, _) = splice_file_bytes(
            SOURCE.as_bytes(),
            EditOp::ReplaceBody,
            start,
            end,
            text.as_bytes(),
        )
        .unwrap();
        assert_eq!(out, SOURCE.as_bytes(), "round trip of {name}");
    }
}
