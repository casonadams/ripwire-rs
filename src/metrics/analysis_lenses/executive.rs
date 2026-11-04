use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::query::safety::SafetyContext;
use crate::serialize::{DEFAULT_PACK_BUDGET_BYTES, PageRequest, PageWindow};
use std::io::{self, Write};

/// The lists the executive report prints, which share its byte budget equally.
const REPORT_LISTS: usize = 5;

fn extract_module_intents(members: &[(&'static str, &str, &str, u32)]) -> String {
    const VERBS: &[&str] = &[
        "create",
        "make",
        "get",
        "parse",
        "build",
        "run",
        "resolve",
        "serialize",
        "check",
        "execute",
        "compute",
        "find",
        "load",
        "write",
    ];
    let mut counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for &(_, name, _, _) in members {
        let lower = name.to_ascii_lowercase();
        for &v in VERBS {
            if lower.starts_with(v) {
                *counts.entry(v).or_default() += 1;
                break;
            }
        }
    }
    let mut ranked: Vec<(&'static str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    if ranked.is_empty() {
        String::new()
    } else {
        let tags: Vec<&str> = ranked.into_iter().take(3).map(|(v, _)| v).collect();
        format!(" `[{}]`", tags.join(", "))
    }
}

fn non_empty_or<'a>(s: &'a str, fallback: &'a str) -> &'a str {
    if s.is_empty() { fallback } else { s }
}

/// One report list: its heading with shown and total counts, its rows windowed under `page` by their rendered
/// bytes, and a route to the rows left out.
fn write_report_list<W: Write>(
    out: &mut W,
    heading: &str,
    lines: &[String],
    page: PageRequest,
    empty: &str,
) -> io::Result<()> {
    let pw = page.window_fit(lines.len(), |i| lines[i].len() + 1);
    writeln!(
        out,
        "## {heading} (showing {} of {})",
        pw.shown,
        lines.len()
    )?;
    if lines.is_empty() {
        return writeln!(out, "- {empty}\n");
    }
    for line in &lines[pw.begin..pw.end] {
        writeln!(out, "{line}")?;
    }
    write_report_next(out, pw)?;
    writeln!(out)
}

/// The route to the next page of a capped report list; nothing when the list is whole.
fn write_report_next<W: Write>(out: &mut W, pw: PageWindow) -> io::Result<()> {
    if !pw.has_more {
        return Ok(());
    }
    writeln!(
        out,
        "- next: `--report --limit={} --offset={}`",
        pw.shown, pw.next_offset
    )
}

fn write_report_modules_and_bridges<W: Write>(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let (comms, _, modules) =
        crate::metrics::architecture::build_ranked_communities(None, ingest, graph, interner);
    let module_lines: Vec<String> = modules
        .iter()
        .map(|m| {
            let intents = extract_module_intents(&m.members);
            format!(
                "- **{}** (`{}`) — {} symbols{intents}",
                m.label, m.dir, m.size
            )
        })
        .collect();
    write_report_list(
        out,
        "Modules (call-graph clusters)",
        &module_lines,
        page,
        "(none)",
    )?;
    let bridges: Vec<String> =
        crate::metrics::architecture::collect_community_bridges(&comms, &modules, ingest, graph)
            .into_iter()
            .map(|(_, _, la, lb, w)| {
                format!(
                    "- `{}` ↔ `{}` ({w} edges)",
                    non_empty_or(la, "module"),
                    non_empty_or(lb, "module")
                )
            })
            .collect();
    write_report_list(out, "Cross-module bridges", &bridges, page, "(none)")
}

fn write_report_deps_sections<W: Write>(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let deps = crate::metrics::architecture::compute_deps_report(ingest, graph, interner);
    let mut god: Vec<&crate::serialize::DepsFileRow<'_>> =
        deps.rows.iter().filter(|r| r.afferent > 0).collect();
    god.sort_by(|a, b| b.afferent.cmp(&a.afferent).then_with(|| a.path.cmp(b.path)));
    let god_lines: Vec<String> = god
        .iter()
        .map(|g| format!("- `{}` — {} dependents", g.path, g.afferent))
        .collect();
    write_report_list(
        out,
        "God files (most depended-on)",
        &god_lines,
        page,
        "(no include/import edges captured)",
    )?;
    let cycle_lines: Vec<String> = deps
        .cycles
        .iter()
        .map(|cyc| {
            let formatted: Vec<String> = cyc.iter().map(|p| format!("`{p}`")).collect();
            format!("- {}", formatted.join(" ↔ "))
        })
        .collect();
    write_report_list(
        out,
        "Dependency cycles",
        &cycle_lines,
        page,
        "none (acyclic)",
    )
}

pub fn serialize_executive_report_md<W: Write>(
    facts: &SafetyContext<'_>,
    ranks: &[f64],
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let SafetyContext {
        root,
        ingest,
        graph,
        interner,
    } = *facts;
    writeln!(out, "# Architecture & Quality Report (`{root}`)\n")?;
    writeln!(out, "## Corpus Overview")?;
    writeln!(out, "- **Files Indexed**: {}", ingest.files.len())?;
    writeln!(out, "- **Symbols Extracted**: {}", ingest.symbol_count())?;
    writeln!(out, "- **Call Graph Edges**: {}\n", graph.edge_count())?;

    // The report budget is shared by its five lists, so the whole report stays within it.
    let share = PageRequest {
        budget: Some(page.budget.unwrap_or(DEFAULT_PACK_BUDGET_BYTES) / REPORT_LISTS),
        ..page
    };
    write_report_modules_and_bridges(ingest, graph, interner, share, out)?;
    write_report_deps_sections(ingest, graph, interner, share, out)?;

    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let mut order: Vec<usize> = (0..ingest.symbols.len())
        .filter(|&i| view.is_production_symbol(&ingest.symbols[i]))
        .collect();
    if order.is_empty() {
        order = (0..ingest.symbols.len()).collect();
    }
    order.sort_by(|&a, &b| {
        let ra = ranks.get(a).copied().unwrap_or(0.0);
        let rb = ranks.get(b).copied().unwrap_or(0.0);
        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
    });
    let rows: Vec<String> = order
        .iter()
        .map(|&idx| {
            let sym = &ingest.symbols[idx];
            let rank = ranks.get(idx).copied().unwrap_or(0.0);
            format!(
                "| `{}` | `{}` | `{}` | {} | {} | {rank:.4} |",
                view.sym_name(sym),
                sym.kind.tag(),
                view.sym_path(sym),
                sym.line,
                sym.ccx
            )
        })
        .collect();
    let pw = share.window_fit(rows.len(), |i| rows[i].len() + 1);
    writeln!(
        out,
        "## Top Symbols by PageRank (showing {} of {})",
        pw.shown,
        rows.len()
    )?;
    writeln!(out, "| Symbol | Kind | File | Line | CCX | PageRank |")?;
    writeln!(out, "|---|---|---|---|---|---|")?;
    for row in &rows[pw.begin..pw.end] {
        writeln!(out, "{row}")?;
    }
    writeln!(out)?;
    write_report_next(out, pw)
}
