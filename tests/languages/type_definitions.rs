use super::harness::{GraphProtocolCase, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn named_type_definitions_are_indexed_as_their_kind() {
    let cases = [
        GraphProtocolCase {
            label: "kotlin-typealias",
            files: &[(
                "src/Names.kt",
                Lang::Kotlin,
                b"typealias Name = String\nfun greet(n: Name): Int = n.length\n",
            )],
            expected_syms: &[
                sym("Name", SymKind::Var, None, false),
                sym("greet", SymKind::Function, None, true),
            ],
            ..Default::default()
        },
        GraphProtocolCase {
            label: "scala-enum",
            files: &[(
                "src/Color.scala",
                Lang::Scala,
                b"enum Color {\n  case Red, Green\n}\nclass Shade {\n  def tint(): Int = 1\n}\n",
            )],
            expected_syms: &[
                sym("Color", SymKind::Var, None, false),
                sym("Shade", SymKind::Class, None, true),
            ],
            ..Default::default()
        },
    ];
    for case in &cases {
        verify_protocol_case(case);
    }
}
