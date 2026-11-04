use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

/// A bare `length(X)` is the auto-imported `erlang:length/1`; another module's `length/1` is no callee.
#[test]
fn erlang_bare_bif_calls_do_not_bind_to_other_modules() {
    let store = b"-module(store).\n-export([build/1]).\nbuild(Path) ->\n    N = length(Path),\n    util:helper(N),\n    local_helper(N).\nlocal_helper(X) -> X.\n".as_slice();
    let util =
        b"-module(util).\n-export([helper/1, length/1]).\nhelper(X) -> X.\nlength(X) -> X.\n"
            .as_slice();
    let callees = |caller| {
        owner_qualified_callees(
            &[
                ("src/store.erl", Lang::Erlang, store),
                ("src/util.erl", Lang::Erlang, util),
            ],
            caller,
        )
    };
    assert_eq!(callees("build"), [".helper", ".local_helper"]);
}

/// A module that defines its own function named like a BIF calls its own definition.
#[test]
fn erlang_local_definition_shadowing_a_bif_still_binds() {
    let store = b"-module(store).\n-export([build/1, length/1]).\nbuild(Path) ->\n    length(Path).\nlength(X) -> X.\n"
        .as_slice();
    let callees =
        |caller| owner_qualified_callees(&[("src/store.erl", Lang::Erlang, store)], caller);
    assert_eq!(callees("build"), [".length"]);
}
