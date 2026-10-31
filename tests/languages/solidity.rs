use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_solidity_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "solidity",
        files: &[(
            "contracts/Vault.sol",
            Lang::Solidity,
            b"interface IVault {}\ncontract Vault is IVault {\n    function deposit() public {\n        abi.encode();\n        recordDeposit();\n    }\n    function recordDeposit() internal {\n        return;\n    }\n}\n",
        )],
        expected_syms: &[
            sym("IVault", SymKind::Interface, None, false),
            sym("Vault", SymKind::Class, None, true),
            sym("deposit", SymKind::Function, Some("Vault"), true),
            sym("recordDeposit", SymKind::Function, Some("Vault"), true),
        ],
        expected_edges: &[("deposit", "recordDeposit")],
        expect_external: true,
        expected_lego: Some(("IVault", "Vault")),
        ..Default::default()
    });
}

/// `new Vault(..)` constructs the contract: the factory's call edge reaches the contract's class.
#[test]
fn solidity_new_expression_is_a_call_to_the_contract() {
    let source = b"contract Vault {\n    uint256 public total;\n}\ncontract Factory {\n    function make() public returns (Vault) {\n        return new Vault();\n    }\n}\n";
    let files: &[(&str, Lang, &[u8])] = &[("Factory.sol", Lang::Solidity, source)];
    assert_eq!(
        owner_qualified_callees(files, "make"),
        vec![".Vault".to_string()]
    );
}
