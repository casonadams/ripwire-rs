use crate::domain::test_path::is_test_path;
use crate::domain::{CorpusView, IngestResult, Interner, NodeId, SymKind};
use crate::gate::affected::{AffectedReport, AffectedTest};

/// What a change obliges before merge: tests to run and impacted symbols no test reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestGateSummary {
    /// Symbols that transitively call the change, seeds excluded (file-scope owners included).
    pub impacted: usize,
    /// Impacted non-test symbols no test reaches, cognitive complexity descending.
    pub untested: Vec<NodeId>,
    /// Untested `<file-scope>` owners, counted apart because they cannot be called by a test.
    pub untested_modscope: usize,
    /// Leading test rows with the most direct evidence (changed, partner, or one hop; else nearest hops).
    pub run_first: usize,
}

#[must_use]
pub fn summarize_test_gate(
    report: &AffectedReport,
    tested: &[bool],
    ingest: &IngestResult,
    interner: &Interner,
) -> TestGateSummary {
    let view = (ingest, interner);
    let mut untested = Vec::new();
    let mut untested_modscope = 0;
    for &id in &report.impacted {
        let Some(sym) = ingest.sym(id) else {
            continue;
        };
        if tested.get(id as usize).copied().unwrap_or(false) || is_test_path(view.sym_path(sym)) {
            continue;
        }
        if sym.kind == SymKind::ModuleScope {
            untested_modscope += 1;
        } else {
            untested.push(id);
        }
    }
    untested.sort_by(|&a, &b| {
        let (sa, sb) = (&ingest[a], &ingest[b]);
        sb.ccx
            .cmp(&sa.ccx)
            .then_with(|| view.sym_path(sa).cmp(view.sym_path(sb)))
            .then_with(|| sa.line.cmp(&sb.line))
    });
    TestGateSummary {
        impacted: report.impacted.len(),
        untested,
        untested_modscope,
        run_first: run_first(&report.tests),
    }
}

fn run_first(tests: &[AffectedTest]) -> usize {
    let direct = tests
        .iter()
        .take_while(|t| t.is_direct_change || t.is_partner || t.hops == 1)
        .count();
    if direct > 0 {
        return direct;
    }
    tests.first().map_or(0, |first| {
        tests.iter().take_while(|t| t.hops == first.hops).count()
    })
}
