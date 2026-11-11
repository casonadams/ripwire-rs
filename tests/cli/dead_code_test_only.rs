use crate::support::index::index_files;
use ripwire_rs::Lang;
use ripwire_rs::cli::TopModifierArgs;
use ripwire_rs::commands::dead_code::{DeadCodeContext, execute_dead_code_query_paged};

fn dead_code_xml(files: &[(&str, &[u8])]) -> String {
    let typed: Vec<(&str, Lang, &[u8])> = files.iter().map(|&(p, c)| (p, Lang::C, c)).collect();
    let (ingest, graph, interner) = index_files(&typed);
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
fn test_only_references_are_listed_apart_from_production_callers() {
    type Case<'a> = (&'a [(&'a str, &'a [u8])], &'a [&'a str], &'a [&'a str]);
    let cases: [Case; 2] = [
        (
            &[
                (
                    "src/lib.c",
                    b"int audit_only_helper(void) {\nreturn 1;\n}\nint shared_helper(void) {\nreturn 2;\n}\nint prod_entry(void) {\nreturn shared_helper();\n}\n",
                ),
                (
                    "tests/check.c",
                    b"int check_all(void) {\nreturn audit_only_helper() + shared_helper();\n}\n",
                ),
            ],
            &["<to n=\"audit_only_helper\"", " count=\"0\"", " to=\"1\""],
            &["n=\"shared_helper\"", "n=\"check_all\""],
        ),
        (
            &[
                (
                    "src/lib.c",
                    b"int registered_helper(void) {\nreturn 1;\n}\nint (*handler)(void) = registered_helper;\n",
                ),
                (
                    "tests/check.c",
                    b"int check_all(void) {\nreturn registered_helper();\n}\n",
                ),
            ],
            &[],
            &["n=\"registered_helper\"", " to=\""],
        ),
    ];
    for (files, present, absent) in cases {
        let xml = dead_code_xml(files);
        for needle in present {
            assert!(xml.contains(needle), "missing {needle}: {xml}");
        }
        for needle in absent {
            assert!(!xml.contains(needle), "unexpected {needle}: {xml}");
        }
    }
}

#[test]
fn name_scoped_languages_list_the_functions_no_scoped_reference_reaches() {
    let files: [(&str, Lang, &[u8]); 5] = [
        ("app/probe.py", Lang::Python, b"def classify(a):\n    return a - 1\n\n\ndef used(a):\n    return a\n"),
        ("app/real.py", Lang::Python, b"def classify(a):\n    return a * 2\n"),
        (
            "app/user.py",
            Lang::Python,
            b"from app.real import classify\nfrom app.probe import used\n\n\ndef main():\n    return classify(1) + used(2)\n",
        ),
        ("web/src/lib.ts", Lang::TypeScript, b"export function orphan() {\n  return 1;\n}\n"),
        ("src/lib.c", Lang::C, b"static int helper(void) {\nreturn 1;\n}\n"),
    ];
    let (ingest, graph, interner) = index_files(&files);
    let page = TopModifierArgs::default();
    let sources: Vec<(&str, &[u8])> = files.iter().map(|&(p, _, c)| (p, c)).collect();
    let sources: &[(&str, &[u8])] = &sources;
    let ctx = DeadCodeContext {
        root: ":memory:",
        sources: &sources,
        ingest: &ingest,
        graph: &graph,
        interner: &interner,
        page: &page,
    };
    let mut out = Vec::new();
    execute_dead_code_query_paged(&ctx, &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    for needle in [
        "n=\"classify\" t=\"fn\" p=\"app/probe.py\"",
        "n=\"orphan\" t=\"fn\" p=\"web/src/lib.ts\"",
        "n=\"helper\"",
    ] {
        assert!(xml.contains(needle), "missing {needle}: {xml}");
    }
    for needle in ["p=\"app/real.py\"", "n=\"used\"", "n=\"main\""] {
        assert!(!xml.contains(needle), "unexpected {needle}: {xml}");
    }
}
