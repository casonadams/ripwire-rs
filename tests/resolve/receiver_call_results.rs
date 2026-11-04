use crate::a_callees::callees_described;
use ripwire_rs::{IngestResult, Interner, Lang, NodeId};

/// A callee described by its file path and name.
fn callee_at(ingest: &IngestResult, interner: &Interner, id: NodeId) -> String {
    let sym = &ingest.symbols[id as usize];
    format!(
        "{}::{}",
        ingest.sym_file_path(sym, interner),
        interner.resolve(&sym.name)
    )
}

const PAGE: (&str, Lang, &[u8]) = (
    "src/page.rs",
    Lang::Rust,
    b"pub struct Page<'a> { pub items: &'a [u8] }\n\
      impl<'a> Page<'a> { pub fn rows(&self) -> &'a [u8] { self.items } }\n\
      pub struct Ctx { pub items: Vec<u8> }\n\
      impl Ctx {\n\
          pub fn page(&self) -> Page<'_> { Page { items: &self.items } }\n\
          pub fn probe(&self) -> usize { let page = self.page(); page.rows().len() }\n\
          pub fn chain(&self) -> usize { self.page().rows().len() }\n\
      }\n",
);

/// A second `rows`, so a call that is not typed by its receiver is ambiguous.
const OTHER_ROWS: (&str, Lang, &[u8]) = (
    "src/other.rs",
    Lang::Rust,
    b"pub struct Other;\nimpl Other { pub fn rows(&self) -> usize { 0 } }\n",
);

const LOADER: (&str, Lang, &[u8]) = (
    "src/loader.rs",
    Lang::Rust,
    b"pub struct Loader { pub n: u32 }\n\
      impl Loader {\n\
          pub fn new() -> Result<Self, String> { Ok(Loader { n: 1 }) }\n\
          pub fn get(&self, k: u32) -> u32 { k + self.n }\n\
      }\n\
      pub fn make() -> Loader { Loader { n: 2 } }\n\
      pub fn probe_free() -> u32 { let l = make(); l.get(1) }\n\
      pub fn probe_new() -> u32 { let l = Loader::new().unwrap(); l.get(1) }\n",
);

#[test]
fn a_local_bound_to_a_self_method_result_takes_the_returned_type() {
    assert_eq!(
        callees_described(&[PAGE, OTHER_ROWS], "probe", callee_at),
        ["src/page.rs::page", "src/page.rs::rows"]
    );
}

#[test]
fn a_self_method_chain_takes_the_method_return_type() {
    assert_eq!(
        callees_described(&[PAGE, OTHER_ROWS], "chain", callee_at),
        ["src/page.rs::page", "src/page.rs::rows"]
    );
}

#[test]
fn a_local_bound_to_a_free_call_result_takes_the_returned_type() {
    assert_eq!(
        callees_described(&[LOADER], "probe_free", callee_at),
        ["src/loader.rs::get", "src/loader.rs::make"]
    );
}

#[test]
fn a_local_bound_to_a_constructor_unwrapped_takes_the_constructed_type() {
    assert_eq!(
        callees_described(&[LOADER], "probe_new", callee_at),
        ["src/loader.rs::get", "src/loader.rs::new"]
    );
}

#[test]
fn a_builtin_len_on_a_std_receiver_does_not_bind_an_in_tree_len() {
    let files = [
        (
            "src/buf.rs",
            Lang::Rust,
            b"pub struct Buf { pub n: usize }\nimpl Buf { pub fn len(&self) -> usize { self.n } }\n"
                .as_slice(),
        ),
        (
            "src/probe.rs",
            Lang::Rust,
            b"use crate::buf::Buf;\npub fn probe(buf: &Buf) -> usize { let v: Vec<u8> = Vec::new(); v.len() + buf.len() }\n"
                .as_slice(),
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/buf.rs::len"]
    );
}

#[test]
fn two_same_named_methods_bind_only_the_receivers_own_type() {
    let files = [
        (
            "src/a.rs",
            Lang::Rust,
            b"pub struct A;\nimpl A { pub fn run(&self) {} }\n".as_slice(),
        ),
        (
            "src/b.rs",
            Lang::Rust,
            b"pub struct B;\nimpl B { pub fn run(&self) {} }\n".as_slice(),
        ),
        (
            "src/probe.rs",
            Lang::Rust,
            b"use crate::a::A;\nuse crate::b::B;\npub fn probe(a: &A, b: &B) { a.run(); }\n"
                .as_slice(),
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/a.rs::run"]
    );
}

#[test]
fn a_free_call_binds_the_imported_function() {
    let files = [
        (
            "src/helpers.rs",
            Lang::Rust,
            b"pub fn helper() {}\n".as_slice(),
        ),
        (
            "src/probe.rs",
            Lang::Rust,
            b"use crate::helpers::helper;\npub fn probe() { helper(); }\n".as_slice(),
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/helpers.rs::helper"]
    );
}

#[test]
fn a_call_inside_a_macro_argument_binds_the_same_file_function() {
    let files = [(
        "src/gate.rs",
        Lang::Rust,
        b"fn branches(count: u32) -> String { count.to_string() }\nfn func(body: String) -> String { body }\nfn probe() -> Vec<String> { vec![func(branches(24))] }\n"
            .as_slice(),
    )];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/gate.rs::branches", "src/gate.rs::func"]
    );
}

#[test]
fn a_same_named_function_elsewhere_does_not_hide_the_imported_one() {
    let files = [
        (
            "src/git/repo.rs",
            Lang::Rust,
            b"mod grep;\npub use grep::has_word;\n".as_slice(),
        ),
        (
            "src/git/repo/grep.rs",
            Lang::Rust,
            b"pub fn has_word(text: &str, word: &str) -> bool { text.contains(word) }\n".as_slice(),
        ),
        (
            "src/other.rs",
            Lang::Rust,
            b"pub fn has_word(text: &str, word: &str) -> bool { text == word }\n".as_slice(),
        ),
        (
            "src/probe.rs",
            Lang::Rust,
            b"use crate::git::repo::has_word;\npub fn probe_has_word() -> bool { has_word(\"hello world\", \"world\") }\n".as_slice(),
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe_has_word", callee_at),
        ["src/git/repo/grep.rs::has_word"]
    );
}
