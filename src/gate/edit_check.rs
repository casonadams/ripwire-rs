use crate::domain::{IngestResult, Interner, NodeId, RefRole, Reference, Symbol};
use crate::gate::contract::{ContractStatus, SignatureContract, compare_contracts};
use crate::graph::Graph;
use crate::resolve::{RefOutcome, ResolutionIndex, ResolutionLookup};
use crate::serialize::XmlEscapeExt;
use std::collections::BTreeMap;
use std::io::{self, Write};

/// Detailed site information for an incompatible call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleCallSite {
    pub line: u32,
    pub arg_count: u16,
    pub required_params: u16,
    /// Bound by name evidence alone: the call passes the argument count HEAD required.
    pub by_name: bool,
}

/// Verification result for a single 1-hop caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerCheckResult {
    pub caller_id: NodeId,
    pub caller_name: String,
    pub file_path: String,
    pub line: u32,
    pub is_incompatible: bool,
    pub sites: Vec<IncompatibleCallSite>,
}

/// Comprehensive contract and 1-hop caller compatibility report.
#[derive(Debug, Clone, PartialEq)]
pub struct EditCheckReport {
    pub symbol_name: String,
    pub file_path: String,
    pub line: u32,
    pub kind_tag: &'static str,
    pub defs: usize,
    pub status: ContractStatus,
    pub total_callers: usize,
    pub incompatible_callers: usize,
    pub callers: Vec<CallerCheckResult>,
    pub gauges: crate::graph::GraphGauges,
    pub at_stamp: String,
    /// `(offset, limit)` window over unflagged caller rows; `None` emits every row.
    pub page: Option<(usize, usize)>,
}

impl EditCheckReport {
    fn unflagged_window(&self) -> (usize, usize, usize) {
        let unflagged = self.callers.len() - self.incompatible_callers;
        match self.page {
            Some((offset, limit)) => {
                let start = offset.min(unflagged);
                (unflagged, start, limit.min(unflagged - start))
            }
            None => (unflagged, 0, unflagged),
        }
    }

    fn write_page_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let Some((offset, limit)) = self.page else {
            return Ok(());
        };
        let (total, start, shown) = self.unflagged_window();
        let end = start + shown;
        write!(
            out,
            " shown_unflagged=\"{shown}\" unflagged_capped=\"{}\" total=\"{total}\" has_more=\"{}\" next_offset=\"{end}\" offset=\"{offset}\" limit=\"{limit}\"",
            u8::from(total > limit),
            u8::from(end < total)
        )
    }
}
impl crate::serialize::XmlReport for EditCheckReport {
    crate::xml_report_meta!(
        "edit-check",
        "ripwire-rs.edit-check/v1",
        root: false,
        comment: "ripwire-rs edit-check schema=ripwire-rs.edit-check/v1: sym='s contract NOW vs HEAD."
    );
    fn graph_gauges(&self) -> Option<crate::graph::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let (_, _, shown) = self.unflagged_window();
        let at_attr = if self.at_stamp.is_empty() {
            String::new()
        } else {
            format!(" at=\"{}\"", self.at_stamp)
        };
        let est_tokens = 60 + (self.incompatible_callers + shown) * 24;
        write!(
            out,
            " sym=\"{}\" t=\"{}\" p=\"{}:{}\" status=\"{}\" defs=\"{}\" callers=\"{}\" incompatible=\"{}\"{at_attr} root=\".\" next=\"--test-gate={}\"",
            self.symbol_name.xml_escaped(),
            self.kind_tag,
            self.file_path.xml_escaped(),
            self.line,
            self.status,
            self.defs,
            self.total_callers,
            self.incompatible_callers,
            self.file_path.xml_escaped()
        )?;
        self.write_page_attrs(out)?;
        write!(out, " est_tokens=\"{est_tokens}\"")
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let (_, start, shown) = self.unflagged_window();
        let (flagged, unflagged) = self.callers.split_at(self.incompatible_callers);
        for c in flagged.iter().chain(&unflagged[start..start + shown]) {
            write_caller_row(c, out)?;
        }
        Ok(())
    }
}

fn write_caller_row<W: Write>(c: &CallerCheckResult, out: &mut W) -> io::Result<()> {
    let incomp_attr = if c.is_incompatible {
        " incompatible=\"1\""
    } else {
        ""
    };
    if c.sites.is_empty() {
        return writeln!(
            out,
            "  <c n=\"{}\" p=\"{}:{}\"{}/>",
            c.caller_name.xml_escaped(),
            c.file_path.xml_escaped(),
            c.line,
            incomp_attr
        );
    }
    writeln!(
        out,
        "  <c n=\"{}\" p=\"{}:{}\"{}>",
        c.caller_name.xml_escaped(),
        c.file_path.xml_escaped(),
        c.line,
        incomp_attr
    )?;
    for s in &c.sites {
        let via = if s.by_name { " via=\"name\"" } else { "" };
        writeln!(
            out,
            "    <site line=\"{}\" args=\"{}\" required=\"{}\"{via}/>",
            s.line, s.arg_count, s.required_params
        )?;
    }
    writeln!(out, "  </c>")
}

/// Arguments a call site must pass to a definition with `params` parameters: `recv.method(args)` does
/// not write the receiver the parameter list counts, while a path call (`Type::method(recv, args)`) does.
fn expected_arg_count(takes_self: bool, params: u16, r: &Reference) -> u16 {
    if takes_self && r.member_call && !r.path_call {
        params.saturating_sub(1)
    } else {
        params
    }
}

struct CallJudge<'a, L> {
    target: &'a Symbol,
    focus: NodeId,
    /// Parameter count at HEAD when it differs from now.
    old_params: Option<u16>,
    lookup: &'a L,
}

impl<L: ResolutionLookup> CallJudge<'_, L> {
    /// A call the resolver bound to the focus that passes the wrong argument count, or one it could
    /// not bind (the count no longer fits any definition) that passes exactly the count HEAD required.
    fn judge(&self, r: &Reference) -> Option<IncompatibleCallSite> {
        let takes_self = self.target.takes_self;
        let required = expected_arg_count(takes_self, self.target.params, r);
        if r.arg_count == required {
            return None;
        }
        let by_name = match self.lookup.classify_ref(r) {
            RefOutcome::Resolved(resolved) if resolved.targets == [self.focus] => false,
            RefOutcome::Unresolved
                if self
                    .old_params
                    .is_some_and(|old| r.arg_count == expected_arg_count(takes_self, old, r)) =>
            {
                true
            }
            _ => return None,
        };
        Some(IncompatibleCallSite {
            line: r.line,
            arg_count: r.arg_count,
            required_params: required,
            by_name,
        })
    }
}

fn caller_row(
    caller_id: NodeId,
    sites: Vec<IncompatibleCallSite>,
    ingest: &IngestResult,
    interner: &Interner,
) -> Option<CallerCheckResult> {
    let caller_sym = ingest.sym(caller_id)?;
    Some(CallerCheckResult {
        caller_id,
        caller_name: interner[caller_sym.name].to_string(),
        file_path: ingest.sym_file_path(caller_sym, interner).to_string(),
        line: caller_sym.line,
        is_incompatible: !sites.is_empty(),
        sites,
    })
}

/// The focus symbol's contract now against `head_baseline`.
fn contract_status(
    sym: &Symbol,
    sym_name: &str,
    head_baseline: Option<&SignatureContract>,
) -> ContractStatus {
    let is_public = match head_baseline {
        Some(base)
            if sym.lang.semantics().visibility_mode()
                == crate::languages::VisibilityMode::ExplicitModifier =>
        {
            base.is_public
        }
        _ => crate::gate::contract::is_symbol_name_public(sym_name, sym.lang),
    };
    let now_contract = SignatureContract {
        name: sym_name.to_string(),
        params: sym.params,
        is_public,
    };
    compare_contracts(&now_contract, head_baseline)
}

/// One row per 1-hop caller plus each caller of an unbound call that passes the count HEAD required,
/// incompatible rows first.
fn caller_rows(
    focus: NodeId,
    head_baseline: Option<&SignatureContract>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Vec<CallerCheckResult> {
    let target = &ingest[focus];
    let index = ResolutionIndex::build(ingest, interner);
    let lookup = index.context(ingest, interner);
    let judge = CallJudge {
        target,
        focus,
        old_params: head_baseline
            .map(|base| base.params)
            .filter(|&old| old != target.params),
        lookup: &lookup,
    };
    let mut sites: BTreeMap<NodeId, Vec<IncompatibleCallSite>> = BTreeMap::new();
    for r in &ingest.references {
        if r.role == RefRole::Call
            && r.arg_count_known
            && interner[r.callee_name] == interner[target.name]
            && let Some(site) = judge.judge(r)
        {
            sites.entry(r.from_symbol).or_default().push(site);
        }
    }

    let mut caller_nodes: Vec<NodeId> = graph.callers(focus).map(|(c, _)| c).collect();
    let unbound: Vec<NodeId> = sites
        .keys()
        .copied()
        .filter(|id| !caller_nodes.contains(id))
        .collect();
    caller_nodes.extend(unbound);

    let mut callers: Vec<CallerCheckResult> = caller_nodes
        .into_iter()
        .filter_map(|id| caller_row(id, sites.remove(&id).unwrap_or_default(), ingest, interner))
        .collect();
    callers.sort_by(|a, b| {
        b.is_incompatible
            .cmp(&a.is_incompatible)
            .then_with(|| a.caller_name.cmp(&b.caller_name))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    callers
}

/// Verify a symbol's contract change and check 1-hop caller compatibility.
#[must_use]
pub fn check_symbol_contract(
    focus_sym_id: NodeId,
    head_baseline: Option<&SignatureContract>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Option<EditCheckReport> {
    let sym = ingest.sym(focus_sym_id)?;
    let sym_name = interner[sym.name].to_string();
    let file_path = ingest.sym_file_path(sym, interner).to_string();

    let status = contract_status(sym, &sym_name, head_baseline);
    let callers = caller_rows(focus_sym_id, head_baseline, ingest, graph, interner);
    let incompatible_callers = callers.iter().filter(|c| c.is_incompatible).count();

    let defs = crate::navigate::find_symbols_by_name(&sym_name, ingest, interner).len();
    Some(EditCheckReport {
        symbol_name: sym_name,
        file_path,
        line: sym.line,
        kind_tag: sym.kind.tag(),
        defs,
        status,
        total_callers: callers.len(),
        incompatible_callers,
        callers,
        gauges: graph.gauges(),
        at_stamp: String::new(),
        page: None,
    })
}
