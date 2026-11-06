use super::harness::check_owner_callees;
use crate::support::callees::callee_file_names;
use ripwire_rs::Lang;

#[test]
fn test_lua_graph_protocol() {
    assert!(Lang::Lua.semantics().is_std_or_builtin_member("insert"));
}

const MODULE_TABLE: &[(&str, Lang, &[u8])] = &[
    ("lua/options.lua", Lang::Lua, b"local options = {}\nfunction options.error(msg)\n  print(msg)\nend\nfunction options.run()\n  options.error(\"bad\")\nend\nreturn options\n"),
    ("lua/other.lua", Lang::Lua, b"local other = {}\nfunction other.error(msg)\n  print(msg)\nend\nreturn other\n"),
];

const GLOBAL_BUILTIN: &[(&str, Lang, &[u8])] = &[
    (
        "lua/shape.lua",
        Lang::Lua,
        b"local shape = {}\nfunction shape.type(obj)\n  return obj\nend\nreturn shape\n",
    ),
    (
        "lua/check.lua",
        Lang::Lua,
        b"local function check(x)\n  return type(x)\nend\nreturn check\n",
    ),
];

const PL_URL: &[u8] = b"local url = {}\nfunction url.quote(s)\n    return s\nend\nreturn url\n";
const PL_PRETTY: &[u8] =
    b"local pretty = {}\nfunction pretty.quote(v)\n    return v\nend\nreturn pretty\n";

/// One case: a label, the files as `(path, source)`, and the callees of `run` it must have, as `file:name`.
type Case<'a> = (&'a str, &'a [(&'a str, &'a [u8])], &'a [&'a str]);

/// `local x = require "mod"` binds `x` to the file `mod` names: a dotted module binds beside a same-named function,
/// a trailing field binds no module, and a module two files provide binds neither.
#[test]
fn lua_require_binds_the_module_its_local_names() {
    let cases: &[Case<'_>] = &[
        (
            "a dotted module binds beside a same-named function",
            &[
                (
                    "tests/test-url.lua",
                    b"local url = require 'pl.url'\nlocal pretty = require('pl.pretty')\nfunction run()\n    url.quote('a')\n    pretty.quote('b')\nend\n",
                ),
                ("lua/pl/url.lua", PL_URL),
                ("lua/pl/pretty.lua", PL_PRETTY),
            ],
            &["lua/pl/pretty.lua:quote", "lua/pl/url.lua:quote"],
        ),
        (
            "a trailing field binds no module",
            &[
                (
                    "tests/test-url.lua",
                    b"local quote = require 'pl.url' . quote\nfunction run()\n    quote.quote('a')\nend\n",
                ),
                ("lua/pl/url.lua", PL_URL),
                ("lua/pl/pretty.lua", PL_PRETTY),
            ],
            &[],
        ),
        (
            "a module that two files provide binds neither",
            &[
                (
                    "tests/test-url.lua",
                    b"local url = require 'pl.url'\nfunction run()\n    url.quote('a')\nend\n",
                ),
                ("lua/pl/url.lua", PL_URL),
                ("vendor/pl/url.lua", PL_URL),
            ],
            &[],
        ),
    ];
    for (label, sources, expected) in cases {
        let files: Vec<(&str, Lang, &[u8])> = sources
            .iter()
            .map(|(path, source)| (*path, Lang::Lua, *source))
            .collect();
        assert_eq!(callee_file_names(&files, "run"), *expected, "{label}");
    }
}

#[test]
fn lua_module_table_call_binds_to_the_table_that_defines_it() {
    check_owner_callees(MODULE_TABLE, "run", &["options.error"]);
}

#[test]
fn lua_bare_builtin_call_does_not_bind_to_a_module_function_in_another_file() {
    check_owner_callees(GLOBAL_BUILTIN, "check", &[]);
}
