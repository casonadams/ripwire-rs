//! `--verify CLAIM`: formally checks a code relationship claim such as `calls(A,B)`.
use super::common::retrieval::report_text_query;
use super::{Plugin, Run};
use crate::query::commands::QueryContext;
use crate::serialize::PageRequest;
use clap::Args;
use std::io::{self, Write};

/// `--verify`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct VerifyArgs {
    /// Formally verify a code relationship claim (e.g. calls(A,B))
    #[arg(long = "verify", value_name = "CLAIM")]
    pub verify: Option<String>,
}

pub fn execute<W: Write>(query: &QueryContext<'_>, claim: &str, page: PageRequest, out: &mut W) -> io::Result<()> {
    let v_ctx = crate::navigate::VerifyContext::from(query);
    crate::navigate::evaluate_verify_claim(claim, &v_ctx, page, out)
}

fn run(run: &Run<'_>, args: &VerifyArgs) -> io::Result<()> {
    report_text_query(run, args.verify.as_deref(), |query, claim, page, out| {
        execute(query, claim, page, out)
    })
}

pub static VERIFY: Plugin<VerifyArgs> = Plugin {
    name: "verify",
    options: &[],
    selected: |args| args.verify.is_some(),
    run,
    check: None,
    tools: &[],
};
