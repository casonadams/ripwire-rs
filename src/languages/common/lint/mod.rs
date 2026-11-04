pub(crate) mod ast;
pub(crate) mod flow;
pub(crate) mod syntax;
pub(crate) mod text;

use crate::languages::LintGroup as G;

pub(crate) const C_FAMILY_LINT_GROUPS: &[crate::languages::LintGroup] = &[
    G::Cast,
    G::Goto,
    G::DoWhile,
    G::Insecure,
    G::Parens,
    G::CFamily,
    G::SelfAssign,
    G::NativeOnly,
    G::Unreachable,
    G::Naming,
];
pub(crate) const SCRIPT_LINT_GROUPS: &[crate::languages::LintGroup] = &[
    G::DoWhile,
    G::Insecure,
    G::Parens,
    G::SelfAssign,
    G::Unreachable,
    G::Naming,
];
