//! `--edit-payload`: executes a versioned JSON edit plan or payload with the safety checks.
use super::common::safety::{context_of, report_exit};
use super::common::safety_queries::execute_edit_plan_dispatch;
use super::{Plugin, Run};
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct EditPayloadArgs {
    /// Execute JSON edit payload: a file path, or the inline JSON when no file has that name
    #[arg(long = "edit-payload", value_name = "PAYLOAD_JSON")]
    pub edit_payload: Option<String>,
    /// Validate edits without mutating files
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    /// Apply atomic code mutations
    #[arg(long = "apply")]
    pub apply: bool,
    /// Force edits bypassing safety checks
    #[arg(long = "force")]
    pub force: bool,
    /// Target file for symbol edit disambiguation
    #[arg(long = "edit-target-file", value_name = "FILE")]
    pub edit_target_file: Option<String>,
    /// Skip post-edit contract and syntax verification
    #[arg(long = "no-post-check")]
    pub no_post_check: bool,
    /// Allow atomic symbol edits when connected over remote transport
    #[arg(long = "allow-remote-edits")]
    pub allow_remote_edits: bool,
}

fn run(run: &Run<'_>, args: &EditPayloadArgs) -> io::Result<()> {
    let Some(payload) = args.edit_payload.as_deref() else {
        return Ok(());
    };
    report_exit(run, |ctx, out| {
        let safety = context_of(ctx);
        execute_edit_plan_dispatch(payload, &safety, args.dry_run, out).map(Some)
    })
}

pub static EDIT_PAYLOAD: Plugin<EditPayloadArgs> = Plugin {
    name: "edit-payload",
    options: &[],
    selected: |args| args.edit_payload.is_some(),
    run,
    check: None,
    tools: &[],
};
