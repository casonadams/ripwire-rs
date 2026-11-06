use super::harness::check_owner_callees;
use ripwire_rs::Lang;

const INHERITED: &[(&str, Lang, &[u8])] = &[
    ("src/Asserts.sol", Lang::Solidity, b"pragma solidity ^0.8.0;\n\nabstract contract Asserts {\n    function assertEq(uint256 a, uint256 b) internal pure virtual {\n        require(a == b);\n    }\n}\n\nabstract contract Base is Asserts {}\n"),
    ("test/Thing.t.sol", Lang::Solidity, b"pragma solidity ^0.8.0;\n\ncontract Helper {\n    function assertEq(bytes memory a, bytes memory b) internal pure {}\n}\n\ncontract ThingTest is Base {\n    function testIt() public pure {\n        assertEq(uint256(1), uint256(1));\n    }\n}\n"),
];

const BUILTIN: &[(&str, Lang, &[u8])] = &[
    ("src/Checks.sol", Lang::Solidity, b"pragma solidity ^0.8.0;\n\nlibrary Checks {\n    function require(bool ok) internal pure {}\n}\n"),
    ("src/Thing.sol", Lang::Solidity, b"pragma solidity ^0.8.0;\n\ncontract Thing {\n    function run(uint256 a) public pure {\n        require(a > 0);\n    }\n}\n"),
];

#[test]
fn solidity_inherited_call_binds_to_the_base_not_a_same_named_helper() {
    check_owner_callees(INHERITED, "testIt", &["Asserts.assertEq"]);
}

#[test]
fn solidity_builtin_call_does_not_bind_to_a_same_named_in_tree_function() {
    check_owner_callees(BUILTIN, "run", &[]);
}
