use crate::domain::{IngestResult, Interner, NO_NODE, RefRole, Reference, ScipSite, ScipTarget};
use crate::resolve::{RefOutcome, ResolutionIndex, ResolutionLookup, ScipIndex};
use crate::serialize::escape::escape_xml_write;
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SiteVerdict {
    Exact,
    SplitHit,
    Wrong,
    Missed,
    ExternalAgreed,
    FalseEdge,
    Unindexed,
}

impl SiteVerdict {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::SplitHit => "split-hit",
            Self::Wrong => "wrong",
            Self::Missed => "missed",
            Self::ExternalAgreed => "external-agreed",
            Self::FalseEdge => "false-edge",
            Self::Unindexed => "unindexed",
        }
    }

    #[must_use]
    pub fn verdict(scip: ScipTarget, outcome: &RefOutcome) -> Self {
        match (scip, outcome) {
            (ScipTarget::Def(target), RefOutcome::Resolved(r)) if r.targets == [target] => {
                Self::Exact
            }
            (ScipTarget::Def(target), RefOutcome::Resolved(r)) if r.targets.contains(&target) => {
                Self::SplitHit
            }
            (ScipTarget::Def(_), RefOutcome::Resolved(_)) => Self::Wrong,
            (ScipTarget::Def(_), _) => Self::Missed,
            (ScipTarget::External, RefOutcome::Resolved(_)) => Self::FalseEdge,
            (ScipTarget::External, _) => Self::ExternalAgreed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScoredSite {
    pub verdict: SiteVerdict,
    pub why: &'static str,
    pub reference: usize,
    pub ours: Vec<u32>,
    pub scip: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScipEvalReport {
    pub calls: usize,
    pub covered: usize,
    pub counts: [usize; 7],
    pub why_counts: std::collections::BTreeMap<(SiteVerdict, &'static str), usize>,
    pub sites: Vec<ScoredSite>,
}

#[must_use]
pub fn outcome_label(outcome: &RefOutcome) -> &'static str {
    match outcome {
        RefOutcome::Resolved(resolved) => resolved.prov.label(),
        RefOutcome::Ignored => "ignored",
        RefOutcome::Unresolved => "unresolved",
        RefOutcome::External => "external",
        RefOutcome::Declined => "declined",
        RefOutcome::Builtin => "builtin",
    }
}

impl ScipEvalReport {
    #[must_use]
    pub fn count(&self, verdict: SiteVerdict) -> usize {
        self.counts[verdict as usize]
    }

    #[must_use]
    pub fn precision(&self) -> Option<f64> {
        let hits = self.count(SiteVerdict::Exact) + self.count(SiteVerdict::SplitHit);
        let bound = hits + self.count(SiteVerdict::Wrong) + self.count(SiteVerdict::FalseEdge);
        ratio(hits, bound)
    }

    #[must_use]
    pub fn recall(&self) -> Option<f64> {
        let hits = self.count(SiteVerdict::Exact) + self.count(SiteVerdict::SplitHit);
        let truth = hits + self.count(SiteVerdict::Wrong) + self.count(SiteVerdict::Missed);
        ratio(hits, truth)
    }
}

fn ratio(num: usize, den: usize) -> Option<f64> {
    (den > 0).then(|| num as f64 / den as f64)
}

fn is_scored_call(r: &Reference) -> bool {
    r.from_symbol != NO_NODE && matches!(r.role, RefRole::Call | RefRole::Macro | RefRole::Access)
}

#[must_use]
pub fn evaluate_against_scip(
    ingest: &IngestResult,
    interner: &Interner,
    scip_sites: &[ScipSite],
    documented: &std::collections::HashSet<u32>,
) -> ScipEvalReport {
    let truth: ScipIndex = scip_sites
        .iter()
        .map(|s| ((s.file_id, s.line, s.callee), s.target))
        .collect();
    let index = ResolutionIndex::build(ingest, interner);
    let ctx = index.context(ingest, interner);
    let mut report = ScipEvalReport::default();
    for (i, r) in ingest
        .references
        .iter()
        .enumerate()
        .filter(|(_, r)| is_scored_call(r))
    {
        report.calls += 1;
        let Some(&scip) = truth.get(&(r.file_id, r.name_line, r.callee_name)) else {
            continue;
        };
        report.covered += 1;
        let outcome = ctx.classify_ref(r);
        let mut verdict = SiteVerdict::verdict(scip, &outcome);
        if verdict == SiteVerdict::FalseEdge
            && let RefOutcome::Resolved(resolved) = &outcome
            && resolved
                .targets
                .iter()
                .all(|&id| !documented.contains(&ingest.symbols[id as usize].file_id))
        {
            verdict = SiteVerdict::Unindexed;
        }
        let why = outcome_label(&outcome);
        report.counts[verdict as usize] += 1;
        *report.why_counts.entry((verdict, why)).or_default() += 1;
        report.sites.push(ScoredSite {
            verdict,
            why,
            reference: i,
            ours: match outcome {
                RefOutcome::Resolved(resolved) => resolved.targets,
                _ => Vec::new(),
            },
            scip: match scip {
                ScipTarget::Def(id) => Some(id),
                ScipTarget::External => None,
            },
        });
    }
    report.sites.sort_unstable();
    report
}

const ALL_VERDICTS: [SiteVerdict; 7] = [
    SiteVerdict::Exact,
    SiteVerdict::SplitHit,
    SiteVerdict::Wrong,
    SiteVerdict::Missed,
    SiteVerdict::ExternalAgreed,
    SiteVerdict::FalseEdge,
    SiteVerdict::Unindexed,
];

const SAMPLED: [SiteVerdict; 4] = [
    SiteVerdict::Wrong,
    SiteVerdict::Missed,
    SiteVerdict::FalseEdge,
    SiteVerdict::Unindexed,
];

fn write_pct<W: Write>(out: &mut W, name: &str, value: Option<f64>) -> io::Result<()> {
    value.map_or(Ok(()), |v| write!(out, " {name}=\"{v:.4}\""))
}

fn write_sym<W: Write>(
    out: &mut W,
    ingest: &IngestResult,
    interner: &Interner,
    id: u32,
) -> io::Result<()> {
    let sym = &ingest.symbols[id as usize];
    escape_xml_write(out, interner.resolve(&sym.name))?;
    write!(out, "@")?;
    escape_xml_write(out, ingest.file_path(sym.file_id, interner))?;
    write!(out, ":{}", sym.line)
}

fn write_sym_list_attr<W: Write>(
    out: &mut W,
    ingest: &IngestResult,
    interner: &Interner,
    name: &str,
    ids: &[u32],
) -> io::Result<()> {
    let Some((first, rest)) = ids.split_first() else {
        return Ok(());
    };
    write!(out, " {name}=\"")?;
    write_sym(out, ingest, interner, *first)?;
    for &id in rest.iter().take(2) {
        write!(out, ",")?;
        write_sym(out, ingest, interner, id)?;
    }
    write!(out, "\"")
}

fn write_sample<W: Write>(
    out: &mut W,
    ingest: &IngestResult,
    interner: &Interner,
    site: &ScoredSite,
) -> io::Result<()> {
    let r = &ingest.references[site.reference];
    write!(
        out,
        "<site v=\"{}\" why=\"{}\" p=\"",
        site.verdict.label(),
        site.why
    )?;
    escape_xml_write(out, ingest.file_path(r.file_id, interner))?;
    write!(out, ":{}\" callee=\"", r.name_line)?;
    escape_xml_write(out, interner.resolve(&r.callee_name))?;
    write!(out, "\"")?;
    write_sym_list_attr(out, ingest, interner, "scip", site.scip.as_slice())?;
    write_sym_list_attr(out, ingest, interner, "ours", &site.ours)?;
    if site.ours.len() > 3 {
        write!(out, " ours_n=\"{}\"", site.ours.len())?;
    }
    write!(out, "/>")
}

pub fn serialize_scip_eval_xml<W: Write>(
    report: &ScipEvalReport,
    ingest: &IngestResult,
    interner: &Interner,
    samples: usize,
    out: &mut W,
) -> io::Result<()> {
    write!(
        out,
        "<!-- ripwire-rs scip-eval schema=ripwire-rs.scip-eval/v1: name-based resolution scored against a SCIP index at the call sites it pins. calls= call refs considered; covered= pinned by SCIP. precision= in-tree bindings naming SCIP's target (exact or one arm of a split); recall= SCIP in-tree targets reached. unindexed= edges SCIP calls external into files it holds no document for, left out of precision. <why v= why= n=>: sites of a disagreeing verdict by the resolver's answer (an edge's provenance, or declined|unresolved|external|builtin|ignored). <site v= why= p= callee= scip= ours=>: sampled disagreements per verdict and reason. -->"
    )?;
    write!(
        out,
        "<scip-eval schema=\"ripwire-rs.scip-eval/v1\" calls=\"{}\" covered=\"{}\"",
        report.calls, report.covered
    )?;
    for verdict in ALL_VERDICTS {
        write!(out, " {}=\"{}\"", verdict.label(), report.count(verdict))?;
    }
    write_pct(out, "precision", report.precision())?;
    write_pct(out, "recall", report.recall())?;
    write!(out, " samples=\"{samples}\">")?;
    for (&(verdict, why), n) in &report.why_counts {
        if SAMPLED.contains(&verdict) {
            write!(
                out,
                "<why v=\"{}\" why=\"{why}\" n=\"{n}\"/>",
                verdict.label()
            )?;
        }
    }
    let disagreeing = report.sites.iter().filter(|s| SAMPLED.contains(&s.verdict));
    for group in disagreeing
        .collect::<Vec<_>>()
        .chunk_by(|a, b| (a.verdict, a.why) == (b.verdict, b.why))
    {
        for site in group.iter().take(samples) {
            write_sample(out, ingest, interner, site)?;
        }
    }
    writeln!(out, "</scip-eval>")
}
