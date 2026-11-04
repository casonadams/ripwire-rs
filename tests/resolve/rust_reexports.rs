use crate::support::callees::callees_described;
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

/// `query/mod.rs` is a facade over two children, each defining a `render`. The facade re-exports one of
/// them by name, or both through a glob, so a caller reaches exactly the definitions the facade exposes.
const FACADE_NAMED: (&str, Lang, &[u8]) = (
    "src/query/mod.rs",
    Lang::Rust,
    b"mod audit;\nmod branch_tools;\npub use audit::render;\npub use branch_tools::tally;\n",
);

const FACADE_GLOB: (&str, Lang, &[u8]) = (
    "src/query/mod.rs",
    Lang::Rust,
    b"mod audit;\nmod branch_tools;\npub use audit::*;\npub use branch_tools::tally;\n",
);

const BRANCH_TOOLS: (&str, Lang, &[u8]) = (
    "src/query/branch_tools.rs",
    Lang::Rust,
    b"use super::*;\npub fn tally() { render(); }\npub fn render() {}\n",
);

const AUDIT: (&str, Lang, &[u8]) = ("src/query/audit.rs", Lang::Rust, b"pub fn render() {}\n");

#[test]
fn named_facade_reexport_binds_the_caller_to_the_re_exported_definition() {
    let files = [
        FACADE_NAMED,
        BRANCH_TOOLS,
        AUDIT,
        (
            "src/cli/dispatch.rs",
            Lang::Rust,
            b"use crate::query::render;\npub fn probe() { render(); }\n",
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/query/audit.rs::render"]
    );
}

#[test]
fn glob_facade_reexport_binds_a_caller_glob_to_the_re_exported_definition() {
    let files = [
        FACADE_GLOB,
        BRANCH_TOOLS,
        AUDIT,
        (
            "src/cli/dispatch.rs",
            Lang::Rust,
            b"use crate::query::*;\npub fn probe() { render(); }\n",
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/query/audit.rs::render"]
    );
}

#[test]
fn sibling_super_glob_binds_a_name_the_facade_glob_re_exports() {
    let sibling: (&str, Lang, &[u8]) = (
        "src/query/render_user.rs",
        Lang::Rust,
        b"use super::*;\npub fn drive() { render(); }\n",
    );
    let files = [FACADE_GLOB, BRANCH_TOOLS, AUDIT, sibling];
    assert_eq!(
        callees_described(&files, "drive", callee_at),
        ["src/query/audit.rs::render"]
    );
}

#[test]
fn same_named_items_without_an_import_path_stay_unbound() {
    let files = [
        FACADE_NAMED,
        BRANCH_TOOLS,
        AUDIT,
        (
            "src/cli/dispatch.rs",
            Lang::Rust,
            b"pub fn probe() { render(); }\n",
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        Vec::<String>::new()
    );
}

/// The blanket `ToXmlString` impl and its trait, defined inline in the facade or in a child it re-exports.
const TRAIT_INLINE: (&str, Lang, &[u8]) = (
    "src/serialize/mod.rs",
    Lang::Rust,
    b"pub trait ToXmlString { fn write_xml(&self, out: &mut Vec<u8>); }\npub trait XmlReport { fn render(&self); }\nimpl<T: XmlReport + ?Sized> ToXmlString for T { fn write_xml(&self, out: &mut Vec<u8>) { self.render(); } }\n",
);

const TRAIT_CHILD: (&str, Lang, &[u8]) = (
    "src/serialize/xml_report.rs",
    Lang::Rust,
    b"pub trait ToXmlString { fn write_xml(&self, out: &mut Vec<u8>); }\npub trait XmlReport { fn render(&self); }\nimpl<T: XmlReport + ?Sized> ToXmlString for T { fn write_xml(&self, out: &mut Vec<u8>) { self.render(); } }\n",
);

const TRAIT_FACADE: (&str, Lang, &[u8]) = (
    "src/serialize/mod.rs",
    Lang::Rust,
    b"mod xml_report;\npub use xml_report::{ToXmlString, XmlReport};\n",
);

const CALLER: (&str, Lang, &[u8]) = (
    "src/query/safety.rs",
    Lang::Rust,
    b"use crate::serialize::{ToXmlString};\nstruct Report;\nimpl crate::serialize::XmlReport for Report { fn render(&self) {} }\nfn make() -> Report { Report }\npub fn execute(out: &mut Vec<u8>) { make().write_xml(out); }\n",
);

#[test]
fn blanket_trait_method_inline_in_the_facade_resolves() {
    let files = [TRAIT_INLINE, CALLER];
    assert_eq!(
        callees_described(&files, "execute", callee_at),
        [
            "src/query/safety.rs::make",
            "src/serialize/mod.rs::write_xml"
        ]
    );
}

#[test]
fn blanket_trait_method_in_a_re_exported_child_resolves() {
    let files = [TRAIT_FACADE, TRAIT_CHILD, CALLER];
    assert_eq!(
        callees_described(&files, "execute", callee_at),
        [
            "src/query/safety.rs::make",
            "src/serialize/xml_report.rs::write_xml"
        ]
    );
}

const DOMAIN_FACADE: (&str, Lang, &[u8]) = (
    "src/domain/mod.rs",
    Lang::Rust,
    b"mod interner;\npub use interner::Interner;\n",
);

const INTERNER: (&str, Lang, &[u8]) = (
    "src/domain/interner.rs",
    Lang::Rust,
    b"pub struct Interner;\nimpl Interner {\n    pub fn get(&self, s: &str) -> Option<u8> { None }\n}\n",
);

#[test]
fn typed_receiver_through_a_facade_type_path_binds_the_inherent_method() {
    let files = [
        DOMAIN_FACADE,
        INTERNER,
        (
            "src/navigate/mod.rs",
            Lang::Rust,
            b"use crate::domain::Interner;\npub fn probe(interner: &Interner) { interner.get(\"x\"); }\n",
        ),
    ];
    assert_eq!(
        callees_described(&files, "probe", callee_at),
        ["src/domain/interner.rs::get"]
    );
}
