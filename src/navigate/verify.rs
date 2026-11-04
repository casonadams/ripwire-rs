use super::{find_symbols_by_name, transitive_impact_multi};
use crate::domain::{CorpusView, IngestResult, Interner, NodeId};
use crate::graph::{Graph, GraphGauges};
use crate::serialize::{
    PageRequest, PageWindow, Paged, PathLayerExt, XmlEscapeExt, XmlReport, row_bytes,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};

fn reconstruct_path(mut node: NodeId, parent: &HashMap<NodeId, NodeId>) -> Vec<NodeId> {
    let mut path = vec![node];
    while let Some(&prev) = parent.get(&node) {
        if prev == node {
            break;
        }
        path.push(prev);
        node = prev;
    }
    path.reverse();
    path
}

fn bfs_shortest_path(graph: &Graph, from_ids: &[NodeId], to_ids: &[NodeId]) -> Vec<NodeId> {
    if let ([s], [t]) = (from_ids, to_ids)
        && s == t
    {
        return vec![*s];
    }
    let targets: HashSet<NodeId> = to_ids.iter().copied().collect();
    let mut queue: VecDeque<NodeId> = from_ids.iter().copied().collect();
    let mut parent: HashMap<NodeId, NodeId> = from_ids.iter().map(|&id| (id, id)).collect();
    while let Some(curr) = queue.pop_front() {
        for (callee, _) in graph.callees(curr) {
            if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(callee) {
                e.insert(curr);
                if targets.contains(&callee) {
                    return reconstruct_path(callee, &parent);
                }
                queue.push_back(callee);
            }
        }
    }
    Vec::new()
}

fn write_sym_witness<W: Write>(
    id: NodeId,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    if let Some(sym) = ingest.sym(id) {
        let name = interner.resolve(&sym.name).xml_escaped();
        let path = ingest.sym_file_path(sym, interner).xml_escaped();
        write!(
            out,
            "<s t=\"{}\" n=\"{name}\" p=\"{path}:{}\"/>",
            sym.kind.tag(),
            sym.line
        )?;
    }
    Ok(())
}

fn collect_literal_hits(
    file_pat: &str,
    lit: &str,
    ctx: &VerifyContext<'_>,
) -> Vec<(String, u32, String, String)> {
    let req = crate::query::retrieval::GrepRequest {
        path_filter: Some(file_pat),
        any_tier: true,
        ..crate::query::retrieval::GrepRequest::new(ctx.root, lit, ctx.sources)
    };
    let val = crate::query::retrieval::call_grep(ctx.ingest, ctx.interner, &req);
    let mut hits = Vec::new();
    if let Some(arr) = val["hits"].as_array() {
        for h in arr {
            hits.push((
                h["path"].as_str().unwrap_or("").to_string(),
                h["line"].as_u64().unwrap_or(1) as u32,
                h["enclosing"].as_str().unwrap_or("").to_string(),
                h["text"].as_str().unwrap_or("").trim().to_string(),
            ));
        }
    }
    hits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if !hits.is_empty() {
        return hits;
    }
    for sym in &ctx.ingest.symbols {
        let path = ctx.ingest.sym_file_path(sym, ctx.interner);
        let name = ctx.interner.resolve(&sym.name);
        if path.contains(file_pat) && name.contains(lit) {
            hits.push((
                path.to_string(),
                sym.line,
                name.to_string(),
                name.to_string(),
            ));
        }
    }
    hits
}

/// Borrowed analysis context for evaluating verification claims.
pub struct VerifyContext<'a> {
    pub root: &'a str,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub interner: &'a Interner,
    pub sources: &'a dyn crate::query::SourceReader,
}

impl<'a> VerifyContext<'a> {
    pub fn new(
        root: &'a str,
        ingest: &'a IngestResult,
        graph: &'a Graph,
        interner: &'a Interner,
        sources: &'a dyn crate::query::SourceReader,
    ) -> Self {
        Self {
            root,
            ingest,
            graph,
            interner,
            sources,
        }
    }
}

impl<'a> From<&'a crate::query::commands::QueryContext<'a>> for VerifyContext<'a> {
    fn from(ctx: &'a crate::query::commands::QueryContext<'a>) -> Self {
        Self {
            root: ctx.root,
            ingest: ctx.ingest,
            graph: ctx.graph,
            interner: ctx.interner,
            sources: ctx.sources,
        }
    }
}

impl<'a> From<crate::query::commands::QueryContext<'a>> for VerifyContext<'a> {
    fn from(ctx: crate::query::commands::QueryContext<'a>) -> Self {
        Self {
            root: ctx.root,
            ingest: ctx.ingest,
            graph: ctx.graph,
            interner: ctx.interner,
            sources: ctx.sources,
        }
    }
}

impl<'a> CorpusView<'a> for &VerifyContext<'a> {
    fn ingest(&self) -> &'a IngestResult {
        self.ingest
    }
    fn interner(&self) -> &'a Interner {
        self.interner
    }
}

fn write_hit_witness<W: Write>(
    (path, line, enc, text): &(String, u32, String, String),
    out: &mut W,
) -> io::Result<()> {
    write!(
        out,
        "<hit p=\"{}:{line}\" in=\"{}\"><m>",
        path.xml_escaped(),
        enc.xml_escaped()
    )?;
    crate::serialize::write_cdata(out, text)?;
    write!(out, "</m></hit>")
}

pub enum VerifyWitness<'a> {
    Nodes(Vec<NodeId>),
    Uses(Vec<(&'static str, &'a str, u32, Option<&'a str>)>),
    Hits(Vec<(String, u32, String, String)>),
}

fn write_use_witness<W: Write>(
    out: &mut W,
    &(role, file, line, enc): &(&str, &str, u32, Option<&str>),
) -> io::Result<()> {
    let f_esc = file.xml_escaped();
    match enc.filter(|s| !s.is_empty()) {
        Some(e) => write!(
            out,
            "<u role=\"{role}\" p=\"{f_esc}:{line}\" in_id=\"{}\"/>",
            e.xml_escaped()
        ),
        None => write!(out, "<u role=\"{role}\" p=\"{f_esc}:{line}\"/>"),
    }
}

fn hit_window(hits: &[(String, u32, String, String)], page: PageRequest) -> PageWindow {
    Paged::new(hits, page, |h| {
        row_bytes(|mut w| write_hit_witness(h, &mut w))
    })
    .window
}

pub struct VerifyReport<'a> {
    pub claim: &'a str,
    pub shape: &'static str,
    pub verdict: &'static str,
    pub facts: String,
    pub limit: Option<&'static str>,
    pub complete: bool,
    pub witness: VerifyWitness<'a>,
    pub ingest: &'a IngestResult,
    pub interner: &'a Interner,
    pub gauges: GraphGauges,
    pub page: PageRequest,
}

impl VerifyReport<'_> {
    fn witness_window(&self) -> PageWindow {
        let (ingest, interner, page) = (self.ingest, self.interner, self.page);
        match &self.witness {
            VerifyWitness::Nodes(ids) => {
                Paged::new(ids, page, |&id| {
                    row_bytes(|mut w| write_sym_witness(id, ingest, interner, &mut w))
                })
                .window
            }
            VerifyWitness::Uses(sites) => {
                Paged::new(sites, page, |site| {
                    row_bytes(|mut w| write_use_witness(&mut w, site))
                })
                .window
            }
            VerifyWitness::Hits(hits) => hit_window(hits, page),
        }
    }
}

impl XmlReport for VerifyReport<'_> {
    fn schema_tag(&self) -> &'static str {
        "verify"
    }
    fn schema_version(&self) -> &'static str {
        "ripwire-rs.verify/v1"
    }
    fn legend_comment(&self) -> Option<&'static str> {
        Some(
            "ripwire-rs verify schema=ripwire-rs.verify/v1: ONE structured claim, verdict=confirmed|refuted|not-established, witness rows inline.",
        )
    }
    fn graph_gauges(&self) -> Option<GraphGauges> {
        (!self.complete).then_some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " claim=\"{}\" shape=\"{}\" verdict=\"{}\"{}",
            self.claim.xml_escaped(),
            self.shape,
            self.verdict,
            self.facts
        )?;
        if let Some(lim) = self.limit {
            write!(out, " limit=\"{lim}\"")?;
        }
        if self.complete {
            write!(out, " complete=\"1\"")?;
        }
        write!(out, "{}", self.witness_window().xml_attrs())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let win = self.witness_window();
        match &self.witness {
            VerifyWitness::Nodes(ids) => {
                for &id in &ids[win.begin..win.end] {
                    write_sym_witness(id, self.ingest, self.interner, out)?;
                }
            }
            VerifyWitness::Uses(sites) => {
                for site in &sites[win.begin..win.end] {
                    write_use_witness(out, site)?;
                }
            }
            VerifyWitness::Hits(hits) => {
                for h in &hits[win.begin..win.end] {
                    write_hit_witness(h, out)?;
                }
            }
        }
        Ok(())
    }
}

fn parse_verify_claim(claim: &str) -> Option<(&str, &str, &str, bool)> {
    let trimmed = claim.trim();
    let open = trimmed.find('(')?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let word = trimmed[..open].trim();
    let inner = trimmed[open + 1..trimmed.len() - 1].trim();
    if matches!(word, "uses" | "unused") {
        return (!inner.is_empty() && !inner.contains(',')).then_some((word, inner, "", false));
    }
    let (a1, a2) = inner.split_once(',')?;
    let (arg1, rest) = (a1.trim(), a2.trim());
    if arg1.is_empty() || rest.is_empty() {
        return None;
    }
    let quoted = (rest.starts_with('"') && rest.ends_with('"')
        || rest.starts_with('\'') && rest.ends_with('\''))
        && rest.len() >= 2;
    Some((
        word,
        arg1,
        if quoted {
            &rest[1..rest.len() - 1]
        } else {
            rest
        },
        quoted,
    ))
}

fn eval_calls<'a>(
    claim: &'a str,
    a: &str,
    b: &str,
    ctx: &VerifyContext<'a>,
    page: PageRequest,
) -> VerifyReport<'a> {
    let from_ids = find_symbols_by_name(a, ctx.ingest, ctx.interner);
    let to_ids = find_symbols_by_name(b, ctx.ingest, ctx.interner);
    let path = bfs_shortest_path(ctx.graph, &from_ids, &to_ids);
    let ok = !path.is_empty();
    let hops = if ok {
        format!(" hops=\"{}\"", path.len() - 1)
    } else {
        String::new()
    };
    VerifyReport {
        claim,
        shape: "calls",
        verdict: if ok { "confirmed" } else { "not-established" },
        facts: format!(
            " from_defs=\"{}\" to_defs=\"{}\"{hops}",
            from_ids.len(),
            to_ids.len()
        ),
        limit: (!ok).then_some("call-graph-floor"),
        complete: false,
        witness: VerifyWitness::Nodes(path),
        ingest: ctx.ingest,
        interner: ctx.interner,
        gauges: ctx.graph.gauges(),
        page,
    }
}

fn eval_uses<'a>(
    claim: &'a str,
    sym: &str,
    is_unused: bool,
    ctx: &VerifyContext<'a>,
    page: PageRequest,
) -> VerifyReport<'a> {
    let defs = find_symbols_by_name(sym, ctx.ingest, ctx.interner);
    let sites = (ctx.ingest, ctx.interner).collect_symbol_uses(sym);
    let total = sites.len();
    let verdict = match (is_unused, total > 0) {
        (false, true) => "confirmed",
        (true, true) => "refuted",
        _ => "not-established",
    };
    VerifyReport {
        claim,
        shape: if is_unused { "unused" } else { "uses" },
        verdict,
        facts: format!(
            " defs=\"{}\" external=\"{}\" count=\"{total}\"",
            defs.len(),
            usize::from(defs.is_empty())
        ),
        limit: (total == 0).then_some("reference-floor"),
        complete: false,
        witness: VerifyWitness::Uses(sites),
        ingest: ctx.ingest,
        interner: ctx.interner,
        gauges: ctx.graph.gauges(),
        page,
    }
}

fn eval_defines<'a>(
    claim: &'a str,
    file_pat: &str,
    sym: &str,
    ctx: &VerifyContext<'a>,
    page: PageRequest,
) -> VerifyReport<'a> {
    let defs_of_name = find_symbols_by_name(sym, ctx.ingest, ctx.interner);
    let in_file: Vec<NodeId> = defs_of_name
        .iter()
        .copied()
        .filter(|&id| {
            ctx.ingest
                .sym(id)
                .is_some_and(|s| ctx.ingest.sym_file_path(s, ctx.interner).contains(file_pat))
        })
        .collect();
    if !in_file.is_empty() {
        return VerifyReport {
            claim,
            shape: "defines",
            verdict: "confirmed",
            facts: format!(
                " defs=\"{}\" defs_of_name=\"{}\"",
                in_file.len(),
                defs_of_name.len()
            ),
            limit: None,
            complete: false,
            witness: VerifyWitness::Nodes(in_file),
            ingest: ctx.ingest,
            interner: ctx.interner,
            gauges: ctx.graph.gauges(),
            page,
        };
    }
    let hits = collect_literal_hits(file_pat, sym, ctx);
    let occ = hits.len();
    VerifyReport {
        claim,
        shape: "defines",
        verdict: if occ > 0 {
            "not-established"
        } else {
            "refuted"
        },
        facts: format!(
            " defs=\"0\" defs_of_name=\"{}\" occurrences=\"{occ}\"",
            defs_of_name.len()
        ),
        limit: (occ > 0).then_some("extraction-floor"),
        complete: occ == 0,
        witness: VerifyWitness::Hits(hits),
        ingest: ctx.ingest,
        interner: ctx.interner,
        gauges: ctx.graph.gauges(),
        page,
    }
}

/// The `reaches(SYM, "FILE" | LAYER)` operands: the symbol and the file or layer it must reach.
struct ReachesOperands<'a> {
    sym: &'a str,
    target: &'a str,
    is_quoted: bool,
}

fn eval_reaches<'a>(
    claim: &'a str,
    operands: ReachesOperands<'_>,
    ctx: &VerifyContext<'a>,
    page: PageRequest,
) -> VerifyReport<'a> {
    let ReachesOperands {
        sym,
        target,
        is_quoted,
    } = operands;
    let target_defs = find_symbols_by_name(sym, ctx.ingest, ctx.interner);
    let ranks = vec![0.0; ctx.ingest.symbols.len()];
    let impact = transitive_impact_multi(
        &target_defs,
        ctx.ingest,
        ctx.graph,
        &ranks,
        ctx.interner,
        None,
    );
    let mut nodes = Vec::new();
    let mut first_hops = None;
    for node in &impact {
        let matched = if is_quoted {
            node.path.contains(target)
        } else {
            node.path
                .builtin_layer()
                .is_some_and(|l| l.eq_ignore_ascii_case(target))
        };
        if matched {
            first_hops.get_or_insert(node.hops);
            nodes.push(node.id);
        }
    }
    let hops_attr = first_hops.map_or_else(String::new, |h| format!(" hops=\"{h}\""));
    VerifyReport {
        claim,
        shape: "reaches",
        verdict: if nodes.is_empty() {
            "not-established"
        } else {
            "confirmed"
        },
        facts: format!(
            " target_defs=\"{}\" witnesses=\"{}\"{hops_attr}",
            target_defs.len(),
            nodes.len()
        ),
        limit: nodes.is_empty().then_some("call-graph-floor"),
        complete: false,
        witness: VerifyWitness::Nodes(nodes),
        ingest: ctx.ingest,
        interner: ctx.interner,
        gauges: ctx.graph.gauges(),
        page,
    }
}

pub fn evaluate_verify_claim<W: Write>(
    claim: &str,
    ctx: &VerifyContext<'_>,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let rep = match parse_verify_claim(claim) {
        Some(("calls", a, b, _)) => eval_calls(claim, a, b, ctx, page),
        Some(("uses", sym, _, _)) => eval_uses(claim, sym, false, ctx, page),
        Some(("unused", sym, _, _)) => eval_uses(claim, sym, true, ctx, page),
        Some(("contains", f, lit, _)) => {
            let hits = collect_literal_hits(f, lit, ctx);
            let total = hits.len();
            VerifyReport {
                claim,
                shape: "contains",
                verdict: if total > 0 { "confirmed" } else { "refuted" },
                facts: format!(" hits=\"{total}\""),
                limit: None,
                complete: !hit_window(&hits, page).capped,
                witness: VerifyWitness::Hits(hits),
                ingest: ctx.ingest,
                interner: ctx.interner,
                gauges: ctx.graph.gauges(),
                page,
            }
        }
        Some(("defines", f, sym, _)) => eval_defines(claim, f, sym, ctx, page),
        Some(("reaches", sym, target, q)) => eval_reaches(
            claim,
            ReachesOperands {
                sym,
                target,
                is_quoted: q,
            },
            ctx,
            page,
        ),
        _ => VerifyReport {
            claim,
            shape: "calls",
            verdict: "not-established",
            facts: " from_defs=\"0\" to_defs=\"0\"".to_string(),
            limit: Some("call-graph-floor"),
            complete: false,
            witness: VerifyWitness::Nodes(Vec::new()),
            ingest: ctx.ingest,
            interner: ctx.interner,
            gauges: ctx.graph.gauges(),
            page,
        },
    };
    rep.render_envelope(out)
}
