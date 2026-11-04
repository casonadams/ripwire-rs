use crate::support::nonlocal::{function, state_report as support_state_report};
use crate::support::state::assert_reads_are_not_state;
use ripwire_rs::Lang;
use ripwire_rs::metrics::nonlocal::NonlocalFunction;

const MODULE: &str = "src/module.py";

fn state_report(files: &[(&str, &str)]) -> Vec<NonlocalFunction> {
    support_state_report(Lang::Python, files)
}

const CONSTANTS: &str = "from collections import namedtuple
LIMIT = 30
LABEL = \"x\"
ON = True
NOTHING = None
NEGATIVE = -1
KINDS = (\"a\", \"b\")
Outcome = namedtuple(\"Outcome\", \"reason rw\")
";

const READS_CONSTANTS: &str =
    "from module import LIMIT, LABEL, ON, NOTHING, NEGATIVE, KINDS, Outcome
def run():
    return LIMIT + NEGATIVE, LABEL, ON, NOTHING, KINDS, Outcome(None, None)
";

#[test]
fn reads_of_python_constants_and_type_factories_are_not_state() {
    assert_reads_are_not_state(
        Lang::Python,
        &[(MODULE, CONSTANTS), ("src/use.py", READS_CONSTANTS)],
    );
}

const SAME_MODULE_MUTABLE: &str = "CACHE = []
COUNT = 0
def remember(item):
    CACHE.append(item)
def bump():
    global COUNT
    COUNT = COUNT + 1
def read_count():
    return COUNT
";

#[test]
fn python_globals_rebound_by_their_own_module_are_state() {
    let report = state_report(&[(MODULE, SAME_MODULE_MUTABLE)]);
    let remember = function(&report, "remember").expect("remember mutates CACHE");
    assert!(remember.cells.iter().any(|c| c.cell_name == "CACHE"));
    let read = function(&report, "read_count").expect("COUNT is rebound by a global statement");
    assert!(
        read.cells
            .iter()
            .any(|c| c.cell_name == "COUNT" && !c.is_write)
    );
}
