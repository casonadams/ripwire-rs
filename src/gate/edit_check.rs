use crate::domain::{IngestResult, Interner, NodeId, RefRole, Reference, Symbol};
use crate::gate::contract::{ContractStatus, SignatureContract, compare_contracts};
use crate::graph::Graph;
use crate::serialize::XmlEscapeExt;
use std::io::{self, Write};

/// Detailed site information for an incompatible call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleCallSite {
    pub line: u32,
    pub arg_count: u16,
    pub required_params: u16,
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
    pub gauges: crate::serialize::GraphGauges,
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
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
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
        writeln!(
            out,
            "    <site line=\"{}\" args=\"{}\" required=\"{}\"/>",
            s.line, s.arg_count, s.required_params
        )?;
    }
    writeln!(out, "  </c>")
}

/// Arguments a call site must pass to `target`: `recv.method(args)` does not write the receiver the
/// definition's parameter list counts, while a path call (`Type::method(recv, args)`) does.
fn expected_arg_count(target: &Symbol, r: &Reference) -> u16 {
    if target.takes_self && r.member_call && !r.path_call {
        target.params.saturating_sub(1)
    } else {
        target.params
    }
}

fn check_caller_compatibility(
    caller_id: NodeId,
    sym_name: &str,
    target: &Symbol,
    ingest: &IngestResult,
    interner: &Interner,
) -> CallerCheckResult {
    let caller_sym = &ingest[caller_id];
    let caller_name = interner[caller_sym.name].to_string();
    let file_path = ingest.sym_file_path(caller_sym, interner).to_string();

    let mut sites = Vec::new();
    for r in &ingest.references {
        if r.from_symbol == caller_id && r.role == RefRole::Call {
            let callee = &interner[r.callee_name];
            let required = expected_arg_count(target, r);
            if callee == sym_name && r.arg_count_known && r.arg_count != required {
                sites.push(IncompatibleCallSite {
                    line: r.line,
                    arg_count: r.arg_count,
                    required_params: required,
                });
            }
        }
    }

    let is_incompatible = !sites.is_empty();
    CallerCheckResult {
        caller_id,
        caller_name,
        file_path,
        line: caller_sym.line,
        is_incompatible,
        sites,
    }
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

    let is_public = match head_baseline {
        Some(base)
            if sym.lang.semantics().visibility_mode()
                == crate::languages::VisibilityMode::ExplicitModifier =>
        {
            base.is_public
        }
        _ => crate::gate::contract::is_symbol_name_public(&sym_name, sym.lang),
    };
    let now_contract = SignatureContract {
        name: sym_name.clone(),
        params: sym.params,
        is_public,
    };
    let status = compare_contracts(&now_contract, head_baseline);

    let caller_nodes: Vec<NodeId> = graph.callers(focus_sym_id).map(|(c, _)| c).collect();
    let total_callers = caller_nodes.len();

    let mut callers = Vec::with_capacity(total_callers);
    let mut incompatible_callers = 0usize;

    for caller_id in caller_nodes {
        let res = check_caller_compatibility(caller_id, &sym_name, sym, ingest, interner);
        if res.is_incompatible {
            incompatible_callers += 1;
        }
        callers.push(res);
    }

    callers.sort_by(|a, b| {
        b.is_incompatible
            .cmp(&a.is_incompatible)
            .then_with(|| a.caller_name.cmp(&b.caller_name))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    let defs = crate::navigate::find_symbols_by_name(&sym_name, ingest, interner).len();
    Some(EditCheckReport {
        symbol_name: sym_name,
        file_path,
        line: sym.line,
        kind_tag: sym.kind.tag(),
        defs,
        status,
        total_callers,
        incompatible_callers,
        callers,
        gauges: graph.gauges(),
        at_stamp: String::new(),
        page: None,
    })
}
