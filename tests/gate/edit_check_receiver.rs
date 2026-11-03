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
fn visibility_only_change_flags_no_call_site() {
    let now = RUST_HEAD.replace("pub fn go", "fn go");
    let got = check("src/s.rs", Lang::Rust, RUST_HEAD, &now, "go");
    assert_eq!((got.callers, got.incompatible), (2, 0));
}

#[test]
fn path_call_passing_the_receiver_explicitly_is_compatible() {
    let now = "pub struct S;
impl S {
    pub fn go(&mut self, name: &str) {}
    pub fn a(&mut self) { S::go(self, \"x\"); }
}
";
    let got = check("src/s.rs", Lang::Rust, now, now, "go");
    assert_eq!((got.callers, got.incompatible), (1, 0));
}

#[test]
fn python_staticmethod_takes_no_receiver() {
    let now = "class S:
    @staticmethod
    def go(name):
        pass

    def a(self):
        S.go('x')
        self.go('y')
";
    let got = check("s.py", Lang::Python, now, now, "go");
    assert_eq!((got.callers, got.incompatible), (1, 0));
}

#[test]
fn caller_with_wrong_arity_is_flagged_even_when_contract_is_unchanged() {
    let now = RUST_HEAD.replace("self.go(\"x\")", "self.go(\"x\", 1)");
    let got = check("src/s.rs", Lang::Rust, RUST_HEAD, &now, "go");
    assert_eq!(got.status, ContractStatus::Unchanged);
    assert_eq!((got.callers, got.incompatible), (2, 1));
}
