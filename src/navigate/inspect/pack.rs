use super::pack_rows::{
    PackTaskBody, admit_pack_bodies, pack_body_text, write_pack_bodies, write_pack_caller,
    write_pack_sig_row, write_pack_test,
};
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::navigate::{SymbolNavInfo, find_symbols_by_name, navigate_1hop};
use crate::query::commands::QueryContext;
use crate::serialize::{PageRequest, PageWindow, XmlEscapeExt, row_bytes};
use std::io::{self, Write};

/// Shaping passes before a pack falls back to its minimal unit.
const MAX_PACK_SHAPE_PASSES: usize = 16;
/// Passes that settle a document's `est_tokens`, which is written inside the document it estimates.
const MAX_EST_PASSES: usize = 8;

pub(super) struct PackRenderEnv<'a> {
    pub(super) task: &'a str,
    pub(super) route: &'a str,
    pub(super) root: &'a str,
    pub(super) ingest: &'a IngestResult,
    pub(super) graph: &'a Graph,
    pub(super) ranks: &'a [f64],
    pub(super) interner: &'a Interner,
}

/// What one pack admits under a byte budget: the leading signature rows, the bodies of those symbols, and the
/// callers and tests windowed to the bytes left over.
struct PackPlan {
    sigs: usize,
    bodies: Vec<PackTaskBody>,
    bodies_total: usize,
    callers: Vec<SymbolNavInfo>,
    caller_window: PageWindow,
    tests: Vec<crate::gate::affected::AffectedTest>,
    test_window: PageWindow,
    surface: std::collections::BTreeSet<crate::domain::NodeId>,
}

impl PackPlan {
    /// No optional row fits: only the top-ranked signature, the minimal unit, remains.
    fn is_minimal(&self) -> bool {
        self.sigs <= 1
            && self.bodies.is_empty()
            && self.caller_window.shown == 0
            && self.test_window.shown == 0
    }
}

/// The symbols a pack ranks: the retrieved ones, or the first indexed symbol when retrieval found none.
fn pack_candidates<'a>(
    syms: &'a [serde_json::Value],
    ingest: &IngestResult,
    interner: &Interner,
) -> std::borrow::Cow<'a, [serde_json::Value]> {
    let Some(first) = ingest.symbols.first().filter(|_| syms.is_empty()) else {
        return std::borrow::Cow::Borrowed(syms);
    };
    let name = &interner[first.name];
    std::borrow::Cow::Owned(vec![serde_json::json!({
        "name": name,
        "path": ingest.sym_file_path(first, interner),
        "line": first.line,
        "kind": first.kind.tag(),
        "cx": first.cx.max(1),
        "ccx": first.ccx,
        "in": 0,
        "sig": name,
        "body": name,
    })])
}

/// Nodes of the ranked bundle symbols, matched by name, file, and line, so test candidates follow
/// the task surface rather than every symbol in the files it touches.
fn pack_seed_nodes(
    syms: &[serde_json::Value],
    ingest: &IngestResult,
    interner: &Interner,
) -> Vec<crate::domain::NodeId> {
    if syms.is_empty() {
        return ingest.symbols.first().map(|s| s.id).into_iter().collect();
    }
    syms.iter()
        .flat_map(|it| {
            let name = it["name"].as_str().unwrap_or("");
            let path = it["path"].as_str().unwrap_or("");
            let line = it["line"].as_u64().unwrap_or(1);
            find_symbols_by_name(name, ingest, interner)
                .into_iter()
                .filter(move |&id| {
                    ingest.sym(id).is_some_and(|s| {
                        u64::from(s.line) == line && ingest.sym_file_path(s, interner) == path
                    })
                })
        })
        .collect()
}

/// Callers of the top-ranked admitted symbol, the list the pack windows.
fn pack_top_callers(env: &PackRenderEnv<'_>, admitted: &[serde_json::Value]) -> Vec<SymbolNavInfo> {
    admitted
        .first()
        .and_then(|it| it["name"].as_str())
        .and_then(|n| {
            find_symbols_by_name(n, env.ingest, env.interner)
                .into_iter()
                .next()
        })
        .and_then(|id| navigate_1hop(id, env.ingest, env.graph, env.ranks, env.interner))
        .map(|nav| nav.callers)
        .unwrap_or_default()
}

/// Nodes named by the admitted symbols and the callers shown, the set the partition overlap measures.
fn pack_surface(
    env: &PackRenderEnv<'_>,
    admitted: &[serde_json::Value],
    callers: &[SymbolNavInfo],
) -> std::collections::BTreeSet<crate::domain::NodeId> {
    let mut surface = std::collections::BTreeSet::new();
    for it in admitted {
        if let Some(n) = it["name"].as_str() {
            surface.extend(find_symbols_by_name(n, env.ingest, env.interner));
        }
    }
    surface.extend(callers.iter().map(|c| c.id));
    surface
}

/// Shapes one pack to `budget` bytes: signatures take half, bodies take what the signatures leave, and callers
/// then tests take the rest, each window measured row by row so a row that does not fit is not shown.
fn plan_pack(env: &PackRenderEnv<'_>, syms: &[serde_json::Value], budget: usize) -> PackPlan {
    let sig_costs: Vec<usize> = (0..syms.len())
        .map(|i| row_bytes(|mut w| write_pack_sig_row(&mut w, i, &syms[i])))
        .collect();
    let sigs = PageRequest::default()
        .with_budget(Some(budget / 2))
        .window_fit(syms.len(), |i| sig_costs[i]);
    let admitted = &syms[..sigs.shown];
    let mut remaining = budget.saturating_sub(sig_costs[..sigs.shown].iter().sum());
    let bodies = admit_pack_bodies(admitted, &mut remaining);
    let bodies_total = syms
        .iter()
        .filter(|it| pack_body_text(it).is_some())
        .count();
    let callers = pack_top_callers(env, admitted);
    let caller_window = PageRequest::default()
        .with_budget(Some(remaining / 2))
        .window_within(callers.len(), |i| {
            row_bytes(|mut w| write_pack_caller(&mut w, &callers[i]))
        });
    let shown_callers = &callers[caller_window.begin..caller_window.end];
    remaining = remaining.saturating_sub(
        shown_callers
            .iter()
            .map(|c| row_bytes(|mut w| write_pack_caller(&mut w, c)))
            .sum(),
    );
    let surface = pack_surface(env, admitted, shown_callers);
    let seeds = pack_seed_nodes(admitted, env.ingest, env.interner);
    let changed_paths: Vec<&str> = admitted
        .iter()
        .filter_map(|it| it["path"].as_str())
        .filter(|p| !p.is_empty())
        .collect();
    let mut aff = crate::gate::affected::find_affected_tests_for_symbols(
        &seeds,
        &changed_paths,
        env.ingest,
        env.graph,
        env.interner,
    );
    crate::gate::attach_test_runners(
        &mut aff.tests,
        std::path::Path::new(env.root),
        env.ingest,
        env.interner,
    );
    let test_window = PageRequest::default()
        .with_budget(Some(remaining))
        .window_within(aff.tests.len(), |i| {
            row_bytes(|mut w| write_pack_test(&mut w, &aff.tests[i]))
        });
    PackPlan {
        sigs: sigs.shown,
        bodies,
        bodies_total,
        callers,
        caller_window,
        tests: aff.tests,
        test_window,
        surface,
    }
}

/// Writes the pack document: the ranked signatures, the admitted bodies, and the windowed callers and tests.
/// `est_tokens` is the document's own size in estimated tokens; `over_budget` appears only when it exceeds the budget.
fn write_pack_document<W: Write>(
    out: &mut W,
    env: &PackRenderEnv<'_>,
    syms: &[serde_json::Value],
    plan: &PackPlan,
    budget_tokens: usize,
    est_tokens: usize,
    include_legend: bool,
) -> io::Result<()> {
    if include_legend {
        writeln!(
            out,
            "<!-- ripwire-rs pack-task schema=ripwire-rs.pack-task/v1: one-call task bundle for task= under budget_tokens=. -->"
        )?;
    }
    let over = if est_tokens > budget_tokens {
        " over_budget=\"1\""
    } else {
        ""
    };
    write!(
        out,
        "<ctx schema=\"ripwire-rs.pack-task/v1\" task=\"{}\" route=\"{}\" root=\"{}\" est_tokens=\"{est_tokens}\" budget_tokens=\"{budget_tokens}\"{over}><sigs>",
        env.task.xml_escaped(),
        env.route.xml_escaped(),
        env.root
    )?;
    for (idx, it) in syms[..plan.sigs].iter().enumerate() {
        write_pack_sig_row(out, idx, it)?;
    }
    let bodies_capped = usize::from(plan.bodies.len() < plan.bodies_total);
    write!(
        out,
        "</sigs><bodies shown=\"{}\" total=\"{}\" capped=\"{bodies_capped}\">",
        plan.bodies.len(),
        plan.bodies_total.max(plan.bodies.len())
    )?;
    write_pack_bodies(out, &plan.bodies)?;
    write!(
        out,
        "</bodies><callers of_top=\"1\" shown=\"{}\" total=\"{}\" capped=\"{}\">",
        plan.caller_window.shown,
        plan.caller_window.total,
        usize::from(plan.caller_window.capped)
    )?;
    for c in &plan.callers[plan.caller_window.begin..plan.caller_window.end] {
        write_pack_caller(out, c)?;
    }
    write!(
        out,
        "</callers><tests shown=\"{}\" total=\"{}\" capped=\"{}\">",
        plan.test_window.shown,
        plan.test_window.total,
        usize::from(plan.test_window.capped)
    )?;
    for t in &plan.tests[plan.test_window.begin..plan.test_window.end] {
        write_pack_test(out, t)?;
    }
    write!(out, "</tests></ctx>")?;
    if include_legend {
        writeln!(out)?;
    }
    Ok(())
}

/// The pack document with its own `est_tokens` fixed point: the estimate is the size of the text it sits in.
fn pack_document(
    env: &PackRenderEnv<'_>,
    syms: &[serde_json::Value],
    plan: &PackPlan,
    budget_tokens: usize,
    include_legend: bool,
) -> Vec<u8> {
    let mut est = 0usize;
    let mut doc = Vec::new();
    for _ in 0..MAX_EST_PASSES {
        doc.clear();
        let _ = write_pack_document(
            &mut doc,
            env,
            syms,
            plan,
            budget_tokens,
            est,
            include_legend,
        );
        let next = crate::serialize::est_tokens(doc.len());
        if next == est {
            break;
        }
        est = next;
    }
    doc
}

/// Writes one pack shaped to `budget_tokens`: it shrinks the plan until the document fits, or until only the
/// minimal unit (the top signature) remains, in which case `over_budget=1` discloses the overrun.
pub(super) fn write_pack_task_ctx<W: Write>(
    env: &PackRenderEnv<'_>,
    syms: &[serde_json::Value],
    budget_tokens: usize,
    include_legend: bool,
    out: &mut W,
) -> io::Result<std::collections::BTreeSet<crate::domain::NodeId>> {
    let syms = pack_candidates(syms, env.ingest, env.interner);
    let limit = crate::serialize::budget_bytes(budget_tokens);
    let mut budget = limit;
    let mut passes = 0usize;
    let (doc, plan) = loop {
        let plan = plan_pack(env, &syms, budget);
        let doc = pack_document(env, &syms, &plan, budget_tokens, include_legend);
        passes += 1;
        if doc.len() <= limit || plan.is_minimal() {
            break (doc, plan);
        }
        budget = if passes < MAX_PACK_SHAPE_PASSES {
            budget.saturating_sub(doc.len() - limit)
        } else {
            0
        };
    };
    out.write_all(&doc)?;
    Ok(plan.surface)
}

pub fn execute_pack_task<W: Write>(
    task: &str,
    ctx: &QueryContext<'_>,
    budget_tokens: usize,
    out: &mut W,
) -> io::Result<()> {
    let for_val = crate::query::retrieval::call_for_task_with_options(
        ctx.ingest,
        ctx.interner,
        &ctx.pr.ranks,
        task,
        crate::query::retrieval::ForTaskOptions {
            root: ctx.root,
            sources: Some(ctx.sources),
            graph: Some(ctx.graph),
            detail: Some(3),
            adaptive: false,
            at: ctx.at,
            ..Default::default()
        },
    );
    let empty = Vec::new();
    let syms = for_val["symbols"].as_array().unwrap_or(&empty);
    let env = PackRenderEnv {
        task,
        route: for_val["route"].as_str().unwrap_or_default(),
        root: ctx.root,
        ingest: ctx.ingest,
        graph: ctx.graph,
        ranks: &ctx.pr.ranks,
        interner: ctx.interner,
    };
    let _ = write_pack_task_ctx(&env, syms, budget_tokens, true, out)?;
    Ok(())
}
