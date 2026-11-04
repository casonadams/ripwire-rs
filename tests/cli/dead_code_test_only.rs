use crate::index_support::index;
use ripwire_rs::Lang;
use ripwire_rs::cli::TopModifierArgs;
use ripwire_rs::commands::dead_code::{DeadCodeContext, execute_dead_code_query_paged};

fn dead_code_xml(files: &[(&str, &[u8])]) -> String {
    let typed: Vec<(&str, Lang, &[u8])> = files.iter().map(|&(p, c)| (p, Lang::C, c)).collect();
    let (ingest, graph, interner) = index(&typed);
    let page = TopModifierArgs::default();
    let ctx = DeadCodeContext {
        root: ":memory:",
        sources: &files,
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
        page: &page,
    };
    let mut out = Vec::new();
    execute_dead_code_query_paged(&ctx, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn functions_only_tests_call_are_listed_apart_from_count() {
    let xml = dead_code_xml(&[
        (
            "src/lib.c",
            b"int audit_only_helper(void) {\nreturn 1;\n}\nint shared_helper(void) {\nreturn 2;\n}\nint prod_entry(void) {\nreturn shared_helper();\n}\n",
        ),
        (
            "tests/check.c",
            b"int check_all(void) {\nreturn audit_only_helper() + shared_helper();\n}\n",
        ),
    ]);
    assert!(xml.contains("<to n=\"audit_only_helper\""), "{xml}");
    assert!(!xml.contains("n=\"shared_helper\""), "{xml}");
    assert!(!xml.contains("n=\"check_all\""), "{xml}");
    assert!(xml.contains(" count=\"0\""), "{xml}");
    assert!(xml.contains(" to=\"1\""), "{xml}");
}

#[test]
fn a_production_reference_keeps_a_function_out_of_the_test_only_group() {
    let xml = dead_code_xml(&[
        (
            "src/lib.c",
            b"int registered_helper(void) {\nreturn 1;\n}\nint (*handler)(void) = registered_helper;\n",
        ),
        (
            "tests/check.c",
            b"int check_all(void) {\nreturn registered_helper();\n}\n",
        ),
    ]);
    assert!(!xml.contains("n=\"registered_helper\""), "{xml}");
    assert!(!xml.contains(" to=\""), "{xml}");
}
