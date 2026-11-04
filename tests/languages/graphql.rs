use super::harness::{GraphProtocolCase, resolved_edges, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_graphql_fragments_types_and_references() {
    let files: &[(&str, Lang, &[u8])] = &[(
        "schema.graphql",
        Lang::Graphql,
        b"type User implements Node {\n  id: ID!\n  orders(first: Int): [Order!]!\n}\ninterface Node {\n  id: ID!\n}\ntype Order {\n  id: ID!\n  total: Float\n}\nfragment UserFields on User {\n  id\n}\nquery GetUser($id: ID!) {\n  user(id: $id) {\n    ...UserFields\n  }\n}\nenum Role {\n  ADMIN\n  VIEWER\n}\ninput NewUser {\n  role: Role\n}\nscalar DateTime\n",
    )];
    verify_protocol_case(&GraphProtocolCase {
        label: "graphql-fragments",
        files,
        expected_syms: &[
            sym("UserFields", SymKind::Function, None, false),
            sym("Role", SymKind::Class, None, false),
            sym("NewUser", SymKind::Class, None, false),
            sym("DateTime", SymKind::Class, None, false),
            sym("Order", SymKind::Class, None, false),
        ],
        rejected_syms: &["ADMIN"],
        ..Default::default()
    });
    // Type edges (implements, field types, type conditions, input fields) are not call-graph callees, so they
    // are asserted on the resolved edge set. The built-in scalars `ID`/`Float` name no edge.
    assert_eq!(
        resolved_edges(files),
        [
            ("GetUser".into(), "UserFields".into()),
            ("NewUser".into(), "Role".into()),
            ("User".into(), "Node".into()),
            ("UserFields".into(), "User".into()),
            ("orders".into(), "Order".into()),
        ]
    );
}
