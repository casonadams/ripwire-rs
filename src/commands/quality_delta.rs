//! `--quality-delta` and its acknowledgment modifiers: reports regressions vs baseline; MCP `quality_delta`.
use super::common::gate::{validate_cli_gate_modifiers, with_workspace};
use super::common::mcp::mcp_tool;
use super::{Plugin, Run};
use crate::gate::quality_delta::DeltaCliOptions;
use crate::query::commands::{QueryContext, render_xml};
use crate::serialize::JsonValueExt;
use clap::Args;
use serde_json::Value;
use std::io;
use std::path::Path;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct QualityDeltaArgs {
    /// Report code quality delta regressions vs baseline; `=REV` measures from the merge base of REV
    #[arg(
        long = "quality-delta",
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "REV|REV..HEAD"
    )]
    pub quality_delta: Option<String>,
}

impl QualityDeltaArgs {
    #[must_use]
    pub fn requested(&self) -> bool {
        self.quality_delta.is_some()
    }

    /// The `REV` or `REV..HEAD` spec, when one was given.
    #[must_use]
    pub fn base(&self) -> Option<&str> {
        self.quality_delta.as_deref().filter(|s| !s.is_empty())
    }
}

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct QualityDeltaOpts {
    /// Partition quality-delta by ownership scope
    #[arg(long = "scope", value_name = "SCOPE")]
    pub scope: Option<String>,
    /// Acknowledge findings in quality-delta
    #[arg(long = "quality-ack", value_name = "REASON")]
    pub quality_ack: Option<Option<String>>,
    /// Narrow quality-ack to specific kinds or facets
    #[arg(long = "ack-only", value_name = "SUBSTR")]
    pub ack_only: Option<String>,
    /// Show stale quality-delta acknowledgments even on clean runs
    #[arg(long = "show-stale")]
    pub show_stale: bool,
}

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct QualityDeltaFlags {
    #[command(flatten)]
    pub primary: QualityDeltaArgs,
    #[command(flatten)]
    pub opts: QualityDeltaOpts,
}

/// Runs the quality-delta gate; exits with status 2 when the delta holds failures.
pub fn run_quality_delta(
    run: &Run<'_>,
    opts: &QualityDeltaOpts,
    base: Option<&str>,
) -> io::Result<()> {
    let quality_ack = opts
        .quality_ack
        .as_ref()
        .map(|ack| ack.as_deref().unwrap_or("acknowledged"));
    let has_failures = with_workspace(run.cli, run.root, true, |state, out| {
        let sources = crate::query::FileSources {
            root: Path::new(&state.root),
        };
        crate::query::safety::execute_quality_delta_with_args(
            DeltaCliOptions {
                scope: opts.scope.as_deref(),
                quality_ack,
                ack_only: opts.ack_only.as_deref(),
                show_stale: opts.show_stale,
                base,
            },
            &state.query_context(&sources),
            out,
        )
    })?;
    if has_failures {
        std::process::exit(2);
    }
    Ok(())
}

fn run(run: &Run<'_>, args: &QualityDeltaFlags) -> io::Result<()> {
    validate_cli_gate_modifiers(run.cli)?;
    run_quality_delta(run, &args.opts, args.primary.base())
}

fn call_quality_delta(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let options = DeltaCliOptions {
        scope: args.opt_str("scope"),
        quality_ack: args.opt_str("quality_ack"),
        ack_only: args.opt_str("ack_only"),
        show_stale: args.bool_or("show_stale", false),
        base: args.opt_str("base"),
    };
    render_xml(|out| {
        crate::query::safety::execute_quality_delta_with_args(options, state, out).map(|_| ())
    })
}

pub static QUALITY_DELTA: Plugin<QualityDeltaFlags> = Plugin {
    name: "quality_delta",
    options: &[],
    selected: |args| {
        args.primary.requested() || args.opts.quality_ack.is_some() || args.opts.show_stale
    },
    run,
    check: None,
    tools: &[mcp_tool!(
        "quality_delta",
        "Your PR self-check: reports ONLY what your working tree made WORSE vs baseline",
        &[
            ("scope", "string", "Quality delta scope"),
            ("quality_ack", "string", "Acknowledgement text"),
            ("ack_only", "string", "Acknowledgement finding patterns"),
            ("show_stale", "boolean", "Include stale findings"),
            ("base", "string", "REV or REV..HEAD: measure from the merge base of REV instead of the baseline"),
        ],
        &[],
        call_quality_delta
    )],
};
