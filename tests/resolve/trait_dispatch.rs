use crate::support::symbols::{find_symbol, sorted_names};
use ripwire_rs::{EdgeProv, Graph, IngestResult, Interner, Lang, NodeId};

struct Fixture {
    ingest: IngestResult,
    interner: Interner,
    graph: Graph,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let fx = crate::support::corpus::fixture(Lang::Rust, files);
        Self {
            ingest: fx.ingest,
            interner: fx.interner,
            graph: fx.graph,
        }
    }

    fn id(&self, name: &str) -> NodeId {
        find_symbol(&self.ingest, &self.interner, |sym| sym.canonical_id, name).id
    }

    fn callees(&self, name: &str) -> Vec<String> {
        sorted_names(
            &self.ingest,
            &self.interner,
            self.graph.callees(self.id(name)).map(|(id, _)| id),
            |sym| sym.canonical_id,
        )
    }

    fn reaches(&self, from: &str, leaf: &str) -> bool {
        let mut seen = vec![false; self.ingest.symbols.len()];
        let mut queue = vec![self.id(from)];
        let mut offset = 0;
        while offset < queue.len() {
            let current = queue[offset];
            offset += 1;
            if seen[current as usize] {
                continue;
            }
            seen[current as usize] = true;
            if self
                .interner
                .resolve(&self.ingest.symbols[current as usize].name)
                == leaf
            {
                return true;
            }
            queue.extend(self.graph.callees(current).map(|(id, _)| id));
        }
        false
    }
}

/// One fixture and the `(caller, callee)` pairs that must and must not be reachable in it.
struct Case {
    name: &'static str,
    files: &'static [(&'static str, &'static str)],
    reaches: &'static [(&'static str, &'static str)],
    misses: &'static [(&'static str, &'static str)],
}

const CASES: &[Case] = &[
    Case {
        name: "concrete_dispatch_keeps_the_local_inherent_method",
        files: &[(
            "src/main.rs",
            r#"
trait Processor { fn perform(&self); }
struct Worker;
impl Worker { fn perform(&self) { inherent_end(); } }
impl Processor for Worker { fn perform(&self) { trait_end(); } }
fn inherent_end() {}
fn trait_end() {}
fn invoke(worker: &Worker) { worker.perform(); }
"#,
        )],
        reaches: &[("src/main.rs::invoke", "inherent_end")],
        misses: &[("src/main.rs::invoke", "trait_end")],
    },
    Case {
        name: "a_local_concrete_type_is_not_an_unrelated_same_named_trait",
        files: &[
            (
                "src/a.rs",
                "struct Handler; impl Handler { fn handle(&self) { local_end(); } } fn local_end() {} fn invoke(value: &Handler) { value.handle(); }",
            ),
            (
                "src/b.rs",
                "trait Handler { fn handle(&self); } struct Other; impl Handler for Other { fn handle(&self) { unrelated_end(); } } fn unrelated_end() {}",
            ),
        ],
        reaches: &[("src/a.rs::invoke", "local_end")],
        misses: &[("src/a.rs::invoke", "unrelated_end")],
    },
    Case {
        name: "qualified_local_receivers_resolve_their_indexed_owner",
        files: &[(
            "src/main.rs",
            "mod workers { pub struct Worker; impl Worker { pub fn perform(&self) { finish(); } } fn finish() {} } fn invoke(value: &workers::Worker) { value.perform(); }",
        )],
        reaches: &[("src/main.rs::invoke", "finish")],
        misses: &[],
    },
    Case {
        name: "qualified_trait_receivers_resolve_their_indexed_contract",
        files: &[(
            "src/main.rs",
            "mod contracts { pub trait Processor { fn perform(&self); } } struct Worker; impl contracts::Processor for Worker { fn perform(&self) { finish(); } } fn finish() {} fn invoke(value: &dyn contracts::Processor) { value.perform(); }",
        )],
        reaches: &[("src/main.rs::invoke", "finish")],
        misses: &[],
    },
    Case {
        name: "an_external_qualified_trait_does_not_bind_a_unique_local_trait",
        files: &[(
            "src/main.rs",
            r#"
trait Processor { fn perform(&self); }
struct Worker;
impl Processor for Worker { fn perform(&self) { local_end(); } }
struct Foreign;
impl external::Processor for Foreign { fn perform(&self) { unrelated_end(); } }
fn local_end() {}
fn unrelated_end() {}
fn invoke(value: &dyn Processor) { value.perform(); }
"#,
        )],
        reaches: &[("src/main.rs::invoke", "local_end")],
        misses: &[("src/main.rs::invoke", "unrelated_end")],
    },
    Case {
        name: "a_default_remains_a_target_when_another_implementation_overrides_it",
        files: &[(
            "src/main.rs",
            r#"
trait Processor { fn perform(&self) { default_end(); } }
struct Worker;
impl Processor for Worker { fn perform(&self) { override_end(); } }
struct Other;
impl Processor for Other {}
fn default_end() {}
fn override_end() {}
fn invoke(value: &dyn Processor) { value.perform(); }
"#,
        )],
        reaches: &[
            ("src/main.rs::invoke", "default_end"),
            ("src/main.rs::invoke", "override_end"),
        ],
        misses: &[],
    },
    Case {
        name: "separate_trait_impl_blocks_for_one_subject_do_not_merge_method_bodies",
        files: &[(
            "src/main.rs",
            r#"
trait First { fn perform(&self); }
trait Second { fn perform(&self); }
struct Worker;
impl First for Worker { fn perform(&self) { first_end(); } }
impl Second for Worker { fn perform(&self) { second_end(); } }
fn first_end() {}
fn second_end() {}
fn invoke(value: &dyn First) { value.perform(); }
"#,
        )],
        reaches: &[("src/main.rs::invoke", "first_end")],
        misses: &[("src/main.rs::invoke", "second_end")],
    },
    Case {
        name: "blanket_generic_subjects_are_scoped_to_their_actual_impl_blocks",
        files: &[(
            "src/main.rs",
            r#"
trait First { fn write(&self); }
trait Second { fn write(&self); }
trait Format { fn format(&self); }
trait Render { fn format(&self); }
impl<T: First> Format for T { fn format(&self) { self.write(); } }
impl<T: Second> Render for T { fn format(&self) { self.write(); } }
struct Worker;
impl First for Worker { fn write(&self) { first_end(); } }
struct Other;
impl Second for Other { fn write(&self) { second_end(); } }
fn first_end() {}
fn second_end() {}
fn invoke(value: &dyn Format) { value.format(); }
"#,
        )],
        reaches: &[("src/main.rs::invoke", "first_end")],
        misses: &[("src/main.rs::invoke", "second_end")],
    },
    Case {
        name: "concrete_subjects_receive_applicable_blanket_implementations",
        files: &[(
            "src/main.rs",
            r#"
trait Report { fn write(&self); }
trait Text { fn text(&self); }
impl<T: Report> Text for T { fn text(&self) { self.write(); } }
struct Row;
impl Report for Row { fn write(&self) { finish(); } }
fn finish() {}
fn invoke(value: &Row) { value.text(); }
"#,
        )],
        reaches: &[("src/main.rs::invoke", "finish")],
        misses: &[],
    },
    Case {
        name: "proven_cross_file_receivers_can_call_members_with_standard_library_names",
        files: &[
            (
                "src/lib.rs",
                "mod other; use other::Worker; fn invoke(value: &Worker) { value.len(); }",
            ),
            (
                "src/other.rs",
                "pub struct Worker; impl Worker { pub fn len(&self) { finish(); } } fn finish() {}",
            ),
        ],
        reaches: &[("src/lib.rs::invoke", "finish")],
        misses: &[],
    },
    Case {
        name: "dyn_dispatch_ignores_a_foreign_inherent_method_with_the_trait_name",
        files: &[
            (
                "src/lib.rs",
                "mod other; trait Handler { fn handle(&self); } struct Actual; impl Handler for Actual { fn handle(&self) { actual_end(); } } fn actual_end() {} fn invoke(x: &dyn Handler) { x.handle(); }",
            ),
            (
                "src/other.rs",
                "struct Handler; impl Handler { fn handle(&self) { foreign_end(); } } fn foreign_end() {}",
            ),
        ],
        reaches: &[("src/lib.rs::invoke", "actual_end")],
        misses: &[("src/lib.rs::invoke", "foreign_end")],
    },
    Case {
        name: "root_qualified_trait_impl_in_the_same_file_is_a_dispatch_target",
        files: &[(
            "src/main.rs",
            "trait Processor { fn finish(&self); } struct Worker; impl crate::Processor for Worker { fn finish(&self) { finish_end(); } } fn finish_end() {} fn invoke(p: &dyn Processor) { p.finish(); }",
        )],
        reaches: &[("src/main.rs::invoke", "finish_end")],
        misses: &[],
    },
    Case {
        name: "root_qualified_trait_impl_in_another_file_is_a_dispatch_target",
        files: &[
            (
                "src/lib.rs",
                "mod other; trait Processor { fn finish(&self); } fn invoke(p: &dyn Processor) { p.finish(); }",
            ),
            (
                "src/other.rs",
                "struct Worker; impl crate::Processor for Worker { fn finish(&self) { other_end(); } } fn other_end() {}",
            ),
        ],
        reaches: &[("src/lib.rs::invoke", "other_end")],
        misses: &[],
    },
    Case {
        name: "same_named_concrete_types_in_other_files_keep_their_own_trait_impls",
        files: &[
            (
                "src/main.rs",
                "mod a; mod b; fn invoke(value: &a::Worker) { value.run(); }",
            ),
            (
                "src/a.rs",
                "trait Processor { fn run(&self); } struct Worker; impl Processor for Worker { fn run(&self) { a_end(); } } fn a_end() {}",
            ),
            (
                "src/b.rs",
                "trait Other { fn run(&self); } struct Worker; impl Other for Worker { fn run(&self) { b_end(); } } fn b_end() {}",
            ),
        ],
        reaches: &[("src/main.rs::invoke", "a_end")],
        misses: &[("src/main.rs::invoke", "b_end")],
    },
    Case {
        name: "external_trait_impl_on_a_local_type_is_a_method_target",
        files: &[(
            "src/main.rs",
            "struct Writer; impl std::io::Write for Writer { fn write(&mut self, buf: &[u8]) -> usize { io_end(); 0 } fn flush(&mut self) {} } fn io_end() {} fn invoke(w: &mut Writer, buf: &[u8]) { w.write(buf); }",
        )],
        reaches: &[("src/main.rs::invoke", "io_end")],
        misses: &[],
    },
    Case {
        name: "inherent_method_wins_over_an_external_trait_method_of_the_same_name",
        files: &[(
            "src/main.rs",
            "struct Writer; impl Writer { fn write(&mut self, buf: &[u8]) -> usize { inherent_end(); 0 } } impl std::io::Write for Writer { fn write(&mut self, buf: &[u8]) -> usize { io_end(); 0 } fn flush(&mut self) {} } fn inherent_end() {} fn io_end() {} fn invoke(w: &mut Writer, buf: &[u8]) { w.write(buf); }",
        )],
        reaches: &[("src/main.rs::invoke", "inherent_end")],
        misses: &[("src/main.rs::invoke", "io_end")],
    },
    Case {
        name: "an_untyped_receiver_of_an_interface_method_reaches_every_implementor",
        files: &[
            (
                "src/contract/mod.rs",
                "pub trait Command { fn options(&self); }",
            ),
            (
                "src/alpha/mod.rs",
                "struct Alpha; impl Command for Alpha { fn options(&self) { alpha_end(); } } fn alpha_end() {}",
            ),
            (
                "src/beta/mod.rs",
                "struct Beta; impl Command for Beta { fn options(&self) { beta_end(); } } fn beta_end() {}",
            ),
            (
                "src/app/mod.rs",
                "fn run(commands: Vec<Box<dyn Command>>) { for command in commands { command.options(); } }",
            ),
        ],
        reaches: &[
            ("src/app/mod.rs::run", "alpha_end"),
            ("src/app/mod.rs::run", "beta_end"),
        ],
        misses: &[],
    },
    Case {
        name: "an_untyped_receiver_does_not_reach_a_same_named_method_outside_the_interface",
        files: &[
            (
                "src/contract/mod.rs",
                "pub trait Command { fn options(&self); }",
            ),
            (
                "src/alpha/mod.rs",
                "struct Alpha; impl Command for Alpha { fn options(&self) { alpha_end(); } } fn alpha_end() {}",
            ),
            (
                "src/other/mod.rs",
                "struct Config; impl Config { fn options(&self) { config_end(); } } fn config_end() {}",
            ),
            (
                "src/app/mod.rs",
                "fn run(commands: Vec<Box<dyn Command>>) { for command in commands { command.options(); } }",
            ),
        ],
        reaches: &[],
        misses: &[("src/app/mod.rs::run", "config_end")],
    },
    Case {
        name: "an_extension_trait_on_an_external_type_is_reached_through_a_typed_receiver",
        files: &[
            (
                "src/serialize/mod.rs",
                "pub trait JsonExt { fn u64_or(&self, d: u64) -> u64; }\nimpl JsonExt for serde_json::Value { fn u64_or(&self, d: u64) -> u64 { json_end(); d } }\nfn json_end() {}",
            ),
            (
                "src/other/mod.rs",
                "struct Config;\nimpl Config { fn u64_or(&self, d: u64) -> u64 { config_end(); d } }\nfn config_end() {}",
            ),
            (
                "src/app/mod.rs",
                "use serde_json::Value;\nfn probe(args: &Value) { args.u64_or(1); }",
            ),
        ],
        reaches: &[("src/app/mod.rs::probe", "json_end")],
        misses: &[("src/app/mod.rs::probe", "config_end")],
    },
    Case {
        name: "an_inherent_impl_on_a_macro_declared_type_is_reached_through_a_typed_receiver",
        files: &[
            (
                "src/languages/mod.rs",
                "macro_rules! langs { () => { pub enum Lang { A } } }\nlangs!();\nimpl Lang { pub fn semantics(self) -> u8 { lang_end(); 0 } }\nfn lang_end() {}",
            ),
            (
                "src/other/mod.rs",
                "struct Other;\nimpl Other { fn semantics(&self) -> u8 { other_end(); 1 } }\nfn other_end() {}",
            ),
            (
                "src/app/mod.rs",
                "fn probe(lang: Lang) { lang.semantics(); }",
            ),
        ],
        reaches: &[("src/app/mod.rs::probe", "lang_end")],
        misses: &[("src/app/mod.rs::probe", "other_end")],
    },
    Case {
        name: "an_imported_type_name_wins_over_a_same_named_type_elsewhere",
        files: &[
            (
                "src/graph/mod.rs",
                "pub struct Graph;\nimpl Graph { pub fn callees(&self) { graph_end(); } }\nfn graph_end() {}",
            ),
            ("tests/git/history.rs", "type Graph = Vec<u8>;"),
            (
                "src/walk/mod.rs",
                "struct Walker;\nimpl Walker { fn callees(&self) { walker_end(); } }\nfn walker_end() {}",
            ),
            (
                "src/commands/around.rs",
                "use crate::graph::Graph;\nfn probe(graph: &Graph) { graph.callees(); }",
            ),
        ],
        reaches: &[("src/commands/around.rs::probe", "graph_end")],
        misses: &[("src/commands/around.rs::probe", "walker_end")],
    },
    Case {
        name: "method_syntax_never_reaches_a_function_without_a_self_receiver",
        files: &[
            (
                "tests/metrics/lint.rs",
                "fn count(scan: &u8) -> usize { count_end(); 0 }\nfn count_end() {}",
            ),
            (
                "src/gate/arch.rs",
                "pub struct Rules;\nimpl Rules { pub fn parse(content: &str) -> Self { parse_end(); Rules } }\nfn parse_end() {}",
            ),
            (
                "src/app/mod.rs",
                "fn by_method(v: Vec<u8>, s: &str) { v.iter().count(); s.parse(); }\nfn by_path(s: &str) { Rules::parse(s); }",
            ),
        ],
        reaches: &[("src/app/mod.rs::by_path", "parse_end")],
        misses: &[
            ("src/app/mod.rs::by_method", "count_end"),
            ("src/app/mod.rs::by_method", "parse_end"),
        ],
    },
];

#[test]
fn receiver_types_decide_which_trait_and_inherent_targets_are_reached() {
    for case in CASES {
        let fixture = Fixture::new(case.files);
        for (from, leaf) in case.reaches {
            assert!(
                fixture.reaches(from, leaf),
                "{}: {from} should reach {leaf}",
                case.name
            );
        }
        for (from, leaf) in case.misses {
            assert!(
                !fixture.reaches(from, leaf),
                "{}: {from} should not reach {leaf}",
                case.name
            );
        }
    }
}

#[test]
fn external_constructor_receivers_do_not_bind_unrelated_local_methods() {
    let fixture = Fixture::new(&[(
        "src/main.rs",
        r#"
struct Ingest;
impl Ingest { fn file(&self, path: &str) {} }
struct Build;
impl Build { fn file(&self, path: &str) {} }
fn invoke() { let build = cc::Build::new(); build.file("path"); }
"#,
    )]);
    assert_eq!(fixture.callees("src/main.rs::invoke"), [] as [String; 0]);
}

#[test]
fn trait_default_calls_reach_the_implementation_and_its_callee() {
    let fixture = Fixture::new(&[(
        "src/main.rs",
        r#"
trait Processor { fn perform(&self); fn run(&self) { self.perform(); } }
struct Worker;
impl Processor for Worker { fn perform(&self) { finish(); } }
fn finish() {}
fn invoke(processor: &dyn Processor) { processor.run(); }
fn entry() { invoke(&Worker); }
"#,
    )]);
    assert_eq!(
        fixture.callees("src/main.rs::invoke"),
        ["src/main.rs::Processor::run"]
    );
    assert_eq!(
        fixture.callees("src/main.rs::Processor::run"),
        [
            "src/main.rs::Processor::perform",
            "src/main.rs::Worker::perform"
        ]
    );
    assert!(fixture.reaches("src/main.rs::entry", "finish"));
}

#[test]
fn dynamic_dispatch_does_not_discard_implementations_in_other_files() {
    let fixture = Fixture::new(&[
        (
            "src/lib.rs",
            "mod other; trait Processor { fn perform(&self); } struct Worker; impl Processor for Worker { fn perform(&self) {} } fn invoke(value: &dyn Processor) { value.perform(); }",
        ),
        (
            "src/other.rs",
            "use crate::Processor; struct OtherWorker; impl Processor for OtherWorker { fn perform(&self) {} }",
        ),
    ]);
    assert_eq!(
        fixture.callees("src/lib.rs::invoke"),
        [
            "src/lib.rs::Processor::perform",
            "src/lib.rs::Worker::perform",
            "src/other.rs::OtherWorker::perform"
        ]
    );
    assert!(
        fixture
            .graph
            .callee_edges(fixture.id("src/lib.rs::invoke"))
            .all(|edge| edge.prov == EdgeProv::Split)
    );
}

#[test]
fn duplicate_trait_names_in_separate_modules_remain_isolated() {
    let fixture = Fixture::new(&[
        (
            "src/a.rs",
            "trait Processor { fn perform(&self); } struct Worker; impl Processor for Worker { fn perform(&self) {} } fn invoke(value: &dyn Processor) { value.perform(); }",
        ),
        (
            "src/b.rs",
            "trait Processor { fn perform(&self); } struct OtherWorker; impl Processor for OtherWorker { fn perform(&self) {} }",
        ),
    ]);
    assert_eq!(
        fixture.callees("src/a.rs::invoke"),
        ["src/a.rs::Processor::perform", "src/a.rs::Worker::perform"]
    );
}
