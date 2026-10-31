use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_proto_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "proto",
        files: &[(
            "api.proto",
            Lang::Proto,
            b"syntax = \"proto3\";\nservice AuthService {\n  rpc Login (LoginReq) returns (LoginResp);\n}\nmessage LoginReq {\n  string user = 1;\n}\n",
        )],
        expected_syms: &[
            sym("AuthService", SymKind::Class, None, false),
            sym("Login", SymKind::Function, None, false),
            sym("LoginReq", SymKind::Class, None, false),
        ],
        ..Default::default()
    });
}

/// A field's message type and an rpc's request/response types are uses: the message they name is a
/// callee of the message or rpc that mentions it.
#[test]
fn proto_message_and_rpc_type_mentions_are_edges() {
    let source = b"syntax = \"proto3\";\nmessage Item {\n  string sku = 1;\n}\nmessage Order {\n  repeated Item items = 1;\n}\nmessage CreateReq {\n  Order order = 1;\n}\nservice Orders {\n  rpc Create(CreateReq) returns (Order);\n}\n";
    let files: &[(&str, Lang, &[u8])] = &[("orders.proto", Lang::Proto, source)];
    assert_eq!(
        owner_qualified_callees(files, "Order"),
        vec![".Item".to_string()]
    );
    assert_eq!(
        owner_qualified_callees(files, "Create"),
        vec![".CreateReq".to_string(), ".Order".to_string()]
    );
}
