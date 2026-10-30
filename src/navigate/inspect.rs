use super::{find_symbols_by_name, navigate_1hop};
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::query::commands::QueryContext;
use crate::serialize::{PageRequest, PageWindow, XmlEscapeExt, row_bytes};
use std::io::{self, Write};

pub fn resolve_symbol_chain_at<'a>(
    file_spec: &str,
    line_num: u32,
    ingest: &'a IngestResult,
    interner: &Interner,
) -> Vec<&'a crate::domain::Symbol> {
    let mut chain: Vec<&'a crate::domain::Symbol> = ingest
        .symbols
        .iter()
        .filter(|sym| {
            let p = interner.resolve(&ingest.files[sym.file_id as usize].path);
            sym.kind != crate::domain::SymKind::ModuleScope
                && (p == file_spec || p.ends_with(file_spec))
                && sym.spans_line(line_num)
        })
        .collect();
    chain.sort_by_key(|s| (s.line, std::cmp::Reverse(s.loc)));
    chain
}

pub fn resolve_symbol_at<'a>(
    file_spec: &str,
    line_num: u32,
    ingest: &'a IngestResult,
    interner: &Interner,
) -> Option<&'a crate::domain::Symbol> {
    resolve_symbol_chain_at(file_spec, line_num, ingest, interner)
        .into_iter()
        .last()
}

/// A `@FILE:LINE` selector resolved to the innermost definition enclosing that line.
pub struct ResolvedSeed {
    pub id: crate::domain::NodeId,
    pub name: String,
}

/// Resolve a `@FILE:LINE` seed (1-based line). Every failure names its cause; nothing is guessed.
pub fn resolve_seed(
    spec: &str,
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<ResolvedSeed, String> {
    let (file, line) = spec
        .strip_prefix('@')
        .and_then(|body| body.rsplit_once(':'))
        .and_then(|(file, line)| Some((file, line.parse::<u32>().ok().filter(|n| *n > 0)?)))
        .filter(|(file, _)| !file.is_empty())
        .ok_or_else(|| {
            format!("seed '{spec}' is malformed: expected @FILE:LINE with a 1-based line")
        })?;
    let paths: Vec<&str> = ingest
        .files
        .iter()
        .map(|f| interner.resolve(&f.path))
        .filter(|p| *p == file || p.ends_with(file))
        .collect();
    let path = match paths.as_slice() {
        [] => return Err(format!("seed '{spec}' named no indexed file")),
        [only] => *only,
        many if many.contains(&file) => file,
        many => return Err(format!("seed '{spec}' is ambiguous: {}", many.join(", "))),
    };
    let chain: Vec<_> = resolve_symbol_chain_at(path, line, ingest, interner)
        .into_iter()
        .filter(|s| interner.resolve(&ingest.files[s.file_id as usize].path) == path)
        .collect();
    let Some(&innermost) = chain.last() else {
        return Err(format!(
            "seed '{spec}': no indexed definition spans line {line}"
        ));
    };
    if let [.., prev, last] = chain.as_slice()
        && prev.line == last.line
        && prev.loc == last.loc
    {
        return Err(format!("seed '{spec}': two definitions share line {line}"));
    }
    Ok(ResolvedSeed {
        id: innermost.id,
        name: interner.resolve(&innermost.name).to_string(),
    })
}

pub fn serialize_at_xml<W: Write>(
    file_spec: &str,
    line_num: u32,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let exact_match = ingest
        .files
        .iter()
        .any(|f| interner.resolve(&f.path) == file_spec);
    let suffix_matches: Vec<_> = ingest
        .files
        .iter()
        .filter(|f| {
            let p = interner.resolve(&f.path);
            p == file_spec || p.ends_with(file_spec)
        })
        .collect();

    if suffix_matches.is_empty() {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location"
        )));
    }
    if suffix_matches.len() > 1 && !exact_match {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location (ambiguous)"
        )));
    }
    let chain = resolve_symbol_chain_at(file_spec, line_num, ingest, interner);
    let Some(&innermost) = chain.last() else {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location (no indexed symbol spans line {line_num})"
        )));
    };
    let name = interner.resolve(&innermost.name);
    let chain_len = chain.len();
    write!(
        out,
        "<!-- ripwire-rs at schema=ripwire-rs.at/v1: enclosing-definition chain at p=:l=. -->\n<at schema=\"ripwire-rs.at/v1\" p=\"{file_spec}\" l=\"{line_num}\" sym=\"{name}\" chain=\"{chain_len}\" root=\".\">",
    )?;
    for sym in chain {
        let sn = interner.resolve(&sym.name);
        let kind = sym.kind.tag();
        let end_l = sym.line.saturating_add(sym.loc.saturating_sub(1));
        write!(
            out,
            "<s n=\"{sn}\" t=\"{kind}\" l=\"{}\" el=\"{end_l}\"/>",
            sym.line
        )?;
    }
    writeln!(out, "</at>")
}

struct PackTaskBody {
    kind: String,
    line: u64,
    path_esc: String,
    name_esc: String,
    body: String,
}

/// Largest body one pack row may take; a larger body is truncated to fit its row.
const MAX_PACK_BODY_BYTES: usize = 3600;
/// A body is admitted only with at least this many bytes left: a shorter excerpt does not help the task.
const MIN_PACK_BODY_BYTES: usize = 256;
/// Marker appended to a truncated body; its bytes are part of that body's allowance.
const PACK_TRUNCATION_MARK: &str = "\n// ... [truncated to fit pack-task budget]";
/// Shaping passes before a pack falls back to its minimal unit.
const MAX_PACK_SHAPE_PASSES: usize = 16;
/// Passes that settle a document's `est_tokens`, which is written inside the document it estimates.
const MAX_EST_PASSES: usize = 8;

fn write_pack_caller<W: Write>(out: &mut W, c: &super::SymbolNavInfo) -> io::Result<()> {
    write!(
        out,
        "<s t=\"{}\" n=\"{}\" p=\"{}:{}\"/>",
        c.kind,
        c.name.xml_escaped(),
        c.path.xml_escaped(),
        c.line
    )
}

fn write_pack_test<W: Write>(
    out: &mut W,
    t: &crate::gate::affected::AffectedTest,
) -> io::Result<()> {
    let partner = if t.is_partner { " partner=\"1\"" } else { "" };
    let run = t.run_attr();
    write!(
        out,
        "<t p=\"{}\"{partner} hops=\"{}\"{run}/>",
        t.path.xml_escaped(),
        t.hops
    )
}

pub fn truncate_body_to_budget(raw: &str, max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        return raw.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(nl) = raw[..end].rfind('\n') {
        end = nl;
    }
    format!("{}{PACK_TRUNCATION_MARK}", &raw[..end])
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

struct PackRenderEnv<'a> {
    task: &'a str,
    route: &'a str,
    root: &'a str,
    ingest: &'a IngestResult,
    graph: &'a Graph,
    ranks: &'a [f64],
    interner: &'a Interner,
}

/// What one pack admits under a byte budget: the leading signature rows, the bodies of those symbols, and the
/// callers and tests windowed to the bytes left over.
struct PackPlan {
    sigs: usize,
    bodies: Vec<PackTaskBody>,
    bodies_total: usize,
    callers: Vec<super::SymbolNavInfo>,
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

fn pack_body_text(it: &serde_json::Value) -> Option<&str> {
    it.get("body")
        .or_else(|| it.get("sig"))
        .and_then(serde_json::Value::as_str)
}

fn write_pack_sig_row<W: Write>(out: &mut W, idx: usize, it: &serde_json::Value) -> io::Result<()> {
    let raw_name = it["name"].as_str().unwrap_or("");
    let raw_path = it["path"].as_str().unwrap_or("");
    let n_esc = raw_name.xml_escaped();
    let p_esc = raw_path.xml_escaped();
    let l = it["line"].as_u64().unwrap_or(1);
    let cx = it["cx"].as_u64().unwrap_or(1);
    let ccx = it["ccx"].as_u64().unwrap_or(1);
    let in_deg = it["in"].as_u64().unwrap_or(0);
    let sig = it["sig"].as_str().unwrap_or(raw_name).xml_escaped();
    let r = idx + 1;
    write!(
        out,
        "<d l=\"{l}\" n=\"{n_esc}\" p=\"{p_esc}\" cx=\"{cx}\" ccx=\"{ccx}\" in=\"{in_deg}\" r=\"{r}\" next=\"--expand={p_esc}:{n_esc}\">{sig}</d>"
    )
}

/// Bodies of the admitted symbols in rank order, each truncated to what its row may take from `remaining`.
fn admit_pack_bodies(admitted: &[serde_json::Value], remaining: &mut usize) -> Vec<PackTaskBody> {
    let mut bodies = Vec::new();
    for it in admitted {
        let Some(raw) = pack_body_text(it) else {
            continue;
        };
        let kind = it["kind"].as_str().unwrap_or("fn").to_string();
        let line = it["line"].as_u64().unwrap_or(1);
        let path_esc = it["path"].as_str().unwrap_or("").xml_escaped().to_string();
        let name_esc = it["name"].as_str().unwrap_or("").xml_escaped().to_string();
        let overhead = row_bytes(|w| {
            write!(
                w,
                "<b t=\"{kind}\" l=\"{line}\" p=\"{path_esc}\" n=\"{name_esc}\"><![CDATA[]]></b>"
            )
        });
        let cap = remaining.saturating_sub(overhead).min(MAX_PACK_BODY_BYTES);
        if cap < MIN_PACK_BODY_BYTES {
            break;
        }
        let body = if raw.len() <= cap {
            raw.to_string()
        } else {
            truncate_body_to_budget(raw, cap - PACK_TRUNCATION_MARK.len())
        };
        *remaining = remaining.saturating_sub(overhead + body.len());
        bodies.push(PackTaskBody {
            kind,
            line,
            path_esc,
            name_esc,
            body,
        });
    }
    bodies
}

/// Callers of the top-ranked admitted symbol, the list the pack windows.
fn pack_top_callers(
    env: &PackRenderEnv<'_>,
    admitted: &[serde_json::Value],
) -> Vec<super::SymbolNavInfo> {
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
    callers: &[super::SymbolNavInfo],
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
    for b in &plan.bodies {
        write!(
            out,
            "<b t=\"{}\" l=\"{}\" p=\"{}\" n=\"{}\">",
            b.kind, b.line, b.path_esc, b.name_esc
        )?;
        crate::serialize::write_cdata(out, &b.body)?;
        write!(out, "</b>")?;
    }
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
        let next = doc.len().div_ceil(crate::serialize::BYTES_PER_TOKEN);
        if next == est {
            break;
        }
        est = next;
    }
    doc
}

/// Writes one pack shaped to `budget_tokens`: it shrinks the plan until the document fits, or until only the
/// minimal unit (the top signature) remains, in which case `over_budget=1` discloses the overrun.
fn write_pack_task_ctx<W: Write>(
    env: &PackRenderEnv<'_>,
    syms: &[serde_json::Value],
    budget_tokens: usize,
    include_legend: bool,
    out: &mut W,
) -> io::Result<std::collections::BTreeSet<crate::domain::NodeId>> {
    let syms = pack_candidates(syms, env.ingest, env.interner);
    let limit = budget_tokens.saturating_mul(crate::serialize::BYTES_PER_TOKEN);
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

fn build_masked_node_json(
    id: crate::domain::NodeId,
    root: &str,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Option<serde_json::Value> {
    let sym = ingest.sym(id)?;
    let name = interner.resolve(&sym.name);
    let path = ingest.sym_file_path(sym, interner);
    let bytes = crate::domain::read_rel_bytes(std::path::Path::new(root), path).ok();
    let sig = bytes
        .as_deref()
        .map_or_else(|| name.to_string(), |b| sym.extract_signature(b, name));
    let body = bytes
        .as_deref()
        .and_then(|b| sym.source_text(b).map(std::borrow::Cow::into_owned))
        .unwrap_or_else(|| sig.clone());
    Some(serde_json::json!({
        "name": name,
        "path": path,
        "line": sym.line,
        "kind": sym.kind.tag(),
        "cx": sym.cx.max(1),
        "ccx": sym.ccx,
        "in": graph.in_degree(id),
        "sig": sig,
        "body": body,
    }))
}

fn render_masked_pack_bundle(
    env: &PackRenderEnv<'_>,
    keep_ids: &[crate::domain::NodeId],
    budget_tokens: usize,
) -> io::Result<(String, std::collections::BTreeSet<crate::domain::NodeId>)> {
    let syms: Vec<serde_json::Value> = keep_ids
        .iter()
        .filter_map(|&id| build_masked_node_json(id, env.root, env.ingest, env.graph, env.interner))
        .collect();
    let mut buf = Vec::new();
    let surface = write_pack_task_ctx(env, &syms, budget_tokens, false, &mut buf)?;
    Ok((String::from_utf8_lossy(&buf).into_owned(), surface))
}

pub fn execute_pack_task_partitioned<W: Write>(
    task: &str,
    root: &str,
    requested: usize,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let n = requested.clamp(2, 16);
    let pr = graph.page_rank(&crate::rank::PageRankConfig::default());
    let plan = crate::gate::plan_task_partition(task, n, ingest, graph, &pr.ranks, interner);
    let core_budget = 2040usize;
    let part_budget = 3960usize;
    let env = PackRenderEnv {
        task,
        root,
        route: "subtoken+body:broad",
        ingest,
        graph,
        ranks: &pr.ranks,
        interner,
    };
    let (core_xml, core_surface) = render_masked_pack_bundle(&env, &plan.core_ids, core_budget)?;
    let mut part_xmls = Vec::with_capacity(plan.groups.len());
    let mut part_surfaces = Vec::with_capacity(plan.groups.len());
    for grp in &plan.groups {
        let (pxml, psurf) = render_masked_pack_bundle(&env, grp, part_budget)?;
        part_xmls.push(pxml);
        part_surfaces.push(psurf);
    }
    let ov = crate::gate::measure_partition_overlap(&part_surfaces, &core_surface);
    let total_bytes: usize = core_xml.len() + part_xmls.iter().map(String::len).sum::<usize>();
    write!(
        out,
        "<!-- ripwire-rs partitioned task bundle: ONE shared core plus N minimally overlapping slices along call-graph communities. -->\n<ctx-partitions schema=\"ripwire-rs.pack-task/v1\" partitions=\"{}\" requested=\"{n}\" core_symbols=\"{}\" surface=\"{}\" modules=\"{}\" split=\"{}\" budget_per_agent_tokens=\"6000\" core_budget_tokens=\"{core_budget}\" partition_budget_tokens=\"{part_budget}\" total_bytes=\"{total_bytes}\" overlap_mean=\"{:.3}\" overlap_max=\"{:.3}\" shared_symbols=\"{}\" union_symbols=\"{}\" core_overlap=\"{:.3}\">",
        part_xmls.len(),
        plan.core_ids.len(),
        plan.surface_count,
        plan.module_count,
        plan.split_count,
        ov.mean,
        ov.worst,
        ov.shared_count,
        ov.union_count,
        ov.core_leak,
    )?;
    let core_toks = (core_xml.len() * 100 / 236).max(1);
    write!(
        out,
        "<bundle role=\"core\" symbols=\"{}\" bytes=\"{}\" tokens=\"{core_toks}\" est_tokens=\"{core_toks}\">{core_xml}</bundle>",
        plan.core_ids.len(),
        core_xml.len()
    )?;
    for (idx, pxml) in part_xmls.iter().enumerate() {
        let syms_cnt = plan.groups.get(idx).map_or(0, Vec::len);
        let mods_cnt = plan.group_module_count.get(idx).copied().unwrap_or(1);
        let p_toks = (pxml.len() * 100 / 236).max(1);
        write!(
            out,
            "<bundle role=\"partition\" i=\"{idx}\" symbols=\"{syms_cnt}\" modules=\"{mods_cnt}\" bytes=\"{}\" tokens=\"{p_toks}\" est_tokens=\"{p_toks}\">{pxml}</bundle>",
            pxml.len()
        )?;
    }
    writeln!(out, "</ctx-partitions>")
}
