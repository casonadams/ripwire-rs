use crate::support::callees::callee_file_names;
use ripwire_rs::{Interner, Lang, LanguageRegistry, SymKind, ingest_in_memory_files};

/// Callee symbols of `caller`, as `file:name`, across an in-memory multi-file ingest.
pub(super) fn callee_locations(files: &[(&str, Lang, &[u8])], caller: &str) -> Vec<String> {
    callee_file_names(files, caller)
}

#[test]
fn zig_return_type_is_not_a_definition_and_methods_scope_to_their_struct() {
    let files: &[(&str, Lang, &[u8])] = &[(
        "src/store.zig",
        Lang::Zig,
        b"pub const Store = struct {\n    pub fn push(self: *Store, v: u32) void {\n        _ = v;\n    }\n    pub fn len(self: *const Store) usize {\n        _ = self;\n        return 0;\n    }\n};\npub fn make() Store {\n    return .{};\n}\n",
    )];
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let named = |name: &str| {
        ingest
            .symbols
            .iter()
            .filter(|s| interner.resolve(&s.name) == name)
            .collect::<Vec<_>>()
    };
    assert!(
        named("usize").is_empty(),
        "return type `usize` must not define a function"
    );
    let store = named("Store");
    assert_eq!(
        store.len(),
        1,
        "`Store` is the struct only, not the return type of `make`"
    );
    assert_eq!(store[0].kind, SymKind::Class);
    let push = named("push");
    assert_eq!(
        push[0]
            .scope
            .map(|s| interner.resolve(&s).to_string())
            .as_deref(),
        Some("Store")
    );
}

/// `const x = @import("m.zig")` binds `x` to that file: `x.f()` and `x.T.f()` bind into it. Each case is
/// `(label, files as (path, source), caller, callees the caller must have)`.
#[test]
fn zig_import_bound_qualifiers_bind_into_the_imported_file() {
    type Case<'a> = (&'a str, &'a [(&'a str, &'a [u8])], &'a str, &'a [&'a str]);
    let cases: &[Case<'_>] = &[
        (
            "a module function",
            &[
                (
                    "src/store.zig",
                    b"const util = @import(\"util.zig\");\npub fn push(v: u32) void {\n    util.log(v);\n}\n",
                ),
                ("src/util.zig", b"pub fn log(v: u32) void {\n    _ = v;\n}\n"),
                ("src/other.zig", b"pub fn log(v: u32) void {\n    _ = v;\n}\n"),
            ],
            "push",
            &["src/util.zig:log"],
        ),
        (
            "a type a module declares, beside a same-named type and a same-file init",
            &[
                (
                    "src/ast.zig",
                    b"pub const Type = struct {\n    pub fn init() Type {\n        return .{};\n    }\n};\n",
                ),
                (
                    "src/other.zig",
                    b"pub const Type = struct {\n    pub fn init() Type {\n        return .{};\n    }\n};\n",
                ),
                (
                    "src/parser.zig",
                    b"const ast = @import(\"ast.zig\");\nfn init() void {}\npub fn run() void {\n    _ = ast.Type.init();\n}\n",
                ),
            ],
            "run",
            &["src/ast.zig:init"],
        ),
    ];
    for (label, sources, caller, expected) in cases {
        let files: Vec<(&str, Lang, &[u8])> = sources
            .iter()
            .map(|(path, source)| (*path, Lang::Zig, *source))
            .collect();
        assert_eq!(callee_locations(&files, caller), *expected, "{label}");
    }
}
