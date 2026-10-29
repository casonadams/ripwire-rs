use super::token_stream::{
    SKETCH_HASHES, SKETCH_MIN_MATCHES, compute_jaccard, compute_kgrams_ids, compute_lcs_len,
    compute_sketch, scan_body_ids, sketch_matches,
};
use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::serialize::{PageRequest, PageWindow};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Classification of clone match type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneType {
    Type1 = 1,
    Type2 = 2,
    Type3 = 3,
}

impl CloneType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        if self == Self::Type1 {
            "1"
        } else if self == Self::Type2 {
            "2"
        } else {
            "3"
        }
    }
}

/// Recognized programming idiom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneIdiom {
    None,
    ThresholdLadder,
    SwitchNameTable,
    BuilderChain,
}

impl CloneIdiom {
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::ThresholdLadder => Some("threshold-ladder"),
            Self::SwitchNameTable => Some("switch-name-table"),
            Self::BuilderChain => Some("builder-chain"),
        }
    }
}

/// A detected clone group with its member symbols and metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct CloneGroup {
    pub members: Vec<NodeId>,
    pub clone_type: CloneType,
    pub token_count: usize,
    pub similarity: f64,
    pub idiom: CloneIdiom,
    pub tokens_saved: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BodyTokens {
    pub(crate) id: NodeId,
    pub(crate) raw_hash: u64,
    pub(crate) norm_hash: u64,
    pub(crate) norm_token_ids: Vec<u32>,
    pub(crate) kgrams: Vec<u64>,
    pub(crate) sketch: [u8; SKETCH_HASHES],
}

const ID_IF: u32 = 0x3938_6e06;
const ID_RETURN: u32 = 0x85ee_37bf;
const ID_CASE: u32 = 0x9b25_38b1;
const ID_ARROW: u32 = 0x91f4_de62;
const ID_SWITCH: u32 = 0x93e0_5f71;
const ID_MATCH: u32 = 0x7e20_2f96;
const ID_DOT: u32 = 0x2b0c_98f1;
const ID_EQ: u32 = 0x380c_ad68;

fn count_token(tokens: &[u32], id: u32) -> usize {
    tokens.iter().filter(|&&t| t == id).count()
}

fn classify_body_idiom(tokens: &[u32]) -> CloneIdiom {
    if tokens.len() > 80 {
        return CloneIdiom::None;
    }
    if count_token(tokens, ID_IF) >= 2 && count_token(tokens, ID_RETURN) >= 2 {
        CloneIdiom::ThresholdLadder
    } else if tokens.iter().any(|&t| t == ID_SWITCH || t == ID_MATCH)
        && (count_token(tokens, ID_CASE) + count_token(tokens, ID_ARROW)) >= 2
    {
        CloneIdiom::SwitchNameTable
    } else if count_token(tokens, ID_EQ) >= 3 && count_token(tokens, ID_DOT) >= 3 {
        CloneIdiom::BuilderChain
    } else {
        CloneIdiom::None
    }
}

fn check_group_idiom(members: &[&BodyTokens]) -> CloneIdiom {
    let Some(first_member) = members.first() else {
        return CloneIdiom::None;
    };
    let first = classify_body_idiom(&first_member.norm_token_ids);
    if first == CloneIdiom::None {
        return CloneIdiom::None;
    }
    for m in &members[1..] {
        if classify_body_idiom(&m.norm_token_ids) != first {
            return CloneIdiom::None;
        }
    }
    first
}

fn symbol_body_range(sym: &crate::domain::Symbol, file_len: usize) -> (usize, usize) {
    (
        (sym.sig_end_byte as usize).min(file_len),
        (sym.end_byte as usize).min(file_len),
    )
}

fn extract_file_candidates(
    file_bytes: &[u8],
    syms: &[&crate::domain::Symbol],
    min_tokens: usize,
) -> Vec<BodyTokens> {
    syms.iter()
        .filter_map(|sym| symbol_body_tokens(sym, file_bytes, min_tokens))
        .collect()
}

/// Token shape of a callable's body (the bytes after its signature), or `None` when the body is empty or
/// has fewer than `min_tokens` normalized tokens. The clone detector and rename pairing share this shape.
pub(crate) fn symbol_body_tokens(
    sym: &crate::domain::Symbol,
    file_bytes: &[u8],
    min_tokens: usize,
) -> Option<BodyTokens> {
    let (start, end) = symbol_body_range(sym, file_bytes.len());
    if start >= end {
        return None;
    }
    let (raw_hash, norm_hash, norm_token_ids) = scan_body_ids(&file_bytes[start..end], sym.lang);
    if norm_token_ids.len() < min_tokens {
        return None;
    }
    let kgrams = compute_kgrams_ids(&norm_token_ids, 5);
    let sketch = compute_sketch(&kgrams);
    Some(BodyTokens {
        id: sym.id,
        raw_hash,
        norm_hash,
        norm_token_ids,
        kgrams,
        sketch,
    })
}

fn load_clone_file_bytes(
    file_path: &str,
    sources: Option<&HashMap<String, Vec<u8>>>,
    root: Option<&std::path::Path>,
) -> Option<Vec<u8>> {
    if let Some(map) = sources {
        map.get(file_path).cloned()
    } else if let Some(r) = root {
        crate::domain::read_rel_bytes(r, file_path).ok()
    } else {
        std::fs::read(file_path).ok()
    }
}

fn extract_candidate_bodies(
    ingest: &IngestResult,
    interner: &Interner,
    min_tokens: usize,
    sources: Option<&HashMap<String, Vec<u8>>>,
    root: Option<&std::path::Path>,
) -> Vec<BodyTokens> {
    let mut symbols_by_file: Vec<Vec<&crate::domain::Symbol>> =
        vec![Vec::new(); ingest.files.len()];
    for sym in &ingest.symbols {
        if (sym.kind == SymKind::Function || sym.kind == SymKind::Method)
            && sym.end_byte > sym.sig_end_byte
            && let Some(bucket) = symbols_by_file.get_mut(sym.file_id as usize)
        {
            bucket.push(sym);
        }
    }

    symbols_by_file
        .into_par_iter()
        .enumerate()
        .filter(|(_, syms)| !syms.is_empty())
        .filter_map(|(fid, syms)| {
            let fi = ingest.files.get(fid)?;
            let bytes = load_clone_file_bytes(interner.resolve(&fi.path), sources, root)?;
            Some(extract_file_candidates(&bytes, &syms, min_tokens))
        })
        .flatten()
        .collect()
}

fn group_exact_clones(candidates: &[BodyTokens]) -> Vec<CloneGroup> {
    let mut by_norm: HashMap<u64, Vec<&BodyTokens>> = HashMap::new();
    for c in candidates {
        by_norm.entry(c.norm_hash).or_default().push(c);
    }

    let mut groups = Vec::new();
    for list in by_norm.into_values() {
        if list.len() < 2 {
            continue;
        }
        let first_raw = list[0].raw_hash;
        let is_type1 = list.iter().all(|c| c.raw_hash == first_raw);
        let clone_type = if is_type1 {
            CloneType::Type1
        } else {
            CloneType::Type2
        };
        let members: Vec<NodeId> = list.iter().map(|c| c.id).collect();
        let token_count = list[0].norm_token_ids.len();
        let tokens_saved = (members.len() - 1) * token_count;
        let idiom = check_group_idiom(&list);
        groups.push(CloneGroup {
            members,
            clone_type,
            token_count,
            similarity: 1.0,
            idiom,
            tokens_saved,
        });
    }
    groups.sort_by(|a, b| {
        b.token_count
            .cmp(&a.token_count)
            .then_with(|| a.members.cmp(&b.members))
    });
    groups
}

const TYPE3_MAX_BUCKET: usize = 1024;
const TYPE3_MAX_TOKENS_FOR_LCS: usize = 4096;
/// Bodies shorter than this need a stricter Type-3 floor: fluent chains and loops of a few calls share
/// enough normalized tokens by chance to reach 0.80 between unrelated functions.
const TYPE3_SHORT_BODY_TOKENS: usize = 60;
const TYPE3_FLOOR: f64 = 0.80;
const TYPE3_SHORT_BODY_FLOOR: f64 = 0.85;
const TYPE3_MAX_PAIRS: usize = 200_000;

/// Minimum Jaccard of two bodies' k-gram sets before the exact Type-3 score is computed.
const TYPE3_MIN_JACCARD: f64 = 0.40;

/// Type-3 similarity of two bodies: the LCS ratio over normalized tokens, accepted at or above the floor
/// (the stricter one for short bodies) and below 1.0 (identical bodies are Type-1/2 groups).
pub(crate) fn type3_similarity(ca: &BodyTokens, cb: &BodyTokens) -> Option<f64> {
    if compute_jaccard(&ca.kgrams, &cb.kgrams) < TYPE3_MIN_JACCARD {
        return None;
    }
    let len_a = ca.norm_token_ids.len();
    let len_b = cb.norm_token_ids.len();
    let a_ids = &ca.norm_token_ids[..len_a.min(TYPE3_MAX_TOKENS_FOR_LCS)];
    let b_ids = &cb.norm_token_ids[..len_b.min(TYPE3_MAX_TOKENS_FOR_LCS)];
    let lcs = compute_lcs_len(a_ids, b_ids);
    let similarity = (2.0 * lcs as f64) / (a_ids.len() + b_ids.len()) as f64;

    let floor = if len_a.min(len_b) < TYPE3_SHORT_BODY_TOKENS {
        TYPE3_SHORT_BODY_FLOOR
    } else {
        TYPE3_FLOOR
    };
    (floor..1.0).contains(&similarity).then_some(similarity)
}

fn evaluate_candidate_pair(ca: &BodyTokens, cb: &BodyTokens) -> Option<CloneGroup> {
    let similarity = type3_similarity(ca, cb)?;
    let avg_tokens = (ca.norm_token_ids.len() + cb.norm_token_ids.len()) / 2;
    Some(CloneGroup {
        members: vec![ca.id, cb.id],
        clone_type: CloneType::Type3,
        token_count: avg_tokens,
        similarity,
        idiom: CloneIdiom::None,
        tokens_saved: avg_tokens,
    })
}

fn build_dedup_kgram_index(candidates: &[BodyTokens]) -> HashMap<u64, Vec<u32>> {
    let mut index: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut seen_streams: HashSet<u64> = HashSet::with_capacity(candidates.len());
    for (idx, c) in candidates.iter().enumerate() {
        if !seen_streams.insert(c.norm_hash) {
            continue;
        }
        for &kg in &c.kgrams {
            index.entry(kg).or_default().push(idx as u32);
        }
    }
    index
}

fn try_stage_pair(
    candidates: &[BodyTokens],
    ia: u32,
    ib: u32,
    seen: &mut HashSet<u64>,
    survivors: &mut Vec<u64>,
) {
    let (lo, hi) = if ia < ib { (ia, ib) } else { (ib, ia) };
    let key = ((lo as u64) << 32) | (hi as u64);
    if !seen.insert(key) {
        return;
    }
    if sketch_stage_passes(&candidates[lo as usize], &candidates[hi as usize]) {
        survivors.push(key);
    }
}

/// The clone detector's cheap gate before the exact Type-3 score: bodies within 70% of each other's length
/// and enough matching MinHash components.
pub(crate) fn sketch_stage_passes(ca: &BodyTokens, cb: &BodyTokens) -> bool {
    let len_a = ca.norm_token_ids.len();
    let len_b = cb.norm_token_ids.len();
    if len_a.min(len_b) * 100 < len_a.max(len_b) * 70 {
        return false;
    }
    sketch_matches(&ca.sketch, &cb.sketch) >= SKETCH_MIN_MATCHES
}

fn collect_type3_survivor_pairs(
    candidates: &[BodyTokens],
    index: HashMap<u64, Vec<u32>>,
) -> Vec<u64> {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut survivors = Vec::new();
    let mut buckets: Vec<(u64, Vec<u32>)> = index.into_iter().collect();
    buckets.sort_unstable_by_key(|(kgram, _)| *kgram);
    for (_, bucket) in buckets {
        if bucket.len() < 2 || bucket.len() > TYPE3_MAX_BUCKET {
            continue;
        }
        for (i, &ia) in bucket.iter().enumerate() {
            for &ib in &bucket[i + 1..] {
                if survivors.len() >= TYPE3_MAX_PAIRS {
                    return survivors;
                }
                try_stage_pair(candidates, ia, ib, &mut seen, &mut survivors);
            }
        }
    }
    survivors
}

/// Survivor pairs with at least one side in `focus`, walking each focus body's k-grams in order.
fn collect_focused_survivor_pairs(
    candidates: &[BodyTokens],
    index: &HashMap<u64, Vec<u32>>,
    focus: &[bool],
) -> Vec<u64> {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut survivors = Vec::new();
    for (ia, c) in candidates.iter().enumerate().filter(|(i, _)| focus[*i]) {
        for kg in &c.kgrams {
            let Some(bucket) = index.get(kg).filter(|b| b.len() <= TYPE3_MAX_BUCKET) else {
                continue;
            };
            for &ib in bucket.iter().filter(|&&ib| ib as usize != ia) {
                if survivors.len() >= TYPE3_MAX_PAIRS {
                    return survivors;
                }
                let (lo, hi) = (ia.min(ib as usize) as u32, ia.max(ib as usize) as u32);
                try_stage_pair(candidates, lo, hi, &mut seen, &mut survivors);
            }
        }
    }
    survivors
}

fn evaluate_pairs(candidates: &[BodyTokens], pairs: Vec<u64>) -> Vec<CloneGroup> {
    let mut gapped: Vec<CloneGroup> = pairs
        .into_par_iter()
        .filter_map(|key| {
            let ia = (key >> 32) as usize;
            let ib = (key as u32) as usize;
            evaluate_candidate_pair(&candidates[ia], &candidates[ib])
        })
        .collect();
    gapped.sort_by(|a, b| {
        a.members
            .cmp(&b.members)
            .then_with(|| b.token_count.cmp(&a.token_count))
    });
    gapped
}

/// Clone groups with a member in `focus_files`. Every body can be a partner, but only pairs that
/// touch a focus body are compared, so the pair cap is reached on the change, not the whole tree.
#[must_use]
pub fn find_clones_touching(
    ingest: &IngestResult,
    interner: &Interner,
    min_tokens: usize,
    sources: Option<&HashMap<String, Vec<u8>>>,
    root: Option<&std::path::Path>,
    focus_files: &HashSet<u32>,
) -> Vec<CloneGroup> {
    let mut candidates = extract_candidate_bodies(ingest, interner, min_tokens, sources, root);
    candidates.sort_unstable_by_key(|c| c.id);
    let in_focus = |id: NodeId| {
        ingest
            .symbols
            .get(id as usize)
            .is_some_and(|s| focus_files.contains(&s.file_id))
    };
    let focus: Vec<bool> = candidates.iter().map(|c| in_focus(c.id)).collect();
    let mut groups = group_exact_clones(&candidates);
    groups.retain(|g| g.members.iter().any(|&m| in_focus(m)));
    let index = build_dedup_kgram_index(&candidates);
    groups.extend(evaluate_pairs(
        &candidates,
        collect_focused_survivor_pairs(&candidates, &index, &focus),
    ));
    groups
}

fn find_type3_pairs(candidates: &[BodyTokens]) -> Vec<CloneGroup> {
    let index = build_dedup_kgram_index(candidates);
    let pairs = collect_type3_survivor_pairs(candidates, index);
    evaluate_pairs(candidates, pairs)
}

/// Detect all Type-1, Type-2, and Type-3 clones across the codebase.
#[must_use]
pub fn find_clones(
    ingest: &IngestResult,
    interner: &Interner,
    min_tokens: usize,
    sources: Option<&HashMap<String, Vec<u8>>>,
) -> Vec<CloneGroup> {
    find_clones_in_root(ingest, interner, min_tokens, sources, None)
}

/// Detect all Type-1, Type-2, and Type-3 clones across the codebase in target root.
#[must_use]
pub fn find_clones_in_root(
    ingest: &IngestResult,
    interner: &Interner,
    min_tokens: usize,
    sources: Option<&HashMap<String, Vec<u8>>>,
    root: Option<&std::path::Path>,
) -> Vec<CloneGroup> {
    let mut candidates = extract_candidate_bodies(ingest, interner, min_tokens, sources, root);
    candidates.sort_unstable_by_key(|c| c.id);
    let mut exact_groups = group_exact_clones(&candidates);
    let gapped_groups = find_type3_pairs(&candidates);
    exact_groups.extend(gapped_groups);
    exact_groups
}

fn find_root(parent: &mut HashMap<NodeId, NodeId>, mut x: NodeId) -> NodeId {
    while parent[&x] != x {
        let p = parent[&x];
        let gp = parent[&p];
        parent.insert(x, gp);
        x = p;
    }
    x
}

fn union_group_members(parent: &mut HashMap<NodeId, NodeId>, members: &[NodeId]) {
    let Some(&first) = members.first() else {
        return;
    };
    let r0 = find_root(parent, first);
    for &m in &members[1..] {
        let rm = find_root(parent, m);
        if r0 != rm {
            parent.insert(rm, r0);
        }
    }
}

fn assign_component_gids(groups: &[CloneGroup]) -> (Vec<usize>, usize) {
    let mut parent: HashMap<NodeId, NodeId> = HashMap::new();
    for g in groups {
        for &m in &g.members {
            parent.entry(m).or_insert(m);
        }
    }
    for g in groups {
        union_group_members(&mut parent, &g.members);
    }
    let mut comp_id_map: HashMap<NodeId, usize> = HashMap::new();
    let mut gids = Vec::with_capacity(groups.len());
    for g in groups {
        let gid = if let Some(&first) = g.members.first() {
            let root = find_root(&mut parent, first);
            let next_id = comp_id_map.len() + 1;
            *comp_id_map.entry(root).or_insert(next_id)
        } else {
            0
        };
        gids.push(gid);
    }
    (gids, comp_id_map.len())
}

fn compute_clone_components(
    groups: &[CloneGroup],
    ingest: &IngestResult,
) -> (Vec<usize>, usize, usize, usize) {
    let (gids, comp_count) = assign_component_gids(groups);
    let mut comp_members: HashMap<usize, HashSet<NodeId>> = HashMap::new();
    for (g, &gid) in groups.iter().zip(&gids) {
        comp_members.entry(gid).or_default().extend(&g.members);
    }
    let mut dup_loc = 0usize;
    for members in comp_members.values() {
        let mut sum_loc = 0usize;
        let mut max_loc = 0usize;
        for &m in members {
            let loc = ingest.symbols.get(m as usize).map_or(0, |s| s.loc as usize);
            sum_loc += loc;
            max_loc = max_loc.max(loc);
        }
        dup_loc += sum_loc.saturating_sub(max_loc);
    }
    let total_loc: usize = ingest
        .symbols
        .iter()
        .filter(|s| {
            (s.kind == SymKind::Function || s.kind == SymKind::Method)
                && s.end_byte > s.sig_end_byte
        })
        .map(|s| s.loc as usize)
        .sum();
    (gids, comp_count, dup_loc, total_loc)
}

fn select_windowed_clone_indices(
    groups: &[CloneGroup],
    gids: &[usize],
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> (usize, usize, Vec<String>, PageWindow) {
    let (mut order, type3_idx): (Vec<usize>, Vec<usize>) =
        (0..groups.len()).partition(|&i| groups[i].clone_type != CloneType::Type3);
    let (exact, type3) = (order.len(), type3_idx.len());
    order.extend(type3_idx);
    let rows: Vec<String> = order
        .iter()
        .map(|&idx| {
            let mut row = String::new();
            format_clone_group_xml(&groups[idx], gids[idx], ingest, interner, &mut row);
            row
        })
        .collect();
    let window = crate::serialize::Paged::new(&rows, page, |row| row.len()).window;
    let shown = rows
        .into_iter()
        .skip(window.begin)
        .take(window.shown)
        .collect();
    (exact, type3, shown, window)
}

fn format_clone_group_xml(
    g: &CloneGroup,
    gid: usize,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut String,
) {
    let idiom_attr = match g.idiom.as_str() {
        Some(name) => format!(" idiom=\"{name}\""),
        None => String::new(),
    };
    if g.clone_type == CloneType::Type3 {
        out.push_str(&format!(
            "  <group type=\"3\" gid=\"{gid}\" tokens=\"{}\" n=\"{}\" similarity=\"{:.2}\"{idiom_attr}>\n",
            g.token_count,
            g.members.len(),
            g.similarity,
        ));
    } else {
        out.push_str(&format!(
            "  <group type=\"2\" gid=\"{gid}\" tokens=\"{}\" n=\"{}\"{idiom_attr}>\n",
            g.token_count,
            g.members.len(),
        ));
    }
    use crate::domain::CorpusView;
    use crate::serialize::XmlEscapeExt;
    let view = (ingest, interner);
    for &id in &g.members {
        if let Some(sym) = ingest.sym(id) {
            let name = view.sym_name(sym).xml_escaped();
            let fp = view.sym_path(sym);
            let p = if !fp.is_empty() {
                format!("{}:{}", fp.xml_escaped(), sym.line)
            } else {
                format!("line:{}", sym.line)
            };
            out.push_str(&format!("    <f n=\"{name}\" p=\"{p}\"/>\n"));
        }
    }
    out.push_str("  </group>\n");
}

/// Format clone detection results as Ripwire XML.
#[must_use]
pub fn render_clones_xml(
    groups: &[CloneGroup],
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> String {
    let (gids, clone_groups, dup_loc, total_loc) = compute_clone_components(groups, ingest);
    let (exact_count, type3_count, shown_rows, window) =
        select_windowed_clone_indices(groups, &gids, ingest, interner, page);
    let idiom_groups = groups
        .iter()
        .filter(|g| g.idiom != CloneIdiom::None)
        .count();
    let dup_pct = if total_loc > 0 {
        (dup_loc as f64 * 100.0) / (total_loc as f64)
    } else {
        0.0
    };
    let page_attrs = window.xml_attrs();

    let mut out = format!(
        "<!-- ripwire-rs clones schema=ripwire-rs.clones/v1: similar normalized-token bodies: <group type=2|3 gid= tokens= n= similarity=> of <f n= p=>; dup_loc=/dup_pct=. -->\n<clones schema=\"ripwire-rs.clones/v1\" groups=\"{exact_count}\" type3=\"{type3_count}\" idiom_groups=\"{idiom_groups}\" clone_groups=\"{clone_groups}\" dup_loc=\"{dup_loc}\" total_loc=\"{total_loc}\" dup_pct=\"{dup_pct:.1}\"{page_attrs} root=\".\">\n"
    );

    out.extend(shown_rows);

    out.push_str("</clones>\n");
    out
}
