mod families;
mod history;
mod render;

use crate::domain::{IngestResult, Interner, Symbol};
use crate::graph::Graph;
use crate::serialize::PageRequest;
use families::{PANEL_FAMILIES, QualityEvidenceFamily, collect_nonlocal_state_info};

pub use families::{QualityFamilyContext, evaluate_quality_families};
pub(crate) use families::{enabled_families, unavailable_families};
pub use history::PanelHistory;

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyEvidence {
    pub family: &'static str,
    /// Pre-rendered measured values, each attribute prefixed with a space.
    pub attrs: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityPanelRow {
    pub path: String,
    pub line: u32,
    pub name: String,
    pub fam: usize,
    pub of: usize,
    pub fired: String,
    pub evidence: Vec<FamilyEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityPanelReport {
    pub preset: String,
    pub cut: usize,
    pub eligible: usize,
    pub ranked: usize,
    pub below_cut: usize,
    pub no_family: usize,
    pub hranked: usize,
    pub cfiles: usize,
    pub cscope: usize,
    pub cranked: usize,
    pub sfiles: usize,
    pub sscope: usize,
    pub cells: usize,
    pub tested_scope: usize,
    pub deep_untested: usize,
    pub enabled: Vec<&'static str>,
    pub unavailable: Vec<(&'static str, String)>,
    pub history: Result<PanelHistory, String>,
    pub rows: Vec<QualityPanelRow>,
    pub page: PageRequest,
}

/// Inputs beyond the analysis graph: the preset and the git history behind the historical family.
pub struct PanelInputs<'a> {
    pub preset: Option<&'a str>,
    pub history: Result<PanelHistory, String>,
}

fn panel_row(
    sym: &Symbol,
    ingest: &IngestResult,
    interner: &Interner,
    enabled: &[&'static dyn QualityEvidenceFamily],
    ctx: &QualityFamilyContext<'_>,
    fired: &[&'static str],
) -> QualityPanelRow {
    let path = ingest
        .files
        .get(sym.file_id as usize)
        .map(|f| interner.resolve(&f.path))
        .unwrap_or("");
    let applicable: Vec<&dyn QualityEvidenceFamily> =
        enabled.iter().copied().filter(|f| f.applies(sym)).collect();
    let evidence = applicable
        .iter()
        .filter(|f| fired.contains(&f.name()))
        .map(|f| FamilyEvidence {
            family: f.name(),
            attrs: f.evidence(sym, ctx),
        })
        .collect();
    QualityPanelRow {
        path: path.to_string(),
        line: sym.line,
        name: interner.resolve(&sym.name).to_string(),
        fam: fired.len(),
        of: applicable.len(),
        fired: fired.join(","),
        evidence,
    }
}

#[derive(Default)]
struct PanelCounts {
    eligible: usize,
    tested_scope: usize,
    deep_untested: usize,
    cscope: usize,
    cranked: usize,
    hranked: usize,
    below_cut: usize,
    no_family: usize,
}

struct PanelScan<'a> {
    ingest: &'a IngestResult,
    interner: &'a Interner,
    tested_mask: &'a [bool],
    enabled: &'a [&'static dyn QualityEvidenceFamily],
    ctx: &'a QualityFamilyContext<'a>,
    cut: usize,
}

fn scan_symbols(scan: &PanelScan<'_>) -> (Vec<QualityPanelRow>, PanelCounts) {
    let mut counts = PanelCounts::default();
    let mut rows = Vec::new();
    for sym in &scan.ingest.symbols {
        if sym.kind != crate::domain::SymKind::Function
            && sym.kind != crate::domain::SymKind::Method
        {
            continue;
        }
        counts.eligible += 1;
        let is_tested = scan
            .tested_mask
            .get(sym.id as usize)
            .copied()
            .unwrap_or(false);
        counts.tested_scope += usize::from(is_tested);
        counts.deep_untested += usize::from(!is_tested && sym.ccx > 15);
        counts.cscope += usize::from(sym.lang.semantics().supports_c_layout());
        let fired = evaluate_quality_families(sym, scan.ctx, scan.enabled);
        counts.cranked += usize::from(fired.contains(&"confusion"));
        counts.hranked += usize::from(fired.contains(&"historical"));
        match fired.len() {
            0 => counts.no_family += 1,
            fam_count if fam_count < scan.cut => counts.below_cut += 1,
            _ => rows.push(panel_row(
                sym,
                scan.ingest,
                scan.interner,
                scan.enabled,
                scan.ctx,
                &fired,
            )),
        }
    }
    (rows, counts)
}

pub fn compute_quality_panel_report(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    inputs: PanelInputs<'_>,
    page: PageRequest,
) -> QualityPanelReport {
    let preset = inputs.preset.unwrap_or("default");
    let history = inputs.history;
    let cut = usize::from(preset != "lenient") + 1;
    let (state_syms, sfiles, cells) = collect_nonlocal_state_info(ingest, graph, interner);
    let sscope = state_syms.len();
    let tested_mask = crate::navigate::compute_tested_node_mask(ingest, graph, interner);
    let cfiles = ingest
        .files
        .iter()
        .filter(|f| f.lang.semantics().supports_c_layout())
        .count();
    let unavailable = unavailable_families(cfiles, &history);
    let enabled = enabled_families(PANEL_FAMILIES, &unavailable);
    let enabled_names: Vec<&'static str> = enabled.iter().map(|f| f.name()).collect();
    let ctx = QualityFamilyContext {
        graph,
        interner,
        state_syms: Some(&state_syms),
        history: history.as_ref().ok(),
    };
    let (mut rows, counts) = scan_symbols(&PanelScan {
        ingest,
        interner,
        tested_mask: &tested_mask,
        enabled: &enabled,
        ctx: &ctx,
        cut,
    });
    rows.sort_by(|a, b| b.fam.cmp(&a.fam).then_with(|| a.path.cmp(&b.path)));
    let ranked = rows.len();

    QualityPanelReport {
        preset: preset.to_string(),
        cut,
        eligible: counts.eligible,
        ranked,
        below_cut: counts.below_cut,
        no_family: counts.no_family,
        hranked: counts.hranked,
        cfiles,
        cscope: counts.cscope,
        cranked: counts.cranked,
        sfiles,
        sscope,
        cells,
        tested_scope: counts.tested_scope,
        deep_untested: counts.deep_untested,
        enabled: enabled_names,
        unavailable,
        history,
        rows,
        page,
    }
}
