//! `--clones` / `--min-tokens`: AST code duplication detection, paged by `--limit`/`--offset`.
use super::{Plugin, Run, SharedOption};
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct CloneArgs {
    /// Detect AST code duplication and clones (Type-1, Type-2, Type-3)
    #[arg(long)]
    pub clones: bool,

    /// Minimum token length for clone detection
    #[arg(long, default_value_t = crate::clones::DEFAULT_MIN_CLONE_TOKENS)]
    pub min_tokens: usize,
}

fn run(run: &Run<'_>, args: &CloneArgs) -> io::Result<()> {
    crate::clones::execute_clones_command(run.root, args.min_tokens, run.page())
}

pub static CLONES: Plugin<CloneArgs> = Plugin {
    name: "clones",
    options: &[SharedOption::Paged],
    selected: |args| args.clones,
    run,
    check: None,
    tools: &[],
};
