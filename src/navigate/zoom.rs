use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::serialize::{PageRequest, PageWindow, Paged, XmlEscapeExt, row_bytes};
use std::collections::HashMap;
use std::io::{self, Write};

/// Default number of hierarchy levels printed from the top (`--zoom-levels` overrides it).
pub const ZOOM_DEFAULT_LEVELS_SHOWN: usize = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoomOptions {
    pub depth: Option<u32>,
    pub levels_shown: Option<u32>,
    pub page: PageRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoomModule {
    pub level: usize,
    pub id: NodeId,
    pub size: usize,
    pub dir: String,
    pub children: Vec<ZoomModule>,
    pub unprinted_children: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoomTree {
    pub levels: usize,
    pub levels_shown: usize,
    pub symbols: usize,
    pub isolated: usize,
    pub top: Vec<ZoomModule>,
}

/// Levels printed from the top: `requested` 0 means every level, absent means the default window.
#[must_use]
pub fn resolve_levels_shown(levels: usize, requested: Option<u32>) -> usize {
    match requested {
        Some(0) => levels,
        Some(n) => (n as usize).min(levels),
        None => levels.min(ZOOM_DEFAULT_LEVELS_SHOWN),
    }
}

fn group_by_level(nodes: &[NodeId], assign: &[u32]) -> Vec<Vec<NodeId>> {
    let mut map: HashMap<u32, Vec<NodeId>> = HashMap::new();
    for &n in nodes {
        map.entry(assign[n as usize]).or_default().push(n);
    }
    let mut groups: Vec<Vec<NodeId>> = map.into_values().filter(|g| g.len() >= 2).collect();
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    groups
}

fn dominant_dir(nodes: &[NodeId], dirs: &[&str]) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &n in nodes {
        if let Some(d) = dirs.get(n as usize) {
            *counts.entry(d).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(d, _)| d.to_string())
        .unwrap_or_default()
}

fn build_module(
    level: usize,
    nodes: Vec<NodeId>,
    hierarchy: &[Vec<u32>],
    cut: usize,
    dirs: &[&str],
) -> ZoomModule {
    let mut children = Vec::new();
    let mut unprinted_children = 0;
    if level > 0 {
        let groups = group_by_level(&nodes, &hierarchy[level - 1]);
        if level > cut {
            children = groups
                .into_iter()
                .map(|g| build_module(level - 1, g, hierarchy, cut, dirs))
                .collect();
        } else {
            unprinted_children = groups.len();
        }
    }
    ZoomModule {
        level,
        id: nodes[0],
        size: nodes.len(),
        dir: dominant_dir(&nodes, dirs),
        children,
        unprinted_children,
    }
}

/// Build the printed module tree from a finest-first hierarchy and each node's directory.
#[must_use]
pub fn build_zoom_tree(
    hierarchy: &[Vec<u32>],
    dirs: &[&str],
    levels_shown: Option<u32>,
) -> ZoomTree {
    let levels = hierarchy.len();
    let symbols = hierarchy.first().map_or(0, Vec::len);
    let shown = resolve_levels_shown(levels, levels_shown);
    let Some(top_assign) = hierarchy.last().filter(|_| shown > 0) else {
        return ZoomTree {
            levels,
            levels_shown: shown,
            symbols,
            isolated: symbols,
            top: Vec::new(),
        };
    };
    let all: Vec<NodeId> = (0..symbols as NodeId).collect();
    let cut = levels - shown;
    let top: Vec<ZoomModule> = group_by_level(&all, top_assign)
        .into_iter()
        .map(|g| build_module(levels - 1, g, hierarchy, cut, dirs))
        .collect();
    let in_modules: usize = top.iter().map(|m| m.size).sum();
    ZoomTree {
        levels,
        levels_shown: shown,
        symbols,
        isolated: symbols - in_modules,
        top,
    }
}

fn write_module<W: Write>(m: &ZoomModule, out: &mut W) -> io::Result<()> {
    write!(
        out,
        "<module level=\"{}\" id=\"{}\" size=\"{}\" dir=\"{}\"",
        m.level,
        m.id,
        m.size,
        m.dir.xml_escaped()
    )?;
    if m.unprinted_children > 0 {
        write!(out, " children=\"{}\"", m.unprinted_children)?;
    }
    write!(out, ">")?;
    for c in &m.children {
        write_module(c, out)?;
    }
    writeln!(out, "</module>")
}

pub struct ZoomReport<'a> {
    pub tree: &'a ZoomTree,
    pub window: PageWindow,
    pub pr_iters: u32,
    pub gauges: crate::serialize::GraphGauges,
}

impl crate::serialize::XmlReport for ZoomReport<'_> {
    crate::xml_report_meta!(
        "zoom",
        "ripwire-rs.zoom/v1",
        root: false,
        comment: "ripwire-rs zoom schema=ripwire-rs.zoom/v1: nested Leiden module hierarchy: <module level= id= size= dir=>; level 0 is the finest partition; levels_shown= of levels= are printed from the top; symbols= = isolated= + size= of all top_modules=. children=K: K child modules below the levels_shown= cut, unprinted. id= is the module's lowest member node id; dir= its most common directory. pr_iters=N: PageRank iterations."
    );
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " levels=\"{}\" levels_shown=\"{}\" top_modules=\"{}\" symbols=\"{}\" isolated=\"{}\"{} pr_iters=\"{}\"",
            self.tree.levels,
            self.tree.levels_shown,
            self.tree.top.len(),
            self.tree.symbols,
            self.tree.isolated,
            self.window.xml_attrs(),
            self.pr_iters
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        self.tree.top[self.window.begin..self.window.end]
            .iter()
            .try_for_each(|m| write_module(m, out))
    }
}

/// Serialize a zoom tree with its top-module window.
pub fn serialize_zoom_xml<W: Write>(
    tree: &ZoomTree,
    window: &PageWindow,
    pr_iters: u32,
    gauges: crate::serialize::GraphGauges,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ZoomReport {
        tree,
        window: *window,
        pr_iters,
        gauges,
    }
    .render_envelope(out)
}

fn node_dirs<'a>(ingest: &'a IngestResult, interner: &'a Interner, nodes: usize) -> Vec<&'a str> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    (0..nodes)
        .map(|i| {
            ingest
                .sym(i as u32)
                .map(|s| view.sym_path(s))
                .filter(|p| !p.is_empty())
                .map_or("", |p| p.rsplit_once('/').map_or(".", |(d, _)| d))
        })
        .collect()
}

/// `--zoom`: multi-level Leiden hierarchy over the call graph.
pub fn execute_zoom_query<W: Write>(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    pr_iters: u32,
    opts: ZoomOptions,
    out: &mut W,
) -> io::Result<()> {
    let mut hierarchy = crate::graph::community::detect_hierarchy(graph);
    if let Some(depth) = opts.depth.filter(|&d| d > 0) {
        hierarchy.truncate(depth as usize);
    }
    let dirs = node_dirs(ingest, interner, graph.node_count());
    let tree = build_zoom_tree(&hierarchy, &dirs, opts.levels_shown);
    let top = Paged::new(&tree.top, opts.page, |m| {
        row_bytes(|mut w| write_module(m, &mut w))
    });
    serialize_zoom_xml(&tree, &top.window, pr_iters, graph.gauges(), out)
}
