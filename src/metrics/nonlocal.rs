use crate::domain::{Binding, IngestResult, Interner, NodeId, RefRole, Reference, SymKind};
use crate::graph::Graph;
use crate::resolve::{RefOutcome, ResolutionContext, ResolutionIndex, ResolutionLookup};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

/// An accessed nonlocal state cell: a mutable global, or a constant whose contents the corpus mutates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCellAccess {
    pub cell_name: String,
    pub path: String,
    pub line: u32,
    pub is_write: bool,
}

/// Function with nonlocal state accesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonlocalFunction {
    pub sym: NodeId,
    pub sym_name: String,
    pub path: String,
    pub line: u32,
    pub writes: usize,
    pub reads: usize,
    pub cells: Vec<StateCellAccess>,
}

struct StateContext<'a> {
    ingest: &'a IngestResult,
    interner: &'a Interner,
    graph: &'a Graph,
    lookup: ResolutionContext<'a>,
    references: HashMap<NodeId, Vec<&'a Reference>>,
    mutated: HashSet<NodeId>,
}

impl StateContext<'_> {
    /// Whether a binding's reads and writes are state. A `const` is state only when its contents are mutated.
    fn holds_state(&self, id: NodeId, binding: Binding) -> bool {
        match binding {
            Binding::Mutable => true,
            Binding::Const => self.mutated.contains(&id),
            Binding::Frozen => false,
        }
    }

    fn cells(&self, sym_id: NodeId) -> Vec<StateCellAccess> {
        use crate::domain::CorpusView;
        let view = (self.ingest, self.interner);
        let mut targets: Vec<(NodeId, bool)> = self
            .graph
            .callees(sym_id)
            .map(|(id, _)| (id, false))
            .collect();
        for reference in self.references.get(&sym_id).into_iter().flatten() {
            if let RefOutcome::Resolved(resolved) = self.lookup.classify_target(reference)
                && !resolved.is_ambiguous()
            {
                targets.extend(
                    resolved
                        .targets
                        .into_iter()
                        .map(|id| (id, reference.role == RefRole::Write)),
                );
            }
        }
        targets.sort_unstable();
        targets.dedup();
        targets
            .into_iter()
            .filter_map(|(id, is_write)| {
                let target = self.ingest.sym(id)?;
                let is_cell = matches!(target.kind, SymKind::Var | SymKind::Field)
                    && self.holds_state(id, target.binding);
                is_cell.then(|| StateCellAccess {
                    cell_name: view.sym_name(target).to_string(),
                    path: view.sym_path(target).to_string(),
                    line: target.line,
                    is_write,
                })
            })
            .collect()
    }
}

fn data_references(ingest: &IngestResult) -> HashMap<NodeId, Vec<&Reference>> {
    let mut by_owner: HashMap<NodeId, Vec<&Reference>> = HashMap::new();
    for reference in ingest
        .references
        .iter()
        .filter(|r| matches!(r.role, RefRole::Read | RefRole::Write))
    {
        by_owner
            .entry(reference.from_symbol)
            .or_default()
            .push(reference);
    }
    by_owner
}

/// Constant bindings the indexed files mutate: a write reference that resolves to one, which is a property
/// assignment, a `delete`, or an in-place method call on it. A passed-out object is not counted as mutated.
fn mutated_bindings(ingest: &IngestResult, lookup: &ResolutionContext<'_>) -> HashSet<NodeId> {
    ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Write)
        .filter_map(|reference| match lookup.classify_target(reference) {
            RefOutcome::Resolved(resolved) if !resolved.is_ambiguous() => Some(resolved.targets),
            _ => None,
        })
        .flatten()
        .collect()
}

pub fn find_nonlocal_state(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Vec<NonlocalFunction> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let index = ResolutionIndex::build(ingest, interner);
    let lookup = index.context(ingest, interner);
    let mutated = mutated_bindings(ingest, &lookup);
    let context = StateContext {
        ingest,
        interner,
        graph,
        lookup,
        references: data_references(ingest),
        mutated,
    };
    let mut out = Vec::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Function && sym.kind != SymKind::Method {
            continue;
        }
        let p = view.sym_path(sym);
        let cells = context.cells(sym.id);
        let writes = cells.iter().filter(|cell| cell.is_write).count();
        if !cells.is_empty() {
            out.push(NonlocalFunction {
                sym: sym.id,
                sym_name: view.sym_name(sym).to_string(),
                path: p.to_string(),
                line: sym.line,
                writes,
                reads: cells.len() - writes,
                cells,
            });
        }
    }
    out
}

pub struct NonlocalStateReport<'a> {
    pub funcs: &'a [NonlocalFunction],
    pub gauges: crate::graph::GraphGauges,
    pub page: crate::serialize::PageRequest,
}

impl NonlocalStateReport<'_> {
    fn window(&self) -> crate::serialize::PageWindow {
        self.page.window_fit(self.funcs.len(), |i| {
            crate::serialize::row_bytes(|w| write_nonlocal_fn_row(&self.funcs[i], w))
        })
    }
}

impl crate::serialize::XmlReport for NonlocalStateReport<'_> {
    crate::xml_report_meta!(
        "nonlocal_state",
        "ripwire-rs.nonlocal-state/v1",
        root: true,
        comment: "ripwire-rs nonlocal-state schema=ripwire-rs.nonlocal-state/v1: per function, the non-local state it reaches: mutable bindings, and constants the indexed files mutate."
    );
    fn graph_gauges(&self) -> Option<crate::graph::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let window = self.window();
        let cells_count: usize = self.funcs.iter().map(|f| f.cells.len()).sum();
        write!(
            out,
            " cells=\"{cells_count}\" functions=\"{}\"{} unanalyzed_langs=\"\" unanalyzed_files=\"0\"",
            self.funcs.len(),
            window.xml_attrs()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let window = self.window();
        for f in &self.funcs[window.begin..window.end] {
            write_nonlocal_fn_row(f, out)?;
        }
        Ok(())
    }
}

fn write_nonlocal_fn_row<W: Write + ?Sized>(f: &NonlocalFunction, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlEscapeExt;
    writeln!(
        out,
        "  <fn p=\"{}:{}\" n=\"{}\" writes=\"{}\" reads=\"{}\" direct_writes=\"{}\" direct_reads=\"{}\" cells_total=\"{}\">",
        f.path.xml_escaped(),
        f.line,
        f.sym_name.xml_escaped(),
        f.writes,
        f.reads,
        f.writes,
        f.reads,
        f.cells.len()
    )?;
    for c in &f.cells {
        let mode = if c.is_write { "w" } else { "r" };
        writeln!(
            out,
            "    <cell n=\"{}\" p=\"{}:{}\" dir=\"{mode}\"/>",
            c.cell_name.xml_escaped(),
            c.path.xml_escaped(),
            c.line
        )?;
    }
    writeln!(out, "  </fn>")
}

/// Serialize nonlocal state report to canonical XML format.
pub fn serialize_nonlocal_state_xml<W: Write>(
    root: &str,
    funcs: &[NonlocalFunction],
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    NonlocalStateReport {
        funcs,
        gauges: crate::graph::GraphGauges::default(),
        page: crate::serialize::PageRequest::default(),
    }
    .write_xml_report(root, out)
}

pub fn serialize_nonlocal_state_with_gauge_xml<W: Write>(
    root: &str,
    funcs: &[NonlocalFunction],
    gauges: crate::graph::GraphGauges,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    NonlocalStateReport {
        funcs,
        gauges,
        page,
    }
    .write_xml_report(root, out)
}
