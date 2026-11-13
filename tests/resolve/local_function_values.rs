use crate::support::callees::callee_file_names;
use ripwire_rs::Lang;

const DEFINES_DELAY: &[u8] =
    b"const delay = ms => ms * 2;\nasync function run() { return delay(1); }\n";
const PARAMETER_NAMED_DELAY: &[u8] = b"function run(delay) { return delay(1); }\n";
const LIB: &[u8] = b"function delay(ms) { return ms; }\n";

#[test]
fn a_function_valued_const_is_a_definition_not_a_shadowing_variable() {
    let files = [("a.js", Lang::JavaScript, DEFINES_DELAY)];
    assert_eq!(callee_file_names(&files, "run"), ["a.js:delay"]);
}

#[test]
fn a_parameter_named_like_a_definition_elsewhere_still_binds_nothing() {
    let files = [
        ("a.js", Lang::JavaScript, PARAMETER_NAMED_DELAY),
        ("lib.js", Lang::JavaScript, LIB),
    ];
    assert!(callee_file_names(&files, "run").is_empty());
}
