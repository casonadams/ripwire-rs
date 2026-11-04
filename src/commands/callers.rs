//! `--callers SYM`: direct (1-hop) callers of a symbol.
use super::SharedOption;
use super::nav::nav_tool;
use super::plugin::report_command;
use crate::query::navigation::NavMode;

report_command! {
    name: "callers",
    args: CallersArgs,
    static: CALLERS,
    options: [
        SharedOption::Paged,
        SharedOption::Budget("Output budget in tokens"),
    ],
    tools: [
        nav_tool!(
            "callers",
            "Who calls this function or method: each call site's enclosing function, with same-named definitions elsewhere kept apart. Use before grep for call sites",
            NavMode::Callers
        ),
    ],
    run: nav(NavMode::Callers),
    /// Query 1-hop callers of a symbol
    #[arg(long, value_name = "SYM")]
    callers: Option<String>,
}
