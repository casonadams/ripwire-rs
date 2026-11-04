//! `metrics` subcommand: co-change coupling and maintenance hotspots for a repository path.
use super::cochange::{CochangeArgs, CochangeQuery, execute_cochange};
use super::hotspots::{HotspotsArgs, execute_hotspots};
use super::{Command, SharedOption, run_named_subcommand};
use crate::cli::Cli;
use crate::serialize::PageRequest;
use clap::{Args, Command as ClapCommand};
use std::io;

#[derive(Args, Debug, Default, PartialEq)]
pub struct MetricsArgs {
    /// Target repository path
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    #[command(flatten)]
    pub hotspots: HotspotsArgs,

    #[command(flatten)]
    pub cochange: CochangeArgs,
}

fn execute_metrics_subcommand(default_root: &str, args: &MetricsArgs) -> io::Result<()> {
    let root = args.path.as_deref().unwrap_or(default_root);
    let since = args.cochange.since.as_deref();
    if args.cochange.cochange.is_some() {
        return execute_cochange(
            root,
            CochangeQuery {
                file: args.cochange.cochange.as_ref().and_then(|file| file.as_deref()),
                since,
                recur: None,
                groups: false,
                page: PageRequest::default(),
                pretty: false,
                json_out: false,
            },
        );
    }
    if args.hotspots.hotspots {
        return execute_hotspots(root, since, false, false, PageRequest::default());
    }
    Ok(())
}

pub struct Metrics;

pub static METRICS: Metrics = Metrics;

impl Command for Metrics {
    fn name(&self) -> &'static str {
        ""
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            MetricsArgs::augment_args(ClapCommand::new("metrics"))
                .mut_arg("cochange", |arg| {
                    arg.help("Show file co-change coupling (Fowler's Shotgun Surgery)")
                })
                .mut_arg("hotspots", |arg| {
                    arg.help("Show top maintenance hotspots (churn * ccx)")
                })
                .about("Code complexity, churn, and architectural health metrics"),
        ]
    }

    fn run_subcommand(
        &self,
        _cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        root: &str,
    ) -> Option<io::Result<()>> {
        run_named_subcommand::<MetricsArgs>(name, "metrics", matches, |args| {
            execute_metrics_subcommand(root, args)
        })
    }
}
