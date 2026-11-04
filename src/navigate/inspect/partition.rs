use super::pack::{PackRenderEnv, write_pack_task_ctx};
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use std::io::{self, Write};

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
    let bytes = crate::crawler::read_rel_bytes(std::path::Path::new(root), path).ok();
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
