//! `orient` subcommand: repository orientation, optionally ranked for a task.
use super::common::retrieval::report_query;
use super::{Command, Run, SharedOption, for_task, run_named_subcommand};
use crate::cli::Cli;
use crate::commands::map::dispatch_default_run;
use clap::{Args, Command as ClapCommand};
use std::io;

/// `orient` subcommand: repository orientation with an optional task description.
#[derive(Args, Debug, Default, PartialEq)]
pub struct OrientArgs {
    /// Target repository path
    #[arg(value_name = "PATH")]
    pub path: Option<String>,
    /// Task description for relevance-ranked orientation
    #[arg(long = "for", value_name = "TASK")]
    pub for_task: Option<String>,
}

pub struct Orient;
pub static ORIENT: Orient = Orient;

impl Command for Orient {
    fn name(&self) -> &'static str {
        "orient"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            OrientArgs::augment_args(ClapCommand::new("orient"))
                .about("Exploration and task-based context orientation"),
        ]
    }

    fn run_subcommand(
        &self,
        cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        root: &str,
    ) -> Option<io::Result<()>> {
        run_named_subcommand::<OrientArgs>(name, "orient", matches, |args| execute(cli, args, root))
    }
}

fn execute(cli: &Cli, args: &OrientArgs, default_root: &str) -> io::Result<()> {
    let root = args.path.as_deref().unwrap_or(default_root);
    let Some(task) = args.for_task.as_deref() else {
        return dispatch_default_run(cli, root);
    };
    report_query(&Run::new(cli, root, &[]), |query, out| {
        let val = for_task::orient_bundle(cli, query, task);
        if let Some(error) = val["error"].as_str() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, error));
        }
        crate::serialize::serialize_for_xml(&val, out)
    })
}
