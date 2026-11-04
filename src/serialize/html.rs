use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::serialize::{PageRequest, Paged, render_string, text_bytes};
use std::collections::HashMap;
use std::io::{self, Write};

/// Node coloring mode for HTML visualization.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorBy {
    #[default]
    Lang = 0,
    Community = 1,
    Cx = 2,
    Churn = 3,
    Tested = 4,
}

impl ColorBy {
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("comm") {
            Some(Self::Community)
        } else if lower.starts_with('c') {
            if lower.contains('h') {
                Some(Self::Churn)
            } else {
                Some(Self::Cx)
            }
        } else if lower.starts_with('t') {
            Some(Self::Tested)
        } else if lower.starts_with('l') {
            Some(Self::Lang)
        } else {
            None
        }
    }

    #[must_use]
    pub fn mode_name(self) -> &'static str {
        match (self, 0) {
            (Self::Community, _) => "community",
            (Self::Cx, _) => "cx",
            (Self::Churn, _) => "churn",
            (Self::Tested, _) => "tested",
            _ => "lang",
        }
    }
}

impl std::fmt::Display for ColorBy {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.mode_name())
    }
}

impl std::str::FromStr for ColorBy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_opt(s).ok_or_else(|| format!("invalid color-by mode: '{s}'"))
    }
}
/// Palette of 12 distinct dark-theme-friendly community colors.
pub const COMMUNITY_COLORS: &[&str] = &[
    "#29b6f6", "#ab47bc", "#26a69a", "#ffa726", "#ec407a", "#7e57c2", "#66bb6a", "#ff7043",
    "#5c6bc0", "#8d6e63", "#26c6da", "#d4e157",
];

struct HtmlNodeData {
    id: NodeId,
    file_id: u32,
    name: String,
    lang: String,
    cx: u32,
    comm: u32,
    rank: f64,
}

fn build_html_nodes(
    ingest: &IngestResult,
    ranks: &[f64],
    interner: &Interner,
    comms: &crate::graph::community::Communities,
    page: PageRequest,
) -> (Vec<HtmlNodeData>, HashMap<NodeId, usize>) {
    let mut indices: Vec<usize> = (0..ingest.symbols.len()).collect();
    indices.sort_by(|&a, &b| {
        let ra = ranks.get(a).copied().unwrap_or(0.0);
        let rb = ranks.get(b).copied().unwrap_or(0.0);
        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
    });

    let shown = Paged::new(&indices, page, |&idx| {
        ingest.symbols.get(idx).map_or(0, |sym| {
            text_bytes(&[interner.resolve(&sym.name), sym.lang.tag()])
        })
    });
    let mut nodes = Vec::with_capacity(shown.rows().len());
    let mut id_to_slot = HashMap::new();

    for (slot, &idx) in shown.rows().iter().enumerate() {
        if let Some(sym) = ingest.symbols.get(idx) {
            let id = sym.id;
            let name = interner.resolve(&sym.name).to_string();
            let comm = comms.comm.get(id as usize).copied().unwrap_or(0);
            let rank = ranks.get(idx).copied().unwrap_or(0.0);

            nodes.push(HtmlNodeData {
                id,
                file_id: sym.file_id,
                name,
                lang: sym.lang.tag().to_string(),
                cx: sym.cx,
                comm,
                rank,
            });
            id_to_slot.insert(id, slot);
        }
    }

    (nodes, id_to_slot)
}

/// A JSON array element separator: every row but the last ends with a comma.
const fn row_comma(more: bool) -> &'static str {
    if more { "," } else { "" }
}

/// Caller-to-callee edges between shown nodes, as `(source slot, target slot, ambiguous)`.
fn collect_links(
    graph: &Graph,
    nodes: &[HtmlNodeData],
    id_to_slot: &HashMap<NodeId, usize>,
) -> Vec<(usize, usize, bool)> {
    let mut edges = Vec::new();
    for &src_slot in id_to_slot.values() {
        let src_node = &nodes[src_slot];
        let amb = graph.ambiguity(src_node.id) > 0;
        for (tgt_id, _) in graph.callees(src_node.id) {
            if let Some(&tgt_slot) = id_to_slot.get(&tgt_id) {
                edges.push((src_slot, tgt_slot, amb));
            }
        }
    }
    edges.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    edges.dedup();
    edges
}

/// Shown symbols grouped by community: largest group first, ties by community id.
fn community_groups(nodes: &[HtmlNodeData]) -> Vec<(u32, Vec<usize>)> {
    let mut comm_members: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, n) in nodes.iter().enumerate() {
        comm_members.entry(n.comm).or_default().push(k);
    }
    let mut comm_list: Vec<(u32, Vec<usize>)> = comm_members.into_iter().collect();
    comm_list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    comm_list
}

fn write_files_json<W: Write>(
    out: &mut W,
    ingest: &IngestResult,
    interner: &Interner,
) -> io::Result<()> {
    out.write_all(b"const FILES = [\n")?;
    let count = ingest.files.len();
    for (i, f) in ingest.files.iter().enumerate() {
        let p = interner.resolve(&f.path);
        writeln!(out, "  {:?}{}", p, row_comma(i + 1 < count))?;
    }
    out.write_all(b"];\n")
}

fn write_nodes_json<W: Write>(out: &mut W, nodes: &[HtmlNodeData]) -> io::Result<()> {
    out.write_all(b"const NODES = [\n")?;
    for (k, n) in nodes.iter().enumerate() {
        writeln!(
            out,
            "  {{\"id\":{},\"label\":{:?},\"type\":\"fn\",\"lang\":{:?},\"rank\":{:.4},\"file\":{},\"comm\":{},\"cx\":{},\"ts\":0}}{}",
            k,
            n.name,
            n.lang,
            n.rank,
            n.file_id,
            n.comm,
            n.cx,
            row_comma(k + 1 < nodes.len())
        )?;
    }
    out.write_all(b"];\n")
}

fn write_links_json<W: Write>(out: &mut W, edges: &[(usize, usize, bool)]) -> io::Result<()> {
    out.write_all(b"const LINKS = [\n")?;
    for (i, (s, t, amb)) in edges.iter().enumerate() {
        let ambiguous = if *amb { ",\"a\":1" } else { "" };
        writeln!(
            out,
            "  {{\"s\":{},\"t\":{}{}}}{}",
            s,
            t,
            ambiguous,
            row_comma(i + 1 < edges.len())
        )?;
    }
    out.write_all(b"];\n")
}

fn write_modules_json<W: Write>(
    out: &mut W,
    nodes: &[HtmlNodeData],
    page: PageRequest,
) -> io::Result<()> {
    out.write_all(b"const MODULES = [\n")?;
    let groups = community_groups(nodes);
    for (disp, (comm_id, members)) in groups.iter().enumerate() {
        let first_file = members.first().map(|&idx| nodes[idx].file_id).unwrap_or(0);
        let top_m = Paged::new(members, page.nested(), |&idx| {
            let slot = idx.to_string();
            text_bytes(&[slot.as_str()])
        });
        writeln!(
            out,
            "  {{\"id\":{},\"name\":\"module_{}\",\"symCount\":{},\"fileCount\":1,\"files\":[{}],\"members\":{:?},\"top\":{:?},\"inCross\":0,\"outCross\":0,\"neigh\":[]}}{}",
            disp,
            comm_id,
            members.len(),
            first_file,
            members,
            top_m.rows(),
            row_comma(disp + 1 < groups.len())
        )?;
    }
    out.write_all(b"];\n")
}

/// `"<language tag>": "<display color>"` pairs for every language, comma separated.
struct LangColors;

impl std::fmt::Display for LangColors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, lang) in crate::languages::ALL_LANGS.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(
                f,
                "{:?}: {:?}",
                lang.tag(),
                lang.semantics().display_color()
            )?;
        }
        Ok(())
    }
}

const HTML_VIS_JS: &str = include_str!("html_vis.js");

/// Write the page up to the data section: the head, styles, and scalar constants.
fn write_page_head<W: Write>(
    out: &mut W,
    node_count: usize,
    edge_count: usize,
    color_by: ColorBy,
) -> io::Result<()> {
    write!(
        out,
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ripwire graph</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ background:#111; color:#eee; font:13px/1.4 sans-serif; overflow:hidden; }}
#bar {{ position:fixed; top:0; left:0; right:0; height:36px; background:rgba(0,0,0,.7); display:flex; align-items:center; gap:12px; padding:0 12px; z-index:10; border-bottom:1px solid #333; }}
#bar a {{ color:#7fb2ff; text-decoration:none; font-size:12px; }}
#bar input {{ background:#222; border:1px solid #444; color:#eee; padding:3px 8px; border-radius:4px; font-size:12px; }}
#bar select {{ background:#222; border:1px solid #444; color:#eee; padding:3px 6px; border-radius:4px; font-size:12px; }}
#c {{ width:100vw; height:100vh; display:block; }}
.module-card {{ data-module-card:1; }}
#cards {{ display:none; padding:48px 16px 16px; overflow:auto; height:100vh; grid-template-columns:repeat(auto-fill,minmax(260px,1fr)); gap:12px; }}
#crumb {{ position:fixed; bottom:12px; left:12px; background:rgba(0,0,0,.8); padding:4px 8px; border-radius:4px; font-size:11px; z-index:10; }}
#tooltip {{ position:absolute; display:none; background:#222; border:1px solid #666; padding:6px 10px; border-radius:4px; font-size:11px; pointer-events:none; z-index:20; }}
</style>
</head>
<body>
<div id="bar">
  <h1>ripwire graph</h1>
  <a class="nav" href="#graph">Graph</a>
  <a class="nav" href="#overview">Modules</a>
  <input id="search" type="text" placeholder="search labels...">
  <span id="depth">depth <input id="depthSlider" type="range" min="1" max="3" value="2" style="width:60px"><span id="depthVal">2</span></span>
  <span id="info"></span>
  <select id="colorMode"><option value="lang">lang</option><option value="community">community</option><option value="cx">cx</option><option value="churn">churn</option><option value="tested">tested</option></select>
  <span id="legend">read the source before trusting one</span>
  <button id="savePng" title="download this view as a PNG">PNG</button>
</div>
<div id="prov"></div>
<div id="hits"></div>
<div id="crumb"></div>
<div id="cards" data-module-card="1"></div>
<canvas id="c"></canvas>
<div id="tooltip"></div>
<script>
const ROOT = ".";
const AT_STAMP = "";
const BINARY_VER = "0.3.1";
const TOPK = {};
const NODE_TOTAL = {};
const EDGE_TOTAL = {};
const SYM_TOTAL = {};
const COLOR_MODE = "{}";
const LANG_COLORS = {{ {} }};
const SHAPES = {{}};
const CHURN_MAP = [];
const HAS_CHURN = false;
"##,
        node_count,
        node_count,
        edge_count,
        node_count,
        color_by.mode_name(),
        LangColors
    )
}

/// Write a self-contained interactive call graph HTML page to a stream.
pub fn write_call_graph_html<W: Write>(
    out: &mut W,
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
    color_by: ColorBy,
    page: PageRequest,
) -> io::Result<()> {
    let comms = graph.communities();
    let (nodes, id_to_slot) = build_html_nodes(ingest, ranks, interner, &comms, page);
    let edges = collect_links(graph, &nodes, &id_to_slot);

    write_page_head(out, nodes.len(), edges.len(), color_by)?;
    write_files_json(out, ingest, interner)?;
    out.write_all(b"\n")?;
    write_nodes_json(out, &nodes)?;
    out.write_all(b"\n")?;
    write_links_json(out, &edges)?;
    out.write_all(b"\n")?;
    write_modules_json(out, &nodes, page)?;
    out.write_all(b"\n")?;
    out.write_all(HTML_VIS_JS.as_bytes())?;
    out.write_all(b"\n</script>\n</body>\n</html>")
}

/// Generate a self-contained interactive call graph HTML string.
#[must_use]
pub fn generate_call_graph_html(
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
    initial_color_by: ColorBy,
    page: PageRequest,
) -> String {
    render_string(|out| {
        write_call_graph_html(out, ingest, graph, ranks, interner, initial_color_by, page)
    })
}
