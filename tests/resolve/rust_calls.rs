use crate::support::index::index_files;
use ripwire_rs::{Graph, IngestResult, Interner, Lang};

/// The `name@file` of every callee of the first symbol named `caller`, sorted. A struct literal or a type
/// mention is a callee too, so `make` reaches its `Opts` type as well as the `default` it calls.
fn callees(ingest: &IngestResult, graph: &Graph, interner: &Interner, caller: &str) -> Vec<String> {
    let from = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == caller)
        .expect("caller is indexed");
    let mut out: Vec<String> = graph
        .callees(from.id)
        .map(|(to, _)| {
            let sym = &ingest.symbols[to as usize];
            let file = interner.resolve(&ingest.files[sym.file_id as usize].path);
            format!("{}@{file}", interner.resolve(&sym.name))
        })
        .collect();
    out.sort();
    out
}

struct Case {
    rule: &'static str,
    files: &'static [(&'static str, Lang, &'static [u8])],
    caller: &'static str,
    callees: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        rule: "struct update default binds to the literal type's own impl",
        files: &[(
            "src/opts.rs",
            Lang::Rust,
            b"pub struct Opts {\n    pub a: u32,\n    pub b: u32,\n}\n\nimpl Default for Opts {\n    fn default() -> Self {\n        Self { a: 1, b: 2 }\n    }\n}\n\npub fn make() -> Opts {\n    Opts {\n        a: 3,\n        ..Default::default()\n    }\n}\n",
        )],
        caller: "make",
        callees: &["Opts@src/opts.rs", "default@src/opts.rs"],
    },
    Case {
        rule: "a derived Default does not bind to another type's impl",
        files: &[
            (
                "src/a.rs",
                Lang::Rust,
                b"#[derive(Default)]\npub struct Opts {\n    pub a: u32,\n}\n\npub fn make() -> Opts {\n    Opts {\n        a: 3,\n        ..Default::default()\n    }\n}\n",
            ),
            (
                "src/b.rs",
                Lang::Rust,
                b"pub struct Opts {\n    pub a: u32,\n}\n\nimpl Default for Opts {\n    fn default() -> Self {\n        Self { a: 1 }\n    }\n}\n",
            ),
        ],
        caller: "make",
        callees: &["Opts@src/a.rs"],
    },
    Case {
        rule: "a call inside macro arguments is counted with its own arguments",
        files: &[(
            "src/lib.rs",
            Lang::Rust,
            b"fn one(x: u32) -> u32 {\n    x\n}\n\nfn outer(v: Vec<u32>, a: u32, b: u32) -> u32 {\n    v.len() as u32 + a + b\n}\n\npub fn caller() -> u32 {\n    outer(vec![one(1)], 2, 3)\n}\n",
        )],
        caller: "caller",
        callees: &["one@src/lib.rs", "outer@src/lib.rs"],
    },
    Case {
        rule: "a self-recursive call leaves no self edge",
        files: &[(
            "src/walk.rs",
            Lang::Rust,
            b"pub fn walk(n: u32) -> u32 {\n    if n == 0 {\n        0\n    } else {\n        walk(n - 1)\n    }\n}\n",
        )],
        caller: "walk",
        callees: &[],
    },
    Case {
        rule: "a member call on another receiver is not bound to its own caller",
        files: &[(
            "src/kind.rs",
            Lang::Rust,
            b"pub struct Lang;\n\npub struct Kind;\n\nimpl Kind {\n    pub fn tag(&self, lang: &Lang) -> u8 {\n        lang.tag()\n    }\n}\n",
        )],
        caller: "tag",
        callees: &[],
    },
];

const STD_NAME_CASES: &[Case] = &[
    Case {
        rule: "an untyped get on a builtin slice does not bind to a module's free get",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn read(x: &[u8]) -> u8 {\n    x.get(0).copied().unwrap_or(0)\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "read",
        callees: &[],
    },
    Case {
        rule: "an untyped get on a generic receiver does not bind to a module's free get",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn read<W: Clone>(x: W) -> W {\n    x.clone()\n}\n\npub fn peek<W: Clone>(x: W) -> W {\n    x.get(0)\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "peek",
        callees: &[],
    },
    Case {
        rule: "a len call on a builtin receiver does not bind to an in-tree free len",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn size(v: &[u8]) -> usize {\n    v.len()\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn len(x: u8) -> usize {\n    x as usize\n}\n"),
        ],
        caller: "size",
        callees: &[],
    },
    Case {
        rule: "a map call on a builtin receiver does not bind to an in-tree free map",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn twice(v: Option<u8>) -> Option<u8> {\n    v.map(|x| x)\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn map(x: u8) -> u8 {\n    x\n}\n"),
        ],
        caller: "twice",
        callees: &[],
    },
];

const MODULE_PATH_CASES: &[Case] = &[
    Case {
        rule: "a module-qualified free fn named like a std member binds to that module",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn caller() -> String {\n    opts::get(\"a\")\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "caller",
        callees: &["get@src/opts.rs"],
    },
    Case {
        rule: "a module-qualified std-named fn still binds when another file defines the same name",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\npub mod other;\n\npub fn caller() -> String {\n    opts::get(\"a\")\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
            ("src/other.rs", Lang::Rust, b"pub fn get(k: u32) -> u32 {\n    k\n}\n"),
        ],
        caller: "caller",
        callees: &["get@src/opts.rs"],
    },
    Case {
        rule: "a crate-rooted module path binds like the bare module qualifier",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\npub fn caller() -> String {\n    crate::opts::get(\"a\")\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "caller",
        callees: &["get@src/opts.rs"],
    },
    Case {
        rule: "a use-renamed module qualifier binds to the module's fn",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\nuse crate::opts as o;\n\npub fn caller() -> String {\n    o::get(\"a\")\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "caller",
        callees: &["get@src/opts.rs"],
    },
];

const MODULE_SET_CASES: &[Case] = &[
    Case {
        rule: "a module-qualified set imported from a test file pins the module's free fn among four same-named definitions",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\npub mod table;\npub mod other;\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn set(v: u32) -> u32 {\n    v\n}\n"),
            ("src/table.rs", Lang::Rust, b"pub struct Table {\n    pub v: u32,\n}\n\nimpl Table {\n    pub fn set(&mut self, v: u32) {\n        self.v = v;\n    }\n}\n\npub fn set_default(v: u32) -> u32 {\n    v\n}\n"),
            ("src/other.rs", Lang::Rust, b"pub struct Other;\n\nimpl Other {\n    pub fn set(&mut self, v: u32) -> u32 {\n        v\n    }\n}\n\npub fn set(v: u8) -> u8 {\n    v\n}\n"),
            ("tests/options.rs", Lang::Rust, b"use demo::opts;\n\n#[test]\nfn sets() {\n    opts::set(3);\n}\n"),
        ],
        caller: "sets",
        callees: &["set@src/opts.rs"],
    },
    Case {
        rule: "a module imported by crate name binds when its test file shares the module's basename",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod options;\npub mod table;\n"),
            ("src/options.rs", Lang::Rust, b"pub fn set(v: u32) -> u32 {\n    v\n}\n"),
            ("src/table.rs", Lang::Rust, b"pub struct Table {\n    pub v: u32,\n}\n\nimpl Table {\n    pub fn set(&mut self, v: u32) {\n        self.v = v;\n    }\n}\n"),
            ("tests/options.rs", Lang::Rust, b"use demo::options;\n\n#[test]\nfn sets() {\n    options::set(3);\n}\n"),
        ],
        caller: "sets",
        callees: &["set@src/options.rs"],
    },
];

const MACRO_PATH_CASES: &[Case] = &[
    Case {
        rule: "a path call inside assert_eq binds to the module fn",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\n#[test]\nfn calls_both() {\n    assert_eq!(opts::fetch(\"a\"), \"a\");\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn fetch(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "calls_both",
        callees: &["fetch@src/opts.rs"],
    },
    Case {
        rule: "a type path call inside assert_eq binds to the type's associated fn",
        files: &[(
            "src/lib.rs",
            Lang::Rust,
            b"pub struct Opts {\n    pub a: u32,\n}\n\nimpl Opts {\n    pub fn new(a: u32) -> Self {\n        Opts { a }\n    }\n}\n\npub fn check() -> bool {\n    assert_eq!(Opts::new(3).a, 3)\n}\n",
        )],
        caller: "check",
        callees: &["new@src/lib.rs"],
    },
    Case {
        rule: "a turbofish type path call inside a macro binds to the associated fn",
        files: &[(
            "src/lib.rs",
            Lang::Rust,
            b"pub struct Wrap<T>(pub T);\n\nimpl<T> Wrap<T> {\n    pub fn new(v: T) -> Self {\n        Wrap(v)\n    }\n}\n\npub fn check() -> bool {\n    assert_eq!(Wrap::<u8>::new(3).0, 3)\n}\n",
        )],
        caller: "check",
        callees: &["new@src/lib.rs"],
    },
    Case {
        rule: "nested calls inside vec! bind through their own path segments",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod b;\n\nfn a(x: u32) -> u32 {\n    x\n}\n\npub fn nested() -> Vec<u32> {\n    vec![a(b::c(1))]\n}\n"),
            ("src/b.rs", Lang::Rust, b"pub fn c(x: u32) -> u32 {\n    x\n}\n"),
        ],
        caller: "nested",
        callees: &["a@src/lib.rs", "c@src/b.rs"],
    },
    Case {
        rule: "a path call in a macro_rules body is no call of the enclosing fn",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod opts;\n\nmacro_rules! call_get {\n    () => {\n        opts::get(\"a\")\n    };\n}\n\npub fn caller() -> String {\n    call_get!()\n}\n"),
            ("src/opts.rs", Lang::Rust, b"pub fn get(name: &str) -> String {\n    name.to_string()\n}\n"),
        ],
        caller: "caller",
        callees: &["call_get@src/lib.rs"],
    },
    Case {
        rule: "a fn declaration written inside macro arguments is no call of a same-named fn elsewhere",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod helpers;\n\npub fn caller() -> u32 {\n    keep!(fn helper(x: u32) -> u32 { x });\n    0\n}\n"),
            ("src/helpers.rs", Lang::Rust, b"pub fn helper(x: u32) -> u32 {\n    x\n}\n"),
        ],
        caller: "caller",
        callees: &[],
    },
];

const MACRO_SHAPE_CASES: &[Case] = &[
    Case {
        rule: "a Self path call inside a macro binds to the enclosing impl's associated fn",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod other;\n\npub struct Opts {\n    pub a: u32,\n}\n\nimpl Opts {\n    pub fn new(a: u32) -> Self {\n        Opts { a }\n    }\n\n    pub fn check() -> bool {\n        assert_eq!(Self::new(3).a, 3)\n    }\n}\n"),
            ("src/other.rs", Lang::Rust, b"pub struct Other;\n\nimpl Other {\n    pub fn new(a: u32) -> Other {\n        let _ = a;\n        Other\n    }\n}\n"),
        ],
        caller: "check",
        callees: &["new@src/lib.rs"],
    },
    Case {
        rule: "a path call inside macro arguments to an unindexed type's new does not bind to another type's new",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod doctor;\n\npub fn subcommands() -> Vec<u32> {\n    vec![ClapCommand::new(\"gate\")]\n}\n"),
            ("src/doctor.rs", Lang::Rust, b"pub struct Doctor;\n\nimpl Doctor {\n    pub fn new(name: &str) -> Doctor {\n        let _ = name;\n        Doctor\n    }\n}\n"),
        ],
        caller: "subcommands",
        callees: &[],
    },
    Case {
        rule: "a path call inside macro arguments through an unbound module keeps the name edge a plain call has",
        files: &[
            ("src/lib.rs", Lang::Rust, b"pub mod render;\n\npub fn caller() -> Vec<u32> {\n    vec![tools::render(1)]\n}\n"),
            ("src/render.rs", Lang::Rust, b"pub fn render(x: u32) -> u32 {\n    x\n}\n"),
        ],
        caller: "caller",
        callees: &["render@src/render.rs"],
    },
];

#[test]
fn call_binding_rules_hold_on_each_fixture() {
    let mismatches: Vec<String> = [
        CASES,
        STD_NAME_CASES,
        MODULE_PATH_CASES,
        MODULE_SET_CASES,
        MACRO_PATH_CASES,
        MACRO_SHAPE_CASES,
    ]
    .into_iter()
    .flatten()
    .filter_map(|case| {
        let (ingest, graph, interner) = index_files(case.files);
        let got = callees(&ingest, &graph, &interner, case.caller);
        (got != case.callees)
            .then(|| format!("{}: got {got:?}, want {:?}", case.rule, case.callees))
    })
    .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn std_named_members_bind_only_on_a_typed_in_tree_receiver() {
    let (ingest, graph, interner) = index_files(&[(
        "src/table.rs",
        Lang::Rust,
        b"pub struct Table {\n    rows: Vec<u8>,\n}\n\nimpl Table {\n    pub fn get(&self) -> u8 {\n        self.rows.len() as u8\n    }\n}\n\npub fn read(t: &Table) -> u8 {\n    t.get()\n}\n\npub fn size(v: &[u8]) -> usize {\n    v.len()\n}\n",
    )]);
    assert_eq!(
        callees(&ingest, &graph, &interner, "read"),
        vec!["get@src/table.rs".to_string()]
    );
    assert!(callees(&ingest, &graph, &interner, "size").is_empty());
    assert!(callees(&ingest, &graph, &interner, "get").is_empty());
}
