use ripwire_rs::ingest::identifiers::count_identifier_uses;
use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};
use std::collections::HashMap;

/// Identifier-token counts of `names` across one in-memory file of `lang`.
fn counts(
    path: &str,
    lang: Lang,
    source: &str,
    names: &[&'static str],
) -> Vec<(&'static str, u32)> {
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&[(path, lang, source.as_bytes())], &registry, &interner);
    let sources = [(path, source.as_bytes())];
    let mut uses: HashMap<&str, u32> = names.iter().map(|n| (*n, 0)).collect();
    count_identifier_uses(&ingest, &interner, &sources, lang, &mut uses);
    let mut out: Vec<_> = names.iter().map(|n| (*n, uses[n])).collect();
    out.sort_unstable();
    out
}

/// A prototype and definition of `prototyped` with no caller, and of `declared` with one.
const C_LINKAGE: &str = "static int declared(void);\n\nint declared(void) {\n    return 1;\n}\n\nint caller(void) {\n    return declared();\n}\n\nstatic int prototyped(void);\n\nint prototyped(void) {\n    return 2;\n}\n";

#[test]
fn test_identifier_uses_count_value_positions_but_not_definitions_or_prototypes() {
    let ts = "export function continueReview(): void {}\n\nexport const options = { continueReview };\n\nexport function lonely(): void {}\n";
    let rust = "fn helper() -> u32 {\n    1\n}\n\nfn handler() {}\n\nregister!(helper);\n\nstatic TABLE: &[fn()] = &[handler];\n\nfn lonely() {}\n";
    let observed = (
        counts(
            "src/review.ts",
            Lang::TypeScript,
            ts,
            &["continueReview", "lonely"],
        ),
        counts(
            "src/lib.rs",
            Lang::Rust,
            rust,
            &["handler", "helper", "lonely"],
        ),
        counts(
            "src/linkage.c",
            Lang::C,
            C_LINKAGE,
            &["declared", "prototyped"],
        ),
    );
    assert_eq!(
        observed,
        (
            vec![("continueReview", 1), ("lonely", 0)],
            vec![("handler", 1), ("helper", 1), ("lonely", 0)],
            vec![("declared", 1), ("prototyped", 0)],
        )
    );
}
