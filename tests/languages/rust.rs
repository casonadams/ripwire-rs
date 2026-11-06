use super::harness::owner_qualified_callees;
use crate::support::index::index_files;
use ripwire_rs::{Lang, LanguageRegistry, SymKind};
use std::collections::HashMap;

use ripwire_rs::domain::RefRole;
use ripwire_rs::ingest::parse_file_content;
#[test]
fn test_rust_graph_protocol_and_imports() {
    let mut file_index = HashMap::new();
    file_index.insert("crates/core/src/sub/worker.rs", 4);
    file_index.insert("crates/core/src/common.rs", 5);
    let sem = Lang::Rust.semantics();
    assert_eq!(
        sem.resolve_import_target(
            "crates/core/src/lib.rs",
            "crate::sub::worker::Run",
            &file_index
        ),
        Some(4)
    );
    assert_eq!(
        sem.resolve_import_target("crates/core/src/sub/mod.rs", "self::worker", &file_index),
        Some(4)
    );
    assert_eq!(
        sem.resolve_import_target("crates/core/src/sub/mod.rs", "super::common", &file_index),
        Some(5)
    );
    assert_eq!(
        sem.resolve_import_target("crates/cli/src/main.rs", "core::common::Item", &file_index),
        Some(5)
    );
    assert!(sem.is_std_or_builtin_qualifier("std"));
    assert!(sem.is_std_or_builtin_member("unwrap"));
    assert!(sem.is_entrypoint_symbol("main", None, SymKind::Function, "src/main.rs"));
    assert!(!sem.is_entrypoint_symbol("main", None, SymKind::Function, "examples/demo.rs"));
}

#[test]
fn test_rust_receiver_type_evidence_and_bindings() {
    let reg = LanguageRegistry::new().unwrap();
    let (rust_lang, query) = reg.get(Lang::Rust).unwrap();

    let code = br#"
use cc::Build;

pub trait Processor {
    fn perform(&self);
    fn run(&self) {
        self.perform();
    }
}

pub struct Worker;

impl Processor for Worker {
    fn perform(&self) {
        finish();
    }
}

pub trait XmlReport {
    fn generate(&self);
    fn to_xml_report(&self);
}

pub trait ToXmlString {
    fn to_xml(&self);
}

impl<T: XmlReport> ToXmlString for T {
    fn to_xml(&self) {
        self.to_xml_report();
    }
}

fn invoke_dynamic(processor: &dyn Processor) {
    processor.run();
}

fn invoke_generic<T: XmlReport>(report: &T) {
    report.generate();
}

fn invoke_constructors() {
    let mut build = cc::Build::new();
    build.file("src/main.c");

    let worker = Worker::new();
    worker.perform();

    let boxed: Box<dyn Processor> = Box::new(Worker::new());
    boxed.run();

    let arc = std::sync::Arc::new(Worker);
    arc.perform();
}

fn invoke_shadowing() {
    let mut build = cc::Build::new();
    {
        let mut build = Worker::new();
        build.perform();
    }
    build.file("src/lib.c");
}

fn invoke_callable_field(rule: &Rule) {
    (rule.check)(123);
}
"#;

    let res = parse_file_content(code, rust_lang, query, Lang::Rust, "src/test.rs");

    // 1. Extends ref for impl Processor for Worker
    let extends_worker = res
        .refs
        .iter()
        .find(|r| r.role == RefRole::Extends && r.callee_name == "Processor")
        .expect("extends Processor reference");
    assert_eq!(extends_worker.receiver_type.as_deref(), Some("Worker"));
    assert_eq!(extends_worker.qualifier, None);

    // 2. Extends ref for impl<T: XmlReport> ToXmlString for T
    let extends_blanket = res
        .refs
        .iter()
        .find(|r| r.role == RefRole::Extends && r.callee_name == "ToXmlString")
        .expect("extends ToXmlString reference");
    assert_eq!(extends_blanket.receiver_type.as_deref(), Some("T"));

    // 3. self.perform() inside trait Processor default method
    let self_in_trait = res
        .refs
        .iter()
        .find(|r| {
            r.callee_name == "perform"
                && r.receiver.as_deref() == Some("self")
                && r.receiver_type.as_deref() == Some("Processor")
        })
        .expect("self.perform in trait Processor");
    assert_eq!(self_in_trait.qualifier, None);

    // 4. self.to_xml_report() inside impl<T: XmlReport> ToXmlString for T
    let self_bound = res
        .refs
        .iter()
        .find(|r| r.callee_name == "to_xml_report" && r.receiver.as_deref() == Some("self"))
        .expect("self.to_xml_report call");
    assert_eq!(self_bound.receiver_type.as_deref(), Some("XmlReport"));

    // 5. Dynamic trait dispatch: processor.run()
    let dyn_run = res
        .refs
        .iter()
        .find(|r| r.callee_name == "run" && r.receiver.as_deref() == Some("processor"))
        .expect("processor.run call");
    assert_eq!(dyn_run.receiver_type.as_deref(), Some("Processor"));
    assert_eq!(dyn_run.qualifier, None);

    // 6. Generic bound call: report.generate()
    let gen_call = res
        .refs
        .iter()
        .find(|r| r.callee_name == "generate" && r.receiver.as_deref() == Some("report"))
        .expect("report.generate call");
    assert_eq!(gen_call.receiver_type.as_deref(), Some("XmlReport"));

    // 7. External constructor call: build.file() preserves cc::Build
    let build_file = res
        .refs
        .iter()
        .find(|r| r.callee_name == "file" && r.receiver.as_deref() == Some("build"))
        .expect("build.file call");
    assert_eq!(build_file.receiver_type.as_deref(), Some("cc::Build"));
    assert_eq!(build_file.qualifier, None);

    // 8. Box<dyn Processor> unwrapped
    let boxed_call = res
        .refs
        .iter()
        .find(|r| r.callee_name == "run" && r.receiver.as_deref() == Some("boxed"))
        .expect("boxed.run call");
    assert_eq!(boxed_call.receiver_type.as_deref(), Some("Processor"));

    // 9. Arc<Worker> unwrapped
    let arc_call = res
        .refs
        .iter()
        .find(|r| r.callee_name == "perform" && r.receiver.as_deref() == Some("arc"))
        .expect("arc.perform call");
    assert_eq!(arc_call.receiver_type.as_deref(), Some("Worker"));

    // 10. Parenthesized callable field call: (rule.check)(123)
    let callable_call = res
        .refs
        .iter()
        .find(|r| r.callee_name == "check")
        .expect("rule.check call");
    assert!(
        callable_call.callable_field,
        "must set callable_field = true"
    );
}

fn rust_parse(code: &str) -> ripwire_rs::ingest::FileParseResult {
    let reg = LanguageRegistry::new().unwrap();
    let (rust_lang, query) = reg.get(Lang::Rust).unwrap();
    parse_file_content(code.as_bytes(), rust_lang, query, Lang::Rust, "src/test.rs")
}

fn rust_ref<'a>(
    res: &'a ripwire_rs::ingest::FileParseResult,
    callee: &str,
    receiver: &str,
) -> &'a ripwire_rs::ingest::RawRef {
    res.refs
        .iter()
        .find(|r| r.callee_name == callee && r.receiver.as_deref() == Some(receiver))
        .expect("receiver call")
}

#[test]
fn test_rust_std_path_and_impl_facts() {
    let res = rust_parse("fn drop(_: u8) {} fn invoke() { std::mem::drop(1u8); }");
    let call = res
        .refs
        .iter()
        .find(|r| r.callee_name == "drop")
        .expect("drop call");
    assert_eq!(call.qualifier.as_deref(), Some("std::mem"));
    assert!(
        Lang::Rust
            .semantics()
            .is_std_or_builtin_qualifier("std::mem")
    );

    let res = rust_parse(
        "struct Worker; trait Runner { fn run(&self); } impl Runner for Worker { fn run(&self) {} }",
    );
    let imp = res
        .implementations
        .iter()
        .find(|i| i.subject == "Worker")
        .expect("impl Worker fact");
    assert_eq!(imp.trait_path.as_deref(), Some("Runner"));
    assert!(!imp.generic_subject);

    let res = rust_parse("trait Report {} trait Text {} impl<T> Text for T where T: Report {}");
    let imp = res
        .implementations
        .iter()
        .find(|i| i.subject == "T")
        .expect("blanket impl fact");
    assert_eq!(imp.trait_path.as_deref(), Some("Text"));
    assert_eq!(imp.subject_bound.as_deref(), Some("Report"));
    assert!(imp.generic_subject);
}

#[test]
fn test_rust_let_binding_does_not_keep_outer_type() {
    let res = rust_parse(
        "struct A; struct B; impl A { fn ping(&self) {} } impl B { fn ping(&self) {} } fn make_b() -> B { B } fn invoke(x: A) { let x = make_b(); x.ping(); }",
    );
    assert_ne!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("A")
    );
}

#[test]
fn test_rust_local_box_does_not_unwrap_to_inner() {
    let res = rust_parse(
        "mod custom { pub struct Box<T>(pub T); impl<T> Box<T> { pub fn ping(&self) {} } } use custom::Box; struct Inner; impl Inner { fn ping(&self) {} } fn invoke(x: Box<Inner>) { x.ping(); }",
    );
    assert_ne!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Inner")
    );
}

#[test]
fn test_rust_where_clause_bound_is_receiver_type() {
    let res = rust_parse(
        "trait Runner { fn run(&self) {} } fn invoke<T>(x: &T) where T: Runner { x.run(); }",
    );
    assert_eq!(
        rust_ref(&res, "run", "x").receiver_type.as_deref(),
        Some("Runner")
    );
}

#[test]
fn test_rust_ambiguous_nested_bounds_do_not_use_outer_bound() {
    let res = rust_parse(
        "trait A { fn go(&self) {} } trait B { fn go(&self) {} } trait Marker {} fn outer<T: A>() { fn inner<T: B + Marker>(x: T) { x.go(); } }",
    );
    assert_ne!(
        rust_ref(&res, "go", "x").receiver_type.as_deref(),
        Some("A")
    );
}

#[test]
fn test_rust_generic_bound_strips_generic_arguments() {
    let res = rust_parse(
        "trait Runner<U> { fn run(&self) {} } fn invoke<T: Runner<u8>>(x: &T) { x.run(); }",
    );
    assert_eq!(
        rust_ref(&res, "run", "x").receiver_type.as_deref(),
        Some("Runner")
    );
}

#[test]
fn test_rust_type_alias_resolves_to_target() {
    let res = rust_parse(
        "struct Worker; impl Worker { fn ping(&self) {} } type Alias = Worker; fn invoke(x: Alias) { x.ping(); }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Worker")
    );
}

#[test]
fn test_rust_std_arc_new_unwraps_to_inner_type() {
    let res = rust_parse(
        "struct Worker {} impl Worker { fn ping(&self) {} } fn invoke() { let x = std::sync::Arc::new(Worker {}); x.ping(); }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Worker")
    );
}

#[test]
fn test_rust_associated_call_return_selects_return_type() {
    let res = rust_parse(
        "struct Factory; struct Product; impl Factory { fn make() -> Product { Product } fn ping(&self) {} } impl Product { fn ping(&self) {} } fn invoke() { let x = Factory::make(); x.ping(); }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Product")
    );
}

#[test]
fn test_rust_self_box_receiver_resolves_to_enclosing_type() {
    let res = rust_parse(
        "struct Worker; impl Worker { fn ping(&self) {} fn invoke(self: Box<Self>) { self.ping(); } }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "self").receiver_type.as_deref(),
        Some("Worker")
    );
}

#[test]
fn test_rust_destructured_binding_gets_no_type() {
    let res = rust_parse(
        "struct Inner; struct Outer(Inner); impl Inner { fn ping(&self) {} } impl Outer { fn ping(&self) {} } fn invoke(Outer(x): Outer) { x.ping(); }",
    );
    assert_ne!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Outer")
    );
}

#[test]
fn test_rust_nested_custom_box_does_not_shadow_prelude_box() {
    let res = rust_parse(
        "mod custom { pub struct Box<T>(pub T); impl<T> Box<T> { pub fn ping(&self) { custom_end(); } } fn custom_end() {} } struct Inner; impl Inner { fn ping(&self) { inner_end(); } } fn inner_end() {} fn invoke(x: Box<Inner>) { x.ping(); }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Inner")
    );
}

#[test]
fn test_rust_std_mutex_renamed_box_is_not_transparent() {
    let res = rust_parse(
        "use std::sync::Mutex as Box; struct Inner; impl Inner { fn get_mut(&mut self) {} } fn invoke(mut x: Box<Inner>) { x.get_mut(); }",
    );
    assert_ne!(
        rust_ref(&res, "get_mut", "x").receiver_type.as_deref(),
        Some("Inner")
    );
}

#[test]
fn test_rust_renamed_std_arc_import_is_transparent() {
    let res = rust_parse(
        "use std::sync::Arc as Shared; struct Worker; impl Worker { fn ping(&self) {} } fn invoke(x: Shared<Worker>) { x.ping(); }",
    );
    assert_eq!(
        rust_ref(&res, "ping", "x").receiver_type.as_deref(),
        Some("Worker")
    );
}

#[test]
fn test_rust_impl_level_generic_where_bound_is_receiver_type() {
    let res = rust_parse(
        "trait Runner { fn run(&self) {} } struct Wrapper<T>(T); impl<T> Wrapper<T> { fn invoke(&self, x: &T) where T: Runner { x.run(); } }",
    );
    assert_eq!(
        rust_ref(&res, "run", "x").receiver_type.as_deref(),
        Some("Runner")
    );
}

#[test]
fn test_rust_scoped_call_site_is_not_also_a_read() {
    let res = rust_parse(
        "mod util { pub fn row_bytes(n: u8) -> u8 { n } } fn invoke() -> u8 { crate::util::row_bytes(1) }",
    );
    let sites: Vec<_> = res
        .refs
        .iter()
        .filter(|r| r.callee_name == "row_bytes")
        .map(|r| r.role)
        .collect();
    assert_eq!(sites, vec![RefRole::Call]);
}

fn rust_callers_of(source: &[u8], name: &str) -> usize {
    use ripwire_rs::find_symbols_by_name;
    let (ingest, graph, interner) = index_files(&[("src/lib.rs", Lang::Rust, source)]);
    let ids = find_symbols_by_name(name, &ingest, &interner);
    assert_eq!(ids.len(), 1, "one definition of {name}");
    graph.callers(ids[0]).count()
}

#[test]
fn test_rust_macro_argument_calls_bind_only_to_bare_calls() {
    let src = b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { assert_eq!(alpha(2), 2); }\n";
    assert_eq!(rust_callers_of(src, "alpha"), 1);
    let nested =
        b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { assert!(vec![alpha(2)].len() > 0); }\n";
    assert_eq!(rust_callers_of(nested, "alpha"), 1);

    let string_only = b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { println!(\"alpha(2)\"); }\n";
    assert_eq!(rust_callers_of(string_only, "alpha"), 0);
    let operator = b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { assert!(alpha + (2) > 0); }\n";
    assert_eq!(rust_callers_of(operator, "alpha"), 0);
    let rules = b"pub fn alpha(x: u8) -> u8 { x }\nmacro_rules! probe { () => { alpha(2) }; }\n";
    assert_eq!(rust_callers_of(rules, "alpha"), 0);
    let attribute =
        b"pub fn derive(x: u8) -> u8 { x }\n#[cfg_attr(test, derive(Debug))]\nstruct Marker;\n";
    assert_eq!(rust_callers_of(attribute, "derive"), 0);

    let pattern =
        b"pub struct Mode(pub u8);\nfn checks(k: Kind) -> bool { matches!(k, Kind::Mode(_)) }\n";
    assert_eq!(rust_callers_of(pattern, "Mode"), 0);

    let method =
        b"pub fn alpha(x: u8) -> u8 { x }\nfn checks(v: V) { assert_eq!(v.alpha(2), 2); }\n";
    assert_eq!(rust_callers_of(method, "alpha"), 0);
    let path =
        b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { assert_eq!(other::alpha(2), 2); }\n";
    assert_eq!(rust_callers_of(path, "alpha"), 1);
    let bare = b"pub fn alpha(x: u8) -> u8 { x }\nfn checks() { assert_eq!(alpha(2), 2); }\n";
    assert_eq!(rust_callers_of(bare, "alpha"), 1);
}

fn external_surface_xml(src: &[u8]) -> String {
    use ripwire_rs::serialize::PageRequest;
    let (ingest, _, interner) = index_files(&[("src/lib.rs", Lang::Rust, src)]);
    let mut out = Vec::new();
    ripwire_rs::commands::external_surface::execute_external_surface_query(
        ".",
        &ingest,
        &interner,
        PageRequest::default(),
        &mut out,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn test_rust_macro_closure_and_dsl_names_are_not_external() {
    let xml = external_surface_xml(
        b"fn checks() { let callers_of = |x: u8| x; assert_eq!(callers_of(2), 2); }\n",
    );
    assert!(!xml.contains("n=\"callers_of\""), "{xml}");

    let xml = external_surface_xml(
        b"command! { run: nav(Mode::Callers) }\nfn other() { vendor_send(1); }\n",
    );
    assert!(
        !xml.contains("n=\"nav\""),
        "a DSL token is not a call: {xml}"
    );
    assert!(
        xml.contains("n=\"vendor_send\""),
        "a real unresolved call outside a macro stays external: {xml}"
    );
}

#[test]
fn test_rust_macro_argument_call_resolves_across_files() {
    use ripwire_rs::{
        Graph, Interner, LanguageRegistry, find_symbols_by_name, ingest_in_memory_files,
    };
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files: [(&str, Lang, &[u8]); 2] = [
        ("src/a.rs", Lang::Rust, b"pub fn alpha(x: u8) -> u8 { x }\n"),
        (
            "src/b.rs",
            Lang::Rust,
            b"fn checks() { assert_eq!(alpha(2), 2); }\n",
        ),
    ];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let ids = find_symbols_by_name("alpha", &ingest, &interner);
    assert_eq!(graph.callers(ids[0]).count(), 1);
}

#[test]
fn rust_trait_default_method_self_call_reaches_the_trait_declaration() {
    let source = "trait View {\n    fn data(&self) -> u32;\n    fn twice(&self) -> u32 { self.data() * 2 }\n}\n\
struct A;\nimpl View for A { fn data(&self) -> u32 { 1 } }\n\
struct B;\nimpl View for B { fn data(&self) -> u32 { 2 } }\n";
    let callees =
        owner_qualified_callees(&[("src/view.rs", Lang::Rust, source.as_bytes())], "twice");
    assert!(callees.contains(&"View.data".to_string()), "{callees:?}");
}

#[test]
fn rust_bare_calls_never_reach_a_type_member() {
    let source = "struct Op;\nimpl std::str::FromStr for Op {\n    type Err = String;\n    fn from_str(s: &str) -> Result<Self, Self::Err> { Err(s.to_string()) }\n}\n\
fn parse(s: &str) -> Result<Op, String> { Err(s.to_string()) }\n\
impl Op {\n    fn helper(&self) {}\n    fn run(&self) { helper(); }\n}\nfn helper() {}\n";
    let callees =
        |caller| owner_qualified_callees(&[("src/op.rs", Lang::Rust, source.as_bytes())], caller);
    assert!(callees("from_str").is_empty(), "{:?}", callees("from_str"));
    assert!(callees("parse").is_empty(), "{:?}", callees("parse"));
    assert_eq!(callees("run"), [".helper"]);
}
