use super::Graph;
use crate::domain::NodeId;
use leiden_rs::{GraphData, GraphDataBuilder, Leiden, LeidenConfig};

/// Result of community detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Communities {
    /// Community ID for each node (0..node_count-1), compacted to 0..count-1 in first-seen order.
    pub comm: Vec<u32>,
    /// Number of distinct non-empty communities.
    pub count: u32,
}

/// Seed of the Leiden run: the same graph always gives the same communities.
const LEIDEN_SEED: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
struct UndirectedEdge {
    target: NodeId,
    weight: f32,
}

fn coalesce_sorted_edges(edges: &mut Vec<UndirectedEdge>) -> f64 {
    if edges.is_empty() {
        return 0.0;
    }
    edges.sort_unstable_by_key(|e| e.target);
    let mut write_idx = 0usize;
    let mut m2 = f64::from(edges[0].weight);
    for read_idx in 1..edges.len() {
        let e = edges[read_idx];
        m2 += f64::from(e.weight);
        if edges[write_idx].target == e.target {
            edges[write_idx].weight += e.weight;
        } else {
            write_idx += 1;
            edges[write_idx] = e;
        }
    }
    edges.truncate(write_idx + 1);
    m2
}

fn build_undirected_adj(graph: &Graph) -> (Vec<Vec<UndirectedEdge>>, f64) {
    let n = graph.node_count();
    let mut adj: Vec<Vec<UndirectedEdge>> = vec![Vec::new(); n];

    for u in 0..n as NodeId {
        for (v, w) in graph.callees(u) {
            if u != v {
                adj[u as usize].push(UndirectedEdge {
                    target: v,
                    weight: w,
                });
                adj[v as usize].push(UndirectedEdge {
                    target: u,
                    weight: w,
                });
            }
        }
    }

    let mut m2 = 0.0f64;
    for edges in &mut adj {
        m2 += coalesce_sorted_edges(edges);
    }

    (adj, m2)
}

/// The undirected call graph as the Leiden crate takes it, and its total adjacency weight (twice the edge
/// weight); `None` when there is no edge to cluster.
fn leiden_graph(graph: &Graph) -> Option<GraphData> {
    let (adj, m2) = build_undirected_adj(graph);
    if m2 <= 0.0 {
        return None;
    }
    let mut builder = GraphDataBuilder::new(adj.len());
    for (u, edges) in adj.iter().enumerate() {
        for e in edges.iter().filter(|e| e.target as usize > u) {
            builder
                .add_edge(u, e.target as usize, f64::from(e.weight))
                .ok()?;
        }
    }
    builder.build().ok()
}

/// Community ids renumbered 0..count in the order the nodes first name them.
fn compact_community_ids(membership: &[usize]) -> Communities {
    let mut remap = std::collections::HashMap::new();
    let comm = membership
        .iter()
        .map(|&c| {
            let next = remap.len() as u32;
            *remap.entry(c).or_insert(next)
        })
        .collect();
    Communities {
        comm,
        count: remap.len() as u32,
    }
}

fn leiden() -> Leiden {
    Leiden::new(LeidenConfig::builder().seed(LEIDEN_SEED).build())
}

/// A graph with no clustering to find, or that the clusterer rejects: everything in one community when
/// there are no edges, every node alone otherwise.
fn unclustered(n: usize, has_edges: bool) -> Communities {
    if has_edges {
        Communities {
            comm: (0..n as u32).collect(),
            count: n as u32,
        }
    } else {
        Communities {
            comm: vec![0; n],
            count: 1,
        }
    }
}

/// Detect cohesive communities in the call graph with the Leiden modularity algorithm (every community is
/// connected, unlike Louvain's), via the `leiden-rs` crate with a fixed seed.
#[must_use]
pub fn detect_communities(graph: &Graph) -> Communities {
    let n = graph.node_count();
    if n == 0 {
        return Communities {
            comm: Vec::new(),
            count: 0,
        };
    }
    let Some(data) = leiden_graph(graph) else {
        return unclustered(n, false);
    };
    match leiden().run(&data) {
        Ok(out) => compact_community_ids(out.partition.as_slice()),
        Err(_) => unclustered(n, true),
    }
}

/// Multi-level Leiden: one per-node assignment per aggregation level, finest first.
#[must_use]
pub fn detect_hierarchy(graph: &Graph) -> Vec<Vec<u32>> {
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }
    let Some(data) = leiden_graph(graph) else {
        return vec![vec![0; n]];
    };
    match leiden().run_hierarchical(&data) {
        Ok(out) => out
            .levels
            .iter()
            .map(|level| compact_community_ids(&level.membership).comm)
            .collect(),
        Err(_) => vec![unclustered(n, true).comm],
    }
}
