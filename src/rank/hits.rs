use crate::graph::Graph;

/// Configuration for Kleinberg HITS algorithm.
#[derive(Debug, Clone, PartialEq)]
pub struct HitsConfig {
    pub tol: f64,
    pub max_iter: usize,
}

impl Default for HitsConfig {
    fn default() -> Self {
        Self {
            tol: 1e-6,
            max_iter: 100,
        }
    }
}

/// Result of Kleinberg HITS calculation holding Authority and Hub vectors.
#[derive(Debug, Clone, PartialEq)]
pub struct HitsResult {
    pub authority: Vec<f64>,
    pub hub: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
}

fn compute_l2_norm(vec: &[f64]) -> f64 {
    let sum_sq: f64 = vec.iter().map(|&v| v * v).sum();
    sum_sq.sqrt()
}

fn normalize_vector_l2(vec: &mut [f64]) {
    let norm = compute_l2_norm(vec);
    if norm > 0.0 {
        let inv_norm = 1.0 / norm;
        for v in vec.iter_mut() {
            *v *= inv_norm;
        }
    }
}

fn compute_csr_step(
    offsets: &[u32],
    targets: &[u32],
    weights: &[f32],
    src: &[f64],
    dst: &mut [f64],
) {
    for (i, dst_val) in dst.iter_mut().enumerate() {
        let start = offsets[i] as usize;
        let end = offsets[i + 1] as usize;
        *dst_val = (start..end).fold(0.0, |acc, k| {
            acc + (weights[k] as f64) * src[targets[k] as usize]
        });
    }
    normalize_vector_l2(dst);
}

/// Edge weights for one HITS run, one per entry of the graph's in-edge and out-edge arrays.
struct EdgeWeights<'a> {
    inbound: std::borrow::Cow<'a, [f32]>,
    outbound: std::borrow::Cow<'a, [f32]>,
}

/// Each edge divided by the number of callers of its callee that share the caller's file, so many
/// callers in one file count as one file's reliance. Both directions carry the same weight per edge.
fn file_spread_weights<'a>(graph: &Graph, node_files: &[u32]) -> EdgeWeights<'a> {
    let file_of = |node: u32| node_files.get(node as usize).copied().unwrap_or(u32::MAX);
    let in_edges = &graph.in_edges;
    let mut same_file: std::collections::HashMap<(u32, u32), u32> =
        std::collections::HashMap::new();
    for callee in 0..in_edges.rows {
        let span = in_edges.row_offsets[callee] as usize..in_edges.row_offsets[callee + 1] as usize;
        for &caller in &in_edges.col_indices[span] {
            *same_file
                .entry((callee as u32, file_of(caller)))
                .or_default() += 1;
        }
    }
    let spread = |caller: u32, callee: u32, weight: f32| {
        weight
            / same_file
                .get(&(callee, file_of(caller)))
                .copied()
                .unwrap_or(1) as f32
    };
    let inbound = (0..in_edges.rows)
        .flat_map(|callee| {
            let span =
                in_edges.row_offsets[callee] as usize..in_edges.row_offsets[callee + 1] as usize;
            span.map(move |k| (callee as u32, k))
        })
        .map(|(callee, k)| spread(in_edges.col_indices[k], callee, in_edges.values[k]))
        .collect();
    let outbound = (0..graph.out_offsets.len().saturating_sub(1))
        .flat_map(|caller| {
            let span = graph.out_offsets[caller] as usize..graph.out_offsets[caller + 1] as usize;
            span.map(move |k| (caller as u32, k))
        })
        .map(|(caller, k)| spread(caller, graph.out_targets[k], graph.out_weights[k]))
        .collect();
    EdgeWeights {
        inbound: std::borrow::Cow::Owned(inbound),
        outbound: std::borrow::Cow::Owned(outbound),
    }
}

fn compute_hits_residual(a: &[f64], a_prev: &[f64], h: &[f64], h_prev: &[f64]) -> f64 {
    let mut diff = 0.0;
    for i in 0..a.len() {
        diff += (a[i] - a_prev[i]).abs() + (h[i] - h_prev[i]).abs();
    }
    diff
}

/// Compute Kleinberg HITS (Hubs & Authorities) over the call graph.
pub fn compute_hits(graph: &Graph, config: &HitsConfig) -> HitsResult {
    let weights = EdgeWeights {
        inbound: std::borrow::Cow::Borrowed(&graph.in_edges.values),
        outbound: std::borrow::Cow::Borrowed(&graph.out_weights),
    };
    hits_with_weights(graph, &weights, config)
}

/// HITS where a callee's authority counts the files that call it rather than its call sites:
/// `node_files[node]` is the file of each node, and callers sharing a file split one unit.
pub fn compute_hits_by_file(graph: &Graph, node_files: &[u32], config: &HitsConfig) -> HitsResult {
    hits_with_weights(graph, &file_spread_weights(graph, node_files), config)
}

fn hits_with_weights(graph: &Graph, weights: &EdgeWeights<'_>, config: &HitsConfig) -> HitsResult {
    let n = graph.node_count();
    if n == 0 {
        return HitsResult {
            authority: Vec::new(),
            hub: Vec::new(),
            iterations: 0,
            converged: true,
        };
    }

    let seed = 1.0 / (n as f64).sqrt();
    let mut authority = vec![seed; n];
    let mut hub = vec![seed; n];
    let mut a_prev = vec![0.0; n];
    let mut h_prev = vec![0.0; n];

    let mut iterations = 0;
    let mut converged = false;

    for it in 0..config.max_iter {
        iterations = it + 1;
        a_prev.copy_from_slice(&authority);
        h_prev.copy_from_slice(&hub);

        let in_edges = &graph.in_edges;
        compute_csr_step(
            &in_edges.row_offsets,
            &in_edges.col_indices,
            &weights.inbound,
            &hub,
            &mut authority,
        );
        compute_csr_step(
            &graph.out_offsets,
            &graph.out_targets,
            &weights.outbound,
            &authority,
            &mut hub,
        );

        let residual = compute_hits_residual(&authority, &a_prev, &hub, &h_prev);
        if residual < config.tol {
            converged = true;
            break;
        }
    }

    HitsResult {
        authority,
        hub,
        iterations,
        converged,
    }
}
