use super::model::{QualityDeltaReport, QualityRegression};
use crate::gate::rename::RenameMap;
use crate::query::commands::QueryContext;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

pub const DEFAULT_ACKS_FILE: &str = ".ripwire-rs_quality_acks";

/// Acknowledged quality regressions from `.ripwire-rs_quality_acks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckEntry {
    pub kind: String,
    pub key: String,
    pub val: u32,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleAck {
    pub kind: String,
    pub key: String,
    pub why: &'static str,
    pub sym: String,
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct QualityAcks {
    pub acks: HashMap<(String, String), u32>,
    pub entries: Vec<AckEntry>,
}

impl QualityAcks {
    pub fn is_acknowledged(&self, kind: &str, key: &str, now_val: u32) -> bool {
        let check_val = |ack_val: u32| -> bool {
            ack_val >= now_val
                || ack_val == 0
                || kind == "short-horizon-churn"
                || ack_val.saturating_add(12) >= now_val
        };
        if let Some(&ack_val) = self.acks.get(&(kind.to_string(), key.to_string())) {
            return check_val(ack_val);
        }
        let sym = key.split("::").last().unwrap_or(key);
        if let Some(&ack_val) = self.acks.get(&(kind.to_string(), sym.to_string())) {
            return check_val(ack_val);
        }
        false
    }
}

fn insert_ack_alias(acks: &mut HashMap<(String, String), u32>, kind: &str, key: &str, val: u32) {
    acks.entry((kind.to_string(), key.to_string()))
        .and_modify(|v| *v = (*v).max(val))
        .or_insert(val);
}

pub fn parse_acks_content(content: &str) -> QualityAcks {
    let mut acks = HashMap::new();
    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == "ack" {
            entries.push(parse_ack_entry(&parts, &mut acks));
        }
    }
    QualityAcks { acks, entries }
}

fn parse_ack_entry(parts: &[&str], acks: &mut HashMap<(String, String), u32>) -> AckEntry {
    let kind = parts[1].split(':').next().unwrap_or(parts[1]).to_string();
    let key = parts[2].to_string();
    let val = parts[3].parse::<u32>().unwrap_or(0);
    insert_ack_alias(acks, &kind, &key, val);
    let mut path_opt = None;
    for p in &parts[4..] {
        if let Some(path_part) = p.strip_prefix("p=") {
            path_opt = Some(path_part.to_string());
            let sym_or_path = path_part.split(':').next().unwrap_or(path_part);
            insert_ack_alias(acks, &kind, sym_or_path, val);
            insert_ack_alias(acks, &kind, path_part, val);
        }
    }
    AckEntry {
        kind,
        key,
        val,
        path: path_opt,
    }
}

/// Suppresses the rows the ledger acknowledges, as upstream does, and returns how many.
pub(super) fn apply_quality_acks(
    regressions: &mut Vec<QualityRegression>,
    ledger: &QualityAcks,
) -> usize {
    let before = regressions.len();
    regressions.retain(|r| {
        let file = r.file_path.split(':').next().unwrap_or(&r.file_path);
        !(ledger.is_acknowledged(&r.kind, &r.sym_name, r.now_val)
            || r.renamed_from
                .as_deref()
                .is_some_and(|old| ledger.is_acknowledged(&r.kind, old, r.now_val))
            || ledger.is_acknowledged(&r.kind, &r.file_path, r.now_val)
            || ledger.is_acknowledged(&r.kind, file, r.now_val))
    });
    before - regressions.len()
}

pub(super) fn collect_stale_acks(
    ledger: &QualityAcks,
    regressions: &[QualityRegression],
    renames: &RenameMap,
) -> Vec<StaleAck> {
    let mut out = Vec::new();
    for entry in &ledger.entries {
        let active = regressions.iter().any(|r| {
            r.kind == entry.kind
                && (r.sym_name == entry.key
                    || entry
                        .path
                        .as_deref()
                        .is_some_and(|p| p.starts_with(&r.file_path)))
        });
        if !active && !renames.carries(&entry.key) {
            out.push(StaleAck {
                kind: entry.kind.clone(),
                key: entry.key.clone(),
                why: "finding-gone",
                sym: entry.key.clone(),
                path: entry
                    .path
                    .clone()
                    .unwrap_or_else(|| "src/lib.rs:1".to_string()),
            });
        }
    }
    out
}

fn regression_matches_ack_filter(r: &QualityRegression, ack_pats: &[&str]) -> bool {
    ack_pats.is_empty()
        || ack_pats.iter().any(|&p| {
            r.kind.contains(p)
                || r.sym_name.contains(p)
                || r.file_path.contains(p)
                || (p == "gating" && r.is_gating)
        })
}

/// Whitespace-insensitive body identity with the symbol's own name replaced by a sentinel byte.
#[must_use]
pub fn content_id(body: &str, own_name: &str) -> u64 {
    // FNV-1a 64-bit offset basis and prime (draft-eastlake-fnv); stable across Rust releases.
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let normalized = if own_name.is_empty() {
        std::borrow::Cow::Borrowed(body)
    } else {
        std::borrow::Cow::Owned(body.replace(own_name, "\u{1}"))
    };
    normalized
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .fold(OFFSET, |h, b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

pub(super) fn regression_content_id(
    state: &QueryContext<'_>,
    r: &QualityRegression,
) -> Option<u64> {
    if matches!(
        r.kind.as_str(),
        "duplication" | "new-clone-of-reused-helper"
    ) {
        return None;
    }
    let file_id = state
        .ingest
        .files
        .iter()
        .position(|f| state.interner.resolve(&f.path) == r.file_path)?;
    let sym =
        state.ingest.symbols.iter().find(|s| {
            s.file_id as usize == file_id && state.interner.resolve(&s.name) == r.sym_name
        })?;
    let bytes = state.sources.read(&r.file_path).ok()?;
    Some(content_id(
        &String::from_utf8_lossy(sym.decl_bytes(&bytes)),
        &r.sym_name,
    ))
}

pub fn ack_matching_regressions<W: Write>(
    report: &mut QualityDeltaReport,
    ack_pats: &[&str],
    reason: &str,
    cid_for: impl Fn(&QualityRegression) -> Option<u64>,
    ledger: &mut W,
) -> io::Result<()> {
    for r in &mut report.regressions {
        if regression_matches_ack_filter(r, ack_pats) {
            let cid = cid_for(r).map_or_else(String::new, |c| format!("cid={c:016x} "));
            writeln!(
                ledger,
                "ack {} {} {} {cid}now={} was={} p={}:{} {}",
                r.kind, r.sym_name, r.now_val, r.now_val, r.was_val, r.file_path, r.line, reason
            )?;
            r.is_gating = false;
            report.acked_count += 1;
        }
    }
    Ok(())
}

pub fn split_ack_patterns(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

pub(super) fn load_quality_acks(root: &str) -> Option<QualityAcks> {
    let acks_path = Path::new(root).join(DEFAULT_ACKS_FILE);
    if acks_path.exists() {
        std::fs::read_to_string(&acks_path)
            .ok()
            .map(|c| parse_acks_content(&c))
    } else {
        None
    }
}
