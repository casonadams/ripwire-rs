use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

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
