//! `--help-task TASK`: recommends the best ripwire verb for a natural-language task.
use super::plugin::report_command;
use crate::query::commands::QueryContext;
use std::io::{self, Write};

pub fn execute<W: Write>(query: &QueryContext<'_>, task: &str, out: &mut W) -> io::Result<()> {
    let in_repo = gix::open(query.root).is_ok();
    crate::query::task_route::write_help_task_with_facts(
        task,
        query.root,
        query.ingest,
        query.interner,
        in_repo,
        in_repo && !crate::commands::common::changes::get_working_tree_changed_files(query.root)?.is_empty(),
        out,
    )
}

report_command! {
    name: "help-task",
    args: HelpTaskArgs,
    static: HELP_TASK,
    run: query(execute),
    /// Recommend the best ripwire verb for a natural-language task
    #[arg(long = "help-task", value_name = "TASK")]
    help_task: Option<String>,
}
