use ripwire_rs::gate::contract::parse_symbol_contract_from_bytes;
use ripwire_rs::{
    ContractStatus, Graph, Interner, Lang, LanguageRegistry, check_symbol_contract,
    find_symbols_by_name, ingest_in_memory_files,
};

struct Outcome {
    status: ContractStatus,
    callers: usize,
    incompatible: usize,
    sites: usize,
}

fn check(path: &str, lang: Lang, head: &str, now: &str, name: &str) -> Outcome {
    let baseline = parse_symbol_contract_from_bytes(head.as_bytes(), name, lang)
        .expect("parse head contract")
        .expect("head defines the symbol");
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&[(path, lang, now.as_bytes())], &reg, &interner);
    let graph = Graph::build(&ingest, &interner);
    let target = find_symbols_by_name(name, &ingest, &interner)[0];
    let report =
        check_symbol_contract(target, Some(&baseline), &ingest, &graph, &interner).expect("report");
    Outcome {
        status: report.status,
        callers: report.total_callers,
        incompatible: report.incompatible_callers,
        sites: report.callers.iter().map(|c| c.sites.len()).sum(),
    }
}

const RUST_HEAD: &str = "pub struct S;
impl S {
    pub fn go(&mut self, name: &str) {}
    pub fn a(&mut self) { self.go(\"x\"); }
    pub fn b(&mut self) { self.go(\"y\"); }
}
";

const PY_HEAD: &str = "class S:
    def go(self, name):
        pass

    def a(self):
        self.go('x')

    def b(self):
        self.go('y')
";

const GO_HEAD: &str = "package p

type S struct{}

func (s *S) Go(name string) {}

func (s *S) A() { s.Go(\"x\") }

func (s *S) B() { s.Go(\"y\") }
";

struct Case {
    label: &'static str,
    path: &'static str,
    lang: Lang,
    name: &'static str,
    head: &'static str,
    now: String,
}

fn cases() -> Vec<(Case, bool, usize)> {
    let rust = |label, now: String| Case {
        label,
        path: "src/s.rs",
        lang: Lang::Rust,
        name: "go",
        head: RUST_HEAD,
        now,
    };
    let py = |label, now: String| Case {
        label,
        path: "s.py",
        lang: Lang::Python,
        name: "go",
        head: PY_HEAD,
        now,
    };
    let go = |label, now: String| Case {
        label,
        path: "s.go",
        lang: Lang::Go,
        name: "Go",
        head: GO_HEAD,
        now,
    };
    vec![
        (rust("rust unchanged", RUST_HEAD.into()), false, 0),
        (
            rust(
                "rust param added",
                RUST_HEAD.replace("name: &str)", "name: &str, n: u32)"),
            ),
            true,
            2,
        ),
        (
            rust(
                "rust param removed",
                RUST_HEAD.replace("&mut self, name: &str)", "&mut self)"),
            ),
            true,
            2,
        ),
        (py("python unchanged", PY_HEAD.into()), false, 0),
        (
            py(
                "python param added",
                PY_HEAD.replace("self, name)", "self, name, n)"),
            ),
            true,
            2,
        ),
        (
            py(
                "python param removed",
                PY_HEAD.replace("self, name)", "self)"),
            ),
            true,
            2,
        ),
        (go("go unchanged", GO_HEAD.into()), false, 0),
        (
            go(
                "go param added",
                GO_HEAD.replace("name string)", "name string, n int)"),
            ),
            true,
            2,
        ),
        (
            go(
                "go param removed",
                GO_HEAD.replace("(name string) {}", "() {}"),
            ),
            true,
            2,
        ),
    ]
}

#[test]
fn receiver_is_not_a_call_argument() {
    for (case, contract_changed, flagged_sites) in cases() {
        let got = check(case.path, case.lang, case.head, &case.now, case.name);
        assert_eq!(got.callers, 2, "{}: callers", case.label);
        assert_eq!(
            got.status != ContractStatus::Unchanged,
            contract_changed,
            "{}: status",
            case.label
        );
        assert_eq!(
            got.incompatible, flagged_sites,
            "{}: incompatible",
            case.label
        );
        assert_eq!(got.sites, flagged_sites, "{}: sites", case.label);
    }
}

#[test]
fn edits_flag_only_the_call_sites_the_new_contract_breaks() {
    let unbound_head = "pub fn go(x: u8) {}\npub fn a() { go(1); }\npub fn b() { go(1, 2, 3); }\n";
    type Check = fn(&Outcome);
    let cases: [(String, String, Check); 5] = [
        (
            RUST_HEAD.into(),
            RUST_HEAD.replace("pub fn go", "fn go"),
            |got| assert_eq!((got.callers, got.incompatible), (2, 0)),
        ),
        (
            RUST_HEAD.into(),
            RUST_HEAD.replace("self.go(\"x\")", "self.go(\"x\", 1)"),
            |got| {
                assert_eq!(got.status, ContractStatus::Unchanged);
                assert_eq!((got.callers, got.incompatible), (2, 1));
            },
        ),
        (
            "pub fn go(x: u8) {}\npub fn a() { go(1); }\npub fn b() { go(2); }\n".into(),
            "pub fn go() {}\npub fn a() { go(1); }\npub fn b() { go(2); }\n".into(),
            |got| assert_eq!((got.callers, got.incompatible, got.sites), (2, 2, 2)),
        ),
        (
            unbound_head.into(),
            unbound_head.replace("go(x: u8)", "go()"),
            |got| assert_eq!((got.callers, got.incompatible), (1, 1)),
        ),
        (
            "pub fn go(x: u8) {}\npub fn a() { go(1, 2); }\n".into(),
            "pub fn go(x: u8) {}\npub fn a() { go(1, 2); }\n".into(),
            |got| {
                assert_eq!(got.status, ContractStatus::Unchanged);
                assert_eq!((got.callers, got.incompatible), (0, 0));
            },
        ),
    ];
    for (head, now, verify) in cases {
        verify(&check("src/s.rs", Lang::Rust, &head, &now, "go"));
    }
}

#[test]
fn calls_that_fit_the_definition_they_bind_are_never_flagged() {
    let cases: [(&str, &str, Lang, &str, usize); 3] = [
        (
            "path call writing the receiver",
            "src/s.rs",
            Lang::Rust,
            "pub struct S;\nimpl S {\n    pub fn go(&mut self, name: &str) {}\n    pub fn a(&mut self) { S::go(self, \"x\"); }\n}\n",
            1,
        ),
        (
            "python staticmethod",
            "s.py",
            Lang::Python,
            "class S:\n    @staticmethod\n    def go(name):\n        pass\n\n    def a(self):\n        S.go('x')\n        self.go('y')\n",
            1,
        ),
        (
            "same name on another type",
            "src/s.rs",
            Lang::Rust,
            "pub struct P;\npub struct Q;\nimpl P { pub fn go(&self, a: u8) {} }\nimpl Q { pub fn go(&self) {} }\npub fn via_p(p: P) { p.go(1); }\npub fn via_q(q: Q) { q.go(); }\n",
            1,
        ),
    ];
    for (label, path, lang, src, callers) in cases {
        let got = check(path, lang, src, src, "go");
        assert_eq!((got.callers, got.incompatible), (callers, 0), "{label}");
    }
}

struct ArityCase {
    label: &'static str,
    path: &'static str,
    lang: Lang,
    name: &'static str,
    head: &'static str,
    now: &'static str,
    callers: usize,
    incompatible: usize,
}

#[test]
fn defaulted_variadic_and_keyword_params_bind_the_counts_they_accept() {
    let cases = [
        ArityCase {
            label: "python defaulted param, call omits it",
            path: "s.py",
            lang: Lang::Python,
            name: "connect_tcp",
            head: "def connect_tcp(address):\n    return address\n\ndef dial(connect):\n    return connect_tcp(connect)\n",
            now: "def connect_tcp(address, wait=120.0):\n    return address\n\ndef dial(connect):\n    return connect_tcp(connect)\n",
            callers: 1,
            incompatible: 0,
        },
        ArityCase {
            label: "python too few args for a required param",
            path: "s.py",
            lang: Lang::Python,
            name: "f",
            head: "def f(a):\n    return a\n\ndef g(x):\n    return f(x)\n",
            now: "def f(a, b, c=1):\n    return a\n\ndef g(x):\n    return f(x)\n",
            callers: 1,
            incompatible: 1,
        },
        ArityCase {
            label: "python *args accepts any count",
            path: "s.py",
            lang: Lang::Python,
            name: "f",
            head: "def f(a):\n    return a\n\ndef g(x):\n    return f(x, 2, 3)\n",
            now: "def f(a, *args):\n    return a\n\ndef g(x):\n    return f(x, 2, 3)\n",
            callers: 1,
            incompatible: 0,
        },
        ArityCase {
            label: "python **kwargs accepts keywords",
            path: "s.py",
            lang: Lang::Python,
            name: "f",
            head: "def f(a):\n    return a\n\ndef g(x):\n    return f(x)\n",
            now: "def f(a, **kw):\n    return a\n\ndef g(x):\n    return f(x, k=1)\n",
            callers: 1,
            incompatible: 0,
        },
        ArityCase {
            label: "python keyword-only param: separator is not a slot",
            path: "s.py",
            lang: Lang::Python,
            name: "f",
            head: "def f(a):\n    return a\n\ndef g(x):\n    return f(x)\n\ndef h(x):\n    return f(x, b=2)\n",
            now: "def f(a, *, b):\n    return a\n\ndef g(x):\n    return f(x)\n\ndef h(x):\n    return f(x, b=2)\n",
            callers: 2,
            incompatible: 1,
        },
        ArityCase {
            label: "python annotated default",
            path: "s.py",
            lang: Lang::Python,
            name: "f",
            head: "def f(a):\n    return a\n\ndef g(x):\n    return f(x)\n",
            now: "def f(a, b: int = 1):\n    return a\n\ndef g(x):\n    return f(x)\n",
            callers: 1,
            incompatible: 0,
        },
        ArityCase {
            label: "python std-name collision: builtin-shaped get is not in-tree",
            path: "s.py",
            lang: Lang::Python,
            name: "get",
            head: "class S:\n    def get(self, key):\n        return key\n\n    def b(self):\n        return self.get('x')\n",
            now: "class S:\n    def get(self, key, default=None):\n        return key\n\n    def b(self, d):\n        return self.get('x') or d.get('k', 1, 2)\n",
            callers: 1,
            incompatible: 0,
        },
        ArityCase {
            label: "typescript optional and defaulted params",
            path: "s.ts",
            lang: Lang::TypeScript,
            name: "f",
            head: "function f(a: number) {\n  return a;\n}\nfunction g() {\n  return f(1);\n}\n",
            now: "function f(a: number, b?: number, c = 1) {\n  return a;\n}\nfunction g() {\n  return f(1);\n}\nfunction h() {\n  return f();\n}\n",
            callers: 2,
            incompatible: 1,
        },
        ArityCase {
            label: "ruby defaulted, splat, block and required keyword",
            path: "s.rb",
            lang: Lang::Ruby,
            name: "f",
            head: "def f(a)\n  a\nend\n\ndef g\n  f(1)\nend\n",
            now: "def f(a, b = 1, *rest, k:, o: 2, &blk)\n  a\nend\n\ndef g\n  f(1, k: 2)\nend\n\ndef h\n  f(1)\nend\n",
            callers: 2,
            incompatible: 1,
        },
    ];
    for case in cases {
        let got = check(case.path, case.lang, case.head, case.now, case.name);
        assert_eq!(got.callers, case.callers, "{}: callers", case.label);
        assert_eq!(
            got.incompatible, case.incompatible,
            "{}: incompatible",
            case.label
        );
    }
}
