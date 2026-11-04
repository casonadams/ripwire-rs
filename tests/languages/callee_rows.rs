use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

fn check(files: &[(&str, Lang, &[u8])], caller: &str, expected: &[&str]) {
    assert_eq!(
        owner_qualified_callees(files, caller),
        expected,
        "callees of {caller}"
    );
}

#[test]
fn language_fixture_callees_bind_to_their_owners() {
    check(
        &[(
            "src/shapes.f90",
            Lang::Fortran,
            b"module shapes\n  implicit none\n  interface scale\n    module procedure scale_real\n  end interface scale\ncontains\n  real function scale_real(x, f)\n    real, intent(in) :: x, f\n    scale_real = x * f\n  end function scale_real\nend module shapes\n\nprogram demo\n  use shapes\n  real :: a\n  a = scale(1.0, 2.0)\nend program demo\n",
        )],
        "demo",
        &[".scale"],
    );

    check(
        &[(
            "Factory.sol",
            Lang::Solidity,
            b"contract Vault {\n    uint256 public total;\n}\ncontract Factory {\n    function make() public returns (Vault) {\n        return new Vault();\n    }\n}\n",
        )],
        "make",
        &[".Vault"],
    );

    check(
        &[(
            "src/ledger.cbl",
            Lang::Cobol,
            b"       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LEDGER.\n       PROCEDURE DIVISION.\n       MAIN-LINE.\n           PERFORM OPEN-FILES THRU CLOSE-FILES.\n           GO TO FINISH-UP.\n       OPEN-FILES.\n           DISPLAY 'OPEN'.\n       CLOSE-FILES.\n           DISPLAY 'CLOSE'.\n       FINISH-UP.\n           STOP RUN.\n",
        )],
        "MAIN-LINE",
        &[".CLOSE-FILES", ".FINISH-UP", ".OPEN-FILES"],
    );

    let orders: &[(&str, Lang, &[u8])] = &[(
        "orders.proto",
        Lang::Proto,
        b"syntax = \"proto3\";\nmessage Item {\n  string sku = 1;\n}\nmessage Order {\n  repeated Item items = 1;\n}\nmessage CreateReq {\n  Order order = 1;\n}\nservice Orders {\n  rpc Create(CreateReq) returns (Order);\n}\n",
    )];
    check(orders, "Order", &[".Item"]);
    check(orders, "Create", &[".CreateReq", ".Order"]);

    let erlang_store: &[(&str, Lang, &[u8])] = &[(
        "src/store.erl",
        Lang::Erlang,
        b"-module(store).\n-export([build/1]).\nbuild(Path) ->\n    N = length(Path),\n    util:helper(N),\n    local_helper(N).\nlocal_helper(X) -> X.\n",
    )];
    let erlang_util: &[(&str, Lang, &[u8])] = &[(
        "src/util.erl",
        Lang::Erlang,
        b"-module(util).\n-export([helper/1, length/1]).\nhelper(X) -> X.\nlength(X) -> X.\n",
    )];
    let mut both = erlang_store.to_vec();
    both.extend_from_slice(erlang_util);
    check(&both, "build", &[".helper", ".local_helper"]);
    check(
        &[(
            "src/store.erl",
            Lang::Erlang,
            b"-module(store).\n-export([build/1, length/1]).\nbuild(Path) ->\n    length(Path).\nlength(X) -> X.\n",
        )],
        "build",
        &[".length"],
    );

    let python_helpers: &[(&str, Lang, &[u8])] = &[
        (
            "scripts/helpers.py",
            Lang::Python,
            b"def parse_args():\n    return 1\n\ndef parse(text):\n    return text\n",
        ),
        (
            "scripts/app.py",
            Lang::Python,
            b"import argparse\nimport ast\nfrom store import Store\n\ndef main(store: Store):\n    parser = argparse.ArgumentParser()\n    parser.parse_args()\n    ast.parse('x')\n    store.save()\n",
        ),
        (
            "scripts/store.py",
            Lang::Python,
            b"class Store:\n    def save(self):\n        return 1\n",
        ),
    ];
    check(python_helpers, "main", &["Store.save"]);

    let python_store: &[(&str, Lang, &[u8])] = &[
        (
            "pkg/util.py",
            Lang::Python,
            b"def len(x):\n    return 0\n\ndef make_record(raw):\n    return {'raw': raw}\n",
        ),
        (
            "pkg/store.py",
            Lang::Python,
            b"from util import make_record\n\ndef build(raw):\n    n = len(raw)\n    return make_record(n)\n",
        ),
    ];
    check(python_store, "build", &[".make_record"]);

    let python_files: &[(&str, Lang, &[u8])] = &[(
        "pkg/files.py",
        Lang::Python,
        b"def open(path):\n    return path\n\ndef load(path):\n    return open(path)\n",
    )];
    check(python_files, "load", &[".open"]);

    let python_shapes: &[(&str, Lang, &[u8])] = &[(
        "pkg/shapes.py",
        Lang::Python,
        b"def dec(f):\n    return f\n\nclass Shape:\n    @property\n    def area(self):\n        return 1\n\n@dec\ndef decorated():\n    return 1\n",
    )];
    check(python_shapes, "<file-scope>", &[".dec"]);
    check(python_shapes, "area", &[]);

    check(
        &[
            (
                "src/crawler.rs",
                Lang::Rust,
                b"pub struct Config;\n\nimpl Config {\n    pub fn parse(s: &str) -> Config {\n        let _ = s;\n        Config\n    }\n}\n",
            ),
            (
                "src/lib.rs",
                Lang::Rust,
                b"pub mod crawler;\n\npub fn run() -> crawler::Config {\n    crate::crawler::Config::parse(\"y\")\n}\n",
            ),
        ],
        "run",
        &["Config.parse"],
    );
}
