use super::history::{PanelHistory, iso_date};
use crate::domain::{IngestResult, Interner, NodeId, Symbol};
use crate::git::history::FileChurn;
use crate::graph::Graph;
use crate::metrics::lint_naming::{is_short_name, is_wordy_name, words};
use crate::serialize::XmlEscapeExt;
use std::collections::HashMap;

pub struct QualityFamilyContext<'a> {
    pub graph: &'a Graph,
    pub interner: &'a Interner,
    pub state_syms: Option<&'a HashMap<NodeId, StateAccess>>,
    pub history: Option<&'a PanelHistory>,
}

impl QualityFamilyContext<'_> {
    fn file_churn(&self, sym: &Symbol) -> Option<&FileChurn> {
        self.history
            .and_then(|h| h.churn.get(sym.file_id as usize))
            .and_then(Option::as_ref)
    }
}

/// Reads and writes of nonlocal state cells made by one function.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StateAccess {
    pub reads: usize,
    pub writes: usize,
}

/// Trait encapsulating an orthogonal symbol quality evidence family.
pub trait QualityEvidenceFamily: Sync {
    fn name(&self) -> &'static str;
    /// Whether the family is meaningful for this symbol's language.
    fn applies(&self, sym: &Symbol) -> bool;
    fn fires(&self, sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> bool;
    fn evidence(&self, sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String;
}

pub struct QualityFamilySpec {
    pub name: &'static str,
    pub c_only: bool,
    pub predicate: fn(&Symbol, &QualityFamilyContext<'_>) -> bool,
    pub measure: fn(&Symbol, &QualityFamilyContext<'_>) -> String,
}

impl QualityEvidenceFamily for QualityFamilySpec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn applies(&self, sym: &Symbol) -> bool {
        !self.c_only || sym.lang.semantics().supports_c_layout()
    }
    fn fires(&self, sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> bool {
        (self.predicate)(sym, ctx)
    }
    fn evidence(&self, sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String {
        (self.measure)(sym, ctx)
    }
}

fn is_structural_base(sym: &Symbol) -> bool {
    sym.ccx > 15 || sym.loc > 60 || sym.max_nest > 4
}

fn structural_measure(sym: &Symbol, _: &QualityFamilyContext<'_>) -> String {
    let humps = nonzero_attr("humps", sym.humps);
    let deep = nonzero_attr("deep", sym.deep);
    format!(
        " ccx=\"{}\" loc=\"{}\" nest=\"{}\" params=\"{}\"{humps}{deep}",
        sym.ccx, sym.loc, sym.max_nest, sym.params
    )
}

/// ` name="value"` for an informative (non-zero) measure, else nothing: present-only serialization.
fn nonzero_attr(name: &str, value: u16) -> String {
    if value == 0 {
        String::new()
    } else {
        format!(" {name}=\"{value}\"")
    }
}

fn lexical_fires(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> bool {
    let name = ctx.interner.resolve(&sym.name);
    is_short_name(name) || is_wordy_name(name)
}

fn lexical_measure(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String {
    let name = ctx.interner.resolve(&sym.name);
    format!(
        " name_len=\"{}\" name_words=\"{}\"",
        name.chars().count(),
        words(name).len()
    )
}

const LEXICAL_FAMILY: QualityFamilySpec = QualityFamilySpec {
    name: "lexical",
    c_only: false,
    predicate: lexical_fires,
    measure: lexical_measure,
};

fn confusion_fires(sym: &Symbol, _: &QualityFamilyContext<'_>) -> bool {
    sym.lang.semantics().supports_c_layout()
        && (sym.humps > 2 || (sym.max_nest >= 4 && sym.cx >= 12) || sym.ccx > 20)
}

fn confusion_measure(sym: &Symbol, _: &QualityFamilyContext<'_>) -> String {
    format!(
        " humps=\"{}\" nest=\"{}\" cx=\"{}\" ccx=\"{}\"",
        sym.humps, sym.max_nest, sym.cx, sym.ccx
    )
}

const CONFUSION_FAMILY: QualityFamilySpec = QualityFamilySpec {
    name: "confusion",
    c_only: true,
    predicate: confusion_fires,
    measure: confusion_measure,
};

fn historical_fires(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> bool {
    ctx.history
        .is_some_and(|h| ctx.file_churn(sym).is_some_and(|c| c.commits >= h.cut))
}

fn historical_measure(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String {
    let since = ctx.history.map_or_else(String::new, |h| iso_date(h.since));
    ctx.file_churn(sym).map_or_else(
        || " commits=\"0\"".to_string(),
        |c| {
            let last = iso_date(c.last_commit_epoch);
            format!(
                " commits=\"{}\" last=\"{last}\" window_since=\"{since}\"",
                c.commits
            )
        },
    )
}

const HISTORICAL_FAMILY: QualityFamilySpec = QualityFamilySpec {
    name: "historical",
    c_only: false,
    predicate: historical_fires,
    measure: historical_measure,
};

fn colocation_measure(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String {
    format!(" out=\"{}\"", ctx.graph.out_degree(sym.id))
}

const COLOCATION_FAMILY: QualityFamilySpec = QualityFamilySpec {
    name: "colocation",
    c_only: false,
    predicate: |sym, ctx| ctx.graph.out_degree(sym.id) >= 10,
    measure: colocation_measure,
};

fn state_access(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> StateAccess {
    ctx.state_syms
        .and_then(|m| m.get(&sym.id))
        .copied()
        .unwrap_or_default()
}

fn state_measure(sym: &Symbol, ctx: &QualityFamilyContext<'_>) -> String {
    let a = state_access(sym, ctx);
    format!(" reads=\"{}\" writes=\"{}\"", a.reads, a.writes)
}

const STATE_FAMILY: QualityFamilySpec = QualityFamilySpec {
    name: "state",
    c_only: false,
    predicate: |sym, ctx| ctx.state_syms.is_some_and(|m| m.contains_key(&sym.id)),
    measure: state_measure,
};

pub static PANEL_FAMILIES: &[&dyn QualityEvidenceFamily] = &[
    &QualityFamilySpec {
        name: "structural",
        c_only: false,
        predicate: |sym, _| is_structural_base(sym) || sym.params > 5,
        measure: structural_measure,
    },
    &LEXICAL_FAMILY,
    &CONFUSION_FAMILY,
    &HISTORICAL_FAMILY,
    &COLOCATION_FAMILY,
    &STATE_FAMILY,
];

pub static ENSEMBLE_FAMILIES: &[&dyn QualityEvidenceFamily] = &[
    &QualityFamilySpec {
        name: "structural",
        c_only: false,
        predicate: |sym, _| is_structural_base(sym),
        measure: structural_measure,
    },
    &QualityFamilySpec {
        name: "colocation",
        c_only: false,
        predicate: |sym, ctx| ctx.graph.out_degree(sym.id) >= 8,
        measure: colocation_measure,
    },
    &HISTORICAL_FAMILY,
    &LEXICAL_FAMILY,
    &CONFUSION_FAMILY,
];

#[must_use]
pub fn evaluate_quality_families(
    sym: &Symbol,
    ctx: &QualityFamilyContext<'_>,
    families: &[&dyn QualityEvidenceFamily],
) -> Vec<&'static str> {
    families
        .iter()
        .filter(|fam| fam.fires(sym, ctx))
        .map(|fam| fam.name())
        .collect()
}

/// Families whose analysis is unavailable for this invocation, with the reason.
pub(crate) fn unavailable_families(
    cfiles: usize,
    history: &Result<PanelHistory, String>,
) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if cfiles == 0 {
        out.push(("confusion", "confusion: C/C++ only".to_string()));
    }
    if let Err(why) = history {
        out.push(("historical", format!("historical: {why}")));
    }
    out
}

/// Families from `all` that are evaluated for this invocation.
pub(crate) fn enabled_families(
    all: &[&'static dyn QualityEvidenceFamily],
    unavailable: &[(&'static str, String)],
) -> Vec<&'static dyn QualityEvidenceFamily> {
    all.iter()
        .copied()
        .filter(|f| !unavailable.iter().any(|(name, _)| *name == f.name()))
        .collect()
}

/// Root `families`/`enabled`/`unavailable` attributes; `total` is the number of families defined.
pub(crate) fn family_attrs(
    total: usize,
    enabled: &[&str],
    unavailable: &[(&str, String)],
) -> String {
    let mut out = format!(
        " families=\"{total}\" enabled=\"{}\" enabled_n=\"{}\"",
        enabled.join(","),
        enabled.len()
    );
    if !unavailable.is_empty() {
        let names: Vec<&str> = unavailable.iter().map(|(n, _)| *n).collect();
        let why: Vec<&str> = unavailable.iter().map(|(_, w)| w.as_str()).collect();
        out.push_str(&format!(
            " unavailable=\"{}\" unavailable_why=\"{}\"",
            names.join(","),
            why.join("; ").as_str().xml_escaped()
        ));
    }
    out
}

pub(super) fn collect_nonlocal_state_info(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> (HashMap<NodeId, StateAccess>, usize, usize) {
    let funcs = crate::metrics::nonlocal::find_nonlocal_state(ingest, graph, interner);
    let cells = funcs.iter().map(|f| f.cells.len()).sum();
    let mut state_files = std::collections::HashSet::new();
    let mut state_syms: HashMap<NodeId, StateAccess> = HashMap::new();
    for f in &funcs {
        state_files.insert(f.path.as_str());
        let acc = state_syms.entry(f.sym).or_default();
        acc.reads += f.reads;
        acc.writes += f.writes;
    }
    (state_syms, state_files.len(), cells)
}
