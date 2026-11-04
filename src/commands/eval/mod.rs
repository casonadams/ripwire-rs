//! Self-evaluation behind `--eval=KIND`: `cochange` (co-change recall), `retrieval` (known-item retrieval)
//! and `scip` (call resolution scored against the `--scip` index, loaded without that overlay).
use super::{Command, SharedOption};
use crate::cli::Cli;
use crate::commands::context::{crawler_options_for_cli, load_repo_analysis_for_cli};
use clap::{Args, Command as ClapCommand};
use std::io;
use std::path::Path;

/// Evaluation flags.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct EvalFlags {
    /// Self-evaluation: cochange, retrieval, or scip (scores call resolution against --scip)
    #[arg(long = "eval", value_name = "KIND", value_parser = ["cochange", "retrieval", "scip"])]
    pub eval: Option<String>,
    /// Sampled disagreements per verdict and reason for --eval=scip
    #[arg(long = "samples", value_name = "N", default_value_t = 10)]
    pub samples: usize,
}

pub struct Eval;
pub static EVAL: Eval = Eval;

impl Command for Eval {
    fn name(&self) -> &'static str {
        "eval"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        EvalFlags::augment_args(cmd)
    }

    fn run(&self, cli: &Cli, root: &str) -> Option<io::Result<()>> {
        let flags = cli.plugins.decode::<EvalFlags>()?;
        let kind = flags.eval.as_deref()?;
        Some(match kind {
            "cochange" => {
                let mut stdout = io::stdout().lock();
                crate::metrics::run_cochange_eval(&mut stdout)
            }
            "retrieval" => (|| {
                let (ingest, _graph, pr, interner) = load_repo_analysis_for_cli(cli, root, false)?;
                let mut stdout = io::stdout().lock();
                crate::metrics::run_retrieval_eval_for_repo(
                    &ingest,
                    &interner,
                    &pr.ranks,
                    &mut stdout,
                )
            })(),
            _ => run_scip_eval(cli, root, flags.samples),
        })
    }
}

fn run_scip_eval(cli: &Cli, root: &str, samples: usize) -> io::Result<()> {
    let path = cli.scip.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "ripwire-rs: --eval=scip scores against a SCIP index: pass --scip=FILE",
        )
    })?;
    let bytes = std::fs::read(path)
        .map_err(|e| io::Error::new(e.kind(), format!("ripwire-rs: --scip {path}: {e}")))?;
    let (ingest, _graph, _pr, interner) = load_analysis_without_scip(cli, root)?;
    let (sites, documented) = crate::resolve::scip::indexed_sites(&bytes, &ingest, &interner)
        .map_err(|e| io::Error::other(format!("ripwire-rs: --scip {path}: {e}")))?;
    let report = crate::metrics::scip_eval::evaluate_against_scip(
        &ingest,
        &interner,
        &sites,
        &documented,
    );
    crate::metrics::scip_eval::serialize_scip_eval_xml(
        &report,
        &ingest,
        &interner,
        samples,
        &mut io::stdout().lock(),
    )
}

/// The analysis the SCIP index is scored against: resolved without the index overlaid on it.
fn load_analysis_without_scip(
    cli: &Cli,
    root: &str,
) -> io::Result<(
    crate::domain::IngestResult,
    crate::graph::Graph,
    crate::rank::PageRankResult,
    crate::domain::Interner,
)> {
    let cache_path = cli.modifiers.cache.as_deref().map(Path::new);
    let opts = crawler_options_for_cli(cli);
    crate::workspace::load_repo_analysis_with_options(
        root,
        cache_path,
        cli.modifiers.no_cache,
        &opts,
        None,
    )
}
