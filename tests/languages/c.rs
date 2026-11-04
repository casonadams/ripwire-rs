use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};
use std::collections::HashMap;

#[test]
fn test_c_graph_protocol_and_header_refinement() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let cpp_hdr =
        b"namespace rw { inline bool kindIs(const char* a, const char* b) { return true; } }\n";
    let objc_hdr = b"@interface Renderer : NSObject\n- (void)draw;\n@end\n";
    let pure_c_hdr = b"int c_header_fn(int a);\n";
    let ingest = ingest_in_memory_files(
        &[
            ("include/util.h", Lang::Cpp, cpp_hdr),
            ("include/objc.h", Lang::Cpp, objc_hdr),
            ("include/pure.h", Lang::C, pure_c_hdr),
        ],
        &registry,
        &interner,
    );
    assert_eq!(ingest.files[0].lang, Lang::Cpp);
    assert_eq!(ingest.files[1].lang, Lang::ObjC);
    assert_eq!(ingest.files[2].lang, Lang::C);

    let mut file_index = HashMap::new();
    file_index.insert("include/api.h", 7);
    let c_sem = Lang::C.semantics();
    assert_eq!(
        c_sem.resolve_import_target("src/main.c", "<stdio.h>", &file_index),
        None
    );
    assert_eq!(
        c_sem.resolve_import_target("src/main.c", "\"include/api.h\"", &file_index),
        Some(7)
    );
}

#[test]
fn c_header_without_cpp_markers_refines_to_c_and_member_calls_never_bind_free_functions() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            (
                "include/plain.h",
                Lang::Cpp,
                b"int plain_fn(int a);\n#define LIMIT 8\n",
            ),
            (
                "include/ext.h",
                Lang::Cpp,
                b"extern \"C\" {\nint ext_fn(int a);\n}\n",
            ),
        ],
        &registry,
        &interner,
    );
    assert_eq!(ingest.files[0].lang, Lang::C);
    assert_eq!(ingest.files[1].lang, Lang::Cpp);

    let by_ref = ingest_in_memory_files(
        &[(
            "include/by_ref.h",
            Lang::Cpp,
            b"struct Box { int v; };\nint take(const Box& b);\n",
        )],
        &registry,
        &interner,
    );
    assert_eq!(by_ref.files[0].lang, Lang::Cpp);

    let source = b"struct Buffer { int (*get)(struct Buffer *); };\nstatic int get(struct Buffer *b) { return 0; }\nstatic int helper(struct Buffer *b) { return 0; }\nint drop(struct Buffer *b) { return b->get(b) + helper(b); }\n";
    let callees =
        super::harness::owner_qualified_callees(&[("src/pool.c", Lang::C, source)], "drop");
    assert_eq!(callees, [".helper"]);
}
