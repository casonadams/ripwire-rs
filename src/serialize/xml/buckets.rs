use super::options::SerializeOptions;
use super::stream::StreamXmlContext;
use crate::domain::{NodeId, Symbol};

/// Compute symbol rank ordering and bucket symbols into files.
pub fn bucket_symbols_by_rank(
    symbols: &[Symbol],
    files_len: usize,
    ranks: &[f64],
    top_k: Option<usize>,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    let s = symbols.len();
    let keep = top_k.unwrap_or(s).min(s);

    let mut order: Vec<NodeId> = (0..s as NodeId)
        .filter(|&id| symbols[id as usize].is_code_symbol())
        .collect();
    order.sort_by(|&a, &b| {
        let ra = ranks.get(a as usize).copied().unwrap_or(0.0);
        let rb = ranks.get(b as usize).copied().unwrap_or(0.0);
        rb.total_cmp(&ra).then_with(|| a.cmp(&b))
    });
    let keep = keep.min(order.len());

    let mut buckets: Vec<Vec<NodeId>> = vec![Vec::new(); files_len];
    let mut file_order: Vec<u32> = Vec::new();
    let mut seen_files = vec![false; files_len];

    for &id in &order[..keep] {
        let f = symbols[id as usize].file_id;
        if (f as usize) < seen_files.len() && !seen_files[f as usize] {
            seen_files[f as usize] = true;
            file_order.push(f);
        }
        if (f as usize) < buckets.len() {
            buckets[f as usize].push(id);
        }
    }

    (file_order, buckets)
}

fn compute_effective_map_ranks(
    ctx: &StreamXmlContext<'_>,
    options: &SerializeOptions,
) -> Option<Vec<f64>> {
    use crate::domain::CorpusView;
    if options.map_diff_at.is_some()
        || !ctx
            .ingest
            .symbols
            .iter()
            .any(|s| ctx.is_production_symbol(s))
    {
        return None;
    }
    Some(
        ctx.ingest
            .symbols
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let r = ctx.ranks.get(i).copied().unwrap_or(0.0);
                if ctx.is_production_symbol(s) {
                    r
                } else {
                    r * 0.05
                }
            })
            .collect(),
    )
}

/// Stream minified XML with custom ranks and budget enforcement.
fn bucket_symbols_stable(
    ctx: &StreamXmlContext<'_>,
    top_k: Option<usize>,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    use crate::domain::CorpusView;
    let files_len = ctx.ingest.files.len();
    let mut file_order: Vec<u32> = (0..files_len as u32).collect();
    file_order.sort_by(|&a, &b| ctx.file_path(a).cmp(ctx.file_path(b)));
    let mut buckets: Vec<Vec<NodeId>> = vec![Vec::new(); files_len];
    let keep = top_k.unwrap_or(ctx.ingest.symbols.len());
    for sym in ctx
        .ingest
        .symbols
        .iter()
        .filter(|s| s.is_code_symbol())
        .take(keep)
    {
        if let Some(b) = buckets.get_mut(sym.file_id as usize) {
            b.push(sym.id);
        }
    }
    (file_order, buckets)
}

pub(super) fn select_map_buckets(
    ctx: &StreamXmlContext<'_>,
    options: &SerializeOptions,
) -> (Vec<u32>, Vec<Vec<NodeId>>) {
    if !options.include_pr {
        return bucket_symbols_stable(ctx, options.top_k);
    }
    let adjusted = compute_effective_map_ranks(ctx, options);
    let bucket_ranks = adjusted.as_deref().unwrap_or(ctx.ranks);
    bucket_symbols_by_rank(
        &ctx.ingest.symbols,
        ctx.ingest.files.len(),
        bucket_ranks,
        options.top_k,
    )
}
