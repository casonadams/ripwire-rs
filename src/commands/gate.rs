//! `gate` subcommand: pre-commit verification gates scoped to a repository path.
//! The gate flags are owned by their command modules and reused here through flatten.
use super::affected::{AffectedArgs, run_affected};
use super::clones::CloneArgs;
use super::common::gate::validate_gate_modifiers;
use super::edit_check::{EditCheckArgs, run_edit_check};
use super::quality_baseline::{QualityBaselineArgs, QualityBaselineFlags, run_quality_baseline};
use super::quality_delta::{QualityDeltaArgs, QualityDeltaFlags, run_quality_delta};
use super::{Command, Run, SharedOption, decode_subcommand};
use crate::cli::Cli;
use clap::{Args, Command as ClapCommand};
use std::io;

const PAGED: &[SharedOption] = &[SharedOption::Paged];

#[derive(Args, Debug, Default, PartialEq)]
pub struct GateArgs {
    /// Target repository path
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    #[command(flatten)]
    pub edit_check: EditCheckArgs,

    #[command(flatten)]
    pub affected: AffectedArgs,

    #[command(flatten)]
    pub quality_baseline: QualityBaselineArgs,

    #[command(flatten)]
    pub quality_delta: QualityDeltaArgs,

    #[command(flatten)]
    pub clones: CloneArgs,
}

/// The top-level gate flags, decoded once, that the `gate` subcommand falls back on.
#[derive(Default)]
pub struct TopGate {
    pub edit: EditCheckArgs,
    pub affected: AffectedArgs,
    pub baseline: QualityBaselineFlags,
    pub delta: QualityDeltaFlags,
}

impl TopGate {
    fn decode(cli: &Cli) -> Self {
        Self {
            edit: cli.plugins.decode().unwrap_or_default(),
            affected: cli.plugins.decode().unwrap_or_default(),
            baseline: cli.plugins.decode().unwrap_or_default(),
            delta: cli.plugins.decode().unwrap_or_default(),
        }
    }
}

/// What the `gate` subcommand was asked for: its own flags merged with the top-level gate flags.
#[derive(Debug, PartialEq, Eq)]
pub struct GateRequest<'a> {
    pub edit_check: Option<&'a str>,
    pub affected: Option<&'a str>,
    pub quality_baseline: bool,
    pub quality_delta: bool,
    pub clones: bool,
}

#[must_use]
pub fn merge_request<'a>(args: &'a GateArgs, top: &'a TopGate) -> GateRequest<'a> {
    GateRequest {
        edit_check: args.edit_check.edit_check.as_deref().or(top.edit.edit_check.as_deref()),
        affected: args.affected.affected.as_deref().or(top.affected.affected.as_deref()),
        quality_baseline: args.quality_baseline.quality_baseline
            || top.baseline.primary.quality_baseline,
        quality_delta: args.quality_delta.requested()
            || top.delta.primary.requested()
            || top.delta.opts.quality_ack.is_some(),
        clones: args.clones.clones,
    }
}

/// The gate the `gate` subcommand runs, in dispatch precedence.
#[derive(Debug, PartialEq, Eq)]
pub enum GateChoice<'a> {
    EditCheck(&'a str),
    Affected(&'a str),
    QualityBaseline,
    QualityDelta,
    Clones,
}

/// Picks the gate: edit-check, affected, quality-baseline, quality-delta, then clones.
#[must_use]
pub fn select_gate<'a>(request: &GateRequest<'a>) -> Option<GateChoice<'a>> {
    if let Some(symbol) = request.edit_check {
        Some(GateChoice::EditCheck(symbol))
    } else if let Some(specs) = request.affected {
        Some(GateChoice::Affected(specs))
    } else if request.quality_baseline {
        Some(GateChoice::QualityBaseline)
    } else if request.quality_delta {
        Some(GateChoice::QualityDelta)
    } else {
        request.clones.then_some(GateChoice::Clones)
    }
}

impl GateChoice<'_> {
    fn run(self, run: &Run<'_>, top: &TopGate, args: &GateArgs) -> io::Result<()> {
        match self {
            Self::EditCheck(symbol) => run_edit_check(run, symbol),
            Self::Affected(specs) => run_affected(run, specs),
            repo_wide => repo_wide.run_repo_wide(run, top, args),
        }
    }

    fn run_repo_wide(&self, run: &Run<'_>, top: &TopGate, args: &GateArgs) -> io::Result<()> {
        let min_tokens = args.clones.min_tokens;
        match self {
            Self::QualityBaseline => run_quality_baseline(run, top.baseline.opts.allow_dirty),
            Self::QualityDelta => {
                let base = args.quality_delta.base().or(top.delta.primary.base());
                run_quality_delta(run, &top.delta.opts, base)
            }
            _ => crate::clones::execute_clones_command(run.root, min_tokens, run.page()),
        }
    }
}

fn execute_gate_subcommand(cli: &Cli, default_root: &str, args: &GateArgs) -> io::Result<()> {
    let top = TopGate::decode(cli);
    let request = merge_request(args, &top);
    validate_gate_modifiers(
        top.baseline.opts.allow_dirty,
        request.quality_baseline,
        top.delta.opts.show_stale,
        request.quality_delta,
    )?;
    let root = args.path.as_deref().unwrap_or(default_root);
    let run = Run::new(cli, root, PAGED);
    select_gate(&request).map_or(Ok(()), |gate| gate.run(&run, &top, args))
}

pub struct Gate;

pub static GATE: Gate = Gate;

impl Command for Gate {
    fn name(&self) -> &'static str {
        "gate"
    }

    fn options(&self) -> &'static [SharedOption] {
        PAGED
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            GateArgs::augment_args(ClapCommand::new("gate"))
                .mut_arg("quality_delta", |arg| {
                    arg.help("Measure quality delta against baseline, or the merge base of =REV")
                })
                .about("Pre-commit verification gates (edit-check, affected, quality-delta)"),
        ]
    }

    fn run_subcommand(
        &self,
        cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        root: &str,
    ) -> Option<io::Result<()>> {
        if name != "gate" {
            return None;
        }
        Some(decode_subcommand::<GateArgs>(matches).and_then(|args| {
            execute_gate_subcommand(cli, root, &args)
        }))
    }
}
