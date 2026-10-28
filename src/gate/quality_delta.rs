use crate::domain::{IngestResult, Interner};
use crate::gate::baseline::{
    DEFAULT_BASELINE_FILE, QualitySnapshot, compute_snapshot, read_baseline_file,
    symbol_canonical_key,
};
use crate::gate::rename::{RenameMap, RenamePair, attach_body_fingerprints, pair_renames};
use crate::query::commands::QueryContext;
use crate::serialize::XmlEscapeExt;
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
            let kind = parts[1].split(':').next().unwrap_or(parts[1]).to_string();
            let key = parts[2].to_string();
            let val = parts[3].parse::<u32>().unwrap_or(0);
            insert_ack_alias(&mut acks, &kind, &key, val);
            let mut path_opt = None;
            for p in &parts[4..] {
                if let Some(path_part) = p.strip_prefix("p=") {
                    path_opt = Some(path_part.to_string());
                    let sym_or_path = path_part.split(':').next().unwrap_or(path_part);
                    insert_ack_alias(&mut acks, &kind, sym_or_path, val);
                    insert_ack_alias(&mut acks, &kind, path_part, val);
                }
            }
            entries.push(AckEntry {
                kind,
                key,
                val,
                path: path_opt,
            });
        }
    }
    QualityAcks { acks, entries }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityRegression {
    pub kind: String,
    pub sym_name: String,
    pub file_path: String,
    pub line: u32,
    pub was_val: u32,
    pub now_val: u32,
    pub is_gating: bool,
    pub is_new_symbol: bool,
    /// The baseline name this symbol was renamed from, when `pair_renames` paired it; `None` otherwise.
    pub renamed_from: Option<String>,
}

/// Comprehensive delta report across all quality dimensions.
pub struct QualityDeltaReport {
    pub baseline_source: String,
    pub head_sha: String,
    pub at_stamp: String,
    pub head_basis: Option<&'static str>,
    pub regressions: Vec<QualityRegression>,
    pub stale_acks: Vec<StaleAck>,
    pub acked_count: usize,
    pub gating_count: usize,
    pub preexisting_worse_count: usize,
    pub new_symbol_count: usize,
    pub minor_count: usize,
    pub api_new_surface: usize,
    pub stale_collapse_replay: Option<String>,
    pub baseline_absorbed: usize,
    pub renames: usize,
    pub acked_by_rename: usize,
    /// Merge base the delta was measured from under `--quality-delta=REV`.
    pub base_ref: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DeltaCliOptions<'a> {
    pub scope: Option<&'a str>,
    pub quality_ack: Option<&'a str>,
    pub ack_only: Option<&'a str>,
    pub show_stale: bool,
    /// `REV` or `REV..HEAD`: measure from that merge base instead of the sidecar or HEAD.
    pub base: Option<&'a str>,
}

#[must_use]
pub fn stale_by_kind(stale: &[StaleAck]) -> String {
    let mut by_kind: Vec<(&str, usize)> = Vec::new();
    for sa in stale {
        let kind = sa.kind.split(':').next().unwrap_or(&sa.kind);
        match by_kind.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => by_kind.push((kind, 1)),
        }
    }
    by_kind
        .iter()
        .map(|(k, n)| format!("{k}:{n}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn write_regression_row<W: Write>(r: &QualityRegression, out: &mut W) -> io::Result<()> {
    let origin_attr = if r.is_new_symbol {
        " origin=\"new-symbol\""
    } else {
        ""
    };
    let renamed_attr = r.renamed_from.as_ref().map_or_else(String::new, |old| {
        format!(" renamed_from=\"{}\"", old.xml_escaped())
    });
    let gating_val = usize::from(r.is_gating);
    let bar_val = match r.kind.as_str() {
        "complexity" => 15,
        "verbosity" => 60,
        "nesting" => 4,
        "params" => 5,
        _ => 0,
    };
    writeln!(
        out,
        "  <r kind=\"{}\" sym=\"{}\" p=\"{}:{}\" was=\"{}\" now=\"{}\" gating=\"{}\" bar=\"{bar_val}\"{}{}/>",
        r.kind.xml_escaped(),
        r.sym_name.xml_escaped(),
        r.file_path.xml_escaped(),
        r.line,
        r.was_val,
        r.now_val,
        gating_val,
        origin_attr,
        renamed_attr
    )
}

impl QualityDeltaReport {
    fn collapsed_stale_replay(&self) -> Option<&str> {
        let eligible = self.regressions.is_empty() && !self.stale_acks.is_empty();
        self.stale_collapse_replay.as_deref().filter(|_| eligible)
    }

    /// Root attributes emitted only when set: `head_basis=`, `base_ref=` and the non-zero counts.
    fn optional_attrs(&self) -> String {
        let text = [
            ("head_basis", self.head_basis.map(str::to_string)),
            ("base_ref", self.base_ref.clone()),
            ("baseline_absorbed", nonzero(self.baseline_absorbed)),
            ("renames", nonzero(self.renames)),
            ("acked_by_rename", nonzero(self.acked_by_rename)),
        ];
        text.into_iter()
            .filter_map(|(name, value)| value.map(|v| format!(" {name}=\"{}\"", v.xml_escaped())))
            .collect()
    }

    /// A clean identity delta with nothing to disclose but collapsed stale rows.
    fn is_verdict_only(&self, collapsed: bool) -> bool {
        collapsed
            && self.head_basis == Some("identity")
            && self.acked_count == 0
            && self.baseline_absorbed == 0
            && self.api_new_surface == 0
            && self.renames == 0
            && self.acked_by_rename == 0
    }
}

fn nonzero(n: usize) -> Option<String> {
    (n > 0).then(|| n.to_string())
}

impl crate::serialize::ToXmlString for QualityDeltaReport {
    fn write_xml<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let hb_attr = self.optional_attrs();
        let collapse = self.collapsed_stale_replay();
        let stale_attr = collapse.map_or_else(String::new, |replay| {
            format!(
                " stale_by_kind=\"{}\" stale_next=\"{} --show-stale\"",
                stale_by_kind(&self.stale_acks).xml_escaped(),
                replay.xml_escaped()
            )
        });
        if self.is_verdict_only(collapse.is_some()) {
            return writeln!(
                out,
                "<!-- ripwire-rs quality-delta schema=ripwire-rs.quality-delta/v1: only what the change made WORSE vs baseline=: regressions= minor= gating=; <r kind= sym= p= was= now= gating= bar=>, <sa> acked; with no regressions the <sa> rows collapse to stale_by_kind= and stale_next= prints them. -->\n<quality-delta schema=\"ripwire-rs.quality-delta/v1\" baseline=\"{}\" regressions=\"0\" gating=\"0\" stale=\"{}\" at=\"{}\"{hb_attr}{stale_attr}></quality-delta>",
                self.baseline_source.xml_escaped(),
                self.stale_acks.len(),
                self.at_stamp.xml_escaped()
            );
        }
        writeln!(
            out,
            "<!-- ripwire-rs quality-delta schema=ripwire-rs.quality-delta/v1: only what the change made WORSE vs baseline=: regressions= minor= gating=; <r kind= sym= p= was= now= gating= bar=>, <sa> acked; with no regressions the <sa> rows collapse to stale_by_kind= and stale_next= prints them. baseline_absorbed=N: the pinned sidecar was taken on a dirty tree that already held N gating findings, so a green exit means clean since the pin. -->\n<quality-delta schema=\"ripwire-rs.quality-delta/v1\" baseline=\"{}\" regressions=\"{}\" minor=\"{}\" acked=\"{}\" stale=\"{}\" preexisting-worse=\"{}\" new-symbol=\"{}\" gating=\"{}\" api-new-surface=\"{}\" at=\"{}\"{hb_attr}{stale_attr}>",
            self.baseline_source.xml_escaped(),
            self.regressions.len(),
            self.minor_count,
            self.acked_count,
            self.stale_acks.len(),
            self.preexisting_worse_count,
            self.new_symbol_count,
            self.gating_count,
            self.api_new_surface,
            self.at_stamp.xml_escaped()
        )?;
        for r in &self.regressions {
            write_regression_row(r, out)?;
        }
        if collapse.is_none() {
            for sa in &self.stale_acks {
                writeln!(
                    out,
                    "  <sa kind=\"{}\" key=\"{}\" why=\"{}\" sym=\"{}\" p=\"{}\"/>",
                    sa.kind.xml_escaped(),
                    sa.key.xml_escaped(),
                    sa.why.xml_escaped(),
                    sa.sym.xml_escaped(),
                    sa.path.xml_escaped()
                )?;
            }
        }
        writeln!(out, "</quality-delta>")
    }
}

impl QualityDeltaReport {
    /// True if there are major unacknowledged regressions requiring failure exit code.
    #[must_use]
    pub fn has_gating_failures(&self) -> bool {
        self.gating_count > 0
    }
}

/// Trait encapsulating a numeric quality regression dimension compared between baseline and working snapshots.
pub trait QualityDeltaDimension: Sync {
    fn kind(&self) -> &'static str;
    fn extract(&self, snap: &QualitySnapshot, key: &str) -> u32;
    fn preexisting_floor(&self) -> u32;
    fn is_preexisting_gating(&self, now: u32) -> bool;
    fn new_symbol_floor(&self) -> Option<u32> {
        None
    }
}

pub struct NumericDimensionSpec {
    pub kind: &'static str,
    pub preexisting_floor: u32,
    pub gating_threshold: Option<u32>,
    pub new_symbol_floor: Option<u32>,
    pub extract: fn(&QualitySnapshot, &str) -> u32,
}

impl QualityDeltaDimension for NumericDimensionSpec {
    fn kind(&self) -> &'static str {
        self.kind
    }
    fn extract(&self, snap: &QualitySnapshot, key: &str) -> u32 {
        (self.extract)(snap, key)
    }
    fn preexisting_floor(&self) -> u32 {
        self.preexisting_floor
    }
    fn is_preexisting_gating(&self, now: u32) -> bool {
        self.gating_threshold.is_some_and(|t| now > t)
    }
    fn new_symbol_floor(&self) -> Option<u32> {
        self.new_symbol_floor
    }
}

pub static DELTA_DIMENSIONS: &[&dyn QualityDeltaDimension] = &[
    &NumericDimensionSpec {
        kind: "complexity",
        preexisting_floor: 0,
        gating_threshold: Some(15),
        new_symbol_floor: Some(15),
        extract: |snap, key| snap.ccx_by_sym.get(key).copied().unwrap_or(0),
    },
    &NumericDimensionSpec {
        kind: "verbosity",
        preexisting_floor: 60,
        gating_threshold: None,
        new_symbol_floor: None,
        extract: |snap, key| snap.loc_by_sym.get(key).copied().unwrap_or(0),
    },
    &NumericDimensionSpec {
        kind: "nesting",
        preexisting_floor: 4,
        gating_threshold: Some(6),
        new_symbol_floor: None,
        extract: |snap, key| u32::from(snap.nest_by_sym.get(key).copied().unwrap_or(0)),
    },
    &NumericDimensionSpec {
        kind: "params",
        preexisting_floor: 3,
        gating_threshold: Some(5),
        new_symbol_floor: None,
        extract: |snap, key| u32::from(snap.params_by_sym.get(key).copied().unwrap_or(0)),
    },
];

fn check_symbol_numeric_regressions(
    key: &str,
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    let parts: Vec<&str> = key.split("::").collect();
    let file_path = parts.first().copied().unwrap_or(key).to_string();
    let sym_name = parts.last().copied().unwrap_or(key).to_string();
    let line = working.line_by_sym.get(key).copied().unwrap_or(0);
    let pair = renames.pair_for(key);
    let was_key = renames.was_key(key);
    let is_preexisting = baseline.ccx_by_sym.contains_key(was_key);
    let renamed_from = pair.map(|p| p.old_name.clone());

    for dim in DELTA_DIMENSIONS {
        let now_val = dim.extract(working, key);
        if is_preexisting {
            let was_val = dim.extract(baseline, was_key);
            if now_val > was_val && now_val > dim.preexisting_floor() {
                regressions.push(QualityRegression {
                    kind: dim.kind().to_string(),
                    sym_name: sym_name.clone(),
                    file_path: file_path.clone(),
                    line,
                    was_val,
                    now_val,
                    is_gating: dim.is_preexisting_gating(now_val),
                    is_new_symbol: false,
                    renamed_from: renamed_from.clone(),
                });
            }
        } else if dim.new_symbol_floor().is_some_and(|floor| now_val > floor) {
            regressions.push(QualityRegression {
                kind: dim.kind().to_string(),
                sym_name: sym_name.clone(),
                file_path: file_path.clone(),
                line,
                was_val: 0,
                now_val,
                is_gating: false,
                is_new_symbol: true,
                renamed_from: None,
            });
        }
    }
}

fn check_structural_regressions(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    for dead in &working.dead_symbols {
        let was_key = renames.was_key(dead);
        if !baseline.dead_symbols.contains(was_key) {
            let is_new = !baseline.ccx_by_sym.contains_key(was_key);
            regressions.push(QualityRegression {
                kind: "dead-code".to_string(),
                sym_name: dead.clone(),
                file_path: dead.split("::").next().unwrap_or(dead).to_string(),
                line: working.line_by_sym.get(dead).copied().unwrap_or(0),
                was_val: 0,
                now_val: 1,
                is_gating: !is_new,
                is_new_symbol: is_new,
                renamed_from: renames.pair_for(dead).map(|p| p.old_name.clone()),
            });
        }
    }
    for pub_sym in &working.public_api {
        if baseline.public_api.contains(renames.was_key(pub_sym)) {
            continue;
        }
        regressions.push(QualityRegression {
            kind: "api-surface".to_string(),
            sym_name: pub_sym.split("::").last().unwrap_or(pub_sym).to_string(),
            file_path: pub_sym.split("::").next().unwrap_or(pub_sym).to_string(),
            line: working.line_by_sym.get(pub_sym).copied().unwrap_or(0),
            was_val: 0,
            now_val: 1,
            is_gating: false,
            is_new_symbol: true,
            renamed_from: None,
        });
    }
}

/// Compute quality delta report comparing working tree snapshot against baseline.
#[must_use]
pub fn compute_quality_delta(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
) -> QualityDeltaReport {
    compute_quality_delta_with_acks(working, baseline, baseline_source, None)
}

fn apply_quality_acks(regressions: &mut [QualityRegression], ledger: &QualityAcks) -> usize {
    let mut acked = 0usize;
    for r in regressions {
        let file = r.file_path.split(':').next().unwrap_or(&r.file_path);
        if ledger.is_acknowledged(&r.kind, &r.sym_name, r.now_val)
            || r.renamed_from
                .as_deref()
                .is_some_and(|old| ledger.is_acknowledged(&r.kind, old, r.now_val))
            || ledger.is_acknowledged(&r.kind, &r.file_path, r.now_val)
            || ledger.is_acknowledged(&r.kind, file, r.now_val)
        {
            if r.is_gating {
                acked += 1;
            }
            r.is_gating = false;
        }
    }
    acked
}

fn collect_stale_acks(
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

fn count_delta_categories(regressions: &[QualityRegression]) -> (usize, usize, usize) {
    let mut gating = 0;
    let mut preexisting = 0;
    let mut new_sym = 0;
    for r in regressions {
        if r.is_gating {
            gating += 1;
        }
        if r.is_new_symbol {
            new_sym += 1;
        } else {
            preexisting += 1;
        }
    }
    (gating, preexisting, new_sym)
}

/// Compute quality delta report comparing working tree snapshot against baseline with acks.
#[must_use]
pub fn compute_quality_delta_with_acks(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
    acks: Option<&QualityAcks>,
) -> QualityDeltaReport {
    compute_delta_and_renames(working, baseline, baseline_source, acks).0
}

/// The delta report with the rename pairs it was computed under, so rows added later see the same pairing.
fn compute_delta_and_renames(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
    acks: Option<&QualityAcks>,
) -> (QualityDeltaReport, RenameMap) {
    let renames = pair_renames(working, baseline);
    let mut regressions = Vec::new();

    for key in working.ccx_by_sym.keys() {
        check_symbol_numeric_regressions(key, working, baseline, &renames, &mut regressions);
    }

    check_structural_regressions(working, baseline, &renames, &mut regressions);

    let (acked_count, stale_acks, acked_by_rename) = match acks {
        Some(ledger) => (
            apply_quality_acks(&mut regressions, ledger),
            collect_stale_acks(ledger, &regressions, &renames),
            ledger
                .entries
                .iter()
                .filter(|e| renames.carries(&e.key))
                .count(),
        ),
        None => (0, Vec::new(), 0),
    };
    let (gating_count, preexisting_worse_count, new_symbol_count) =
        count_delta_categories(&regressions);
    let api_new_surface = working
        .public_api
        .iter()
        .filter(|key| !baseline.public_api.contains(renames.was_key(key)))
        .count();
    let short_sha = if working.head_sha.len() >= 9 {
        &working.head_sha[..9]
    } else {
        &working.head_sha
    };
    let at_stamp = if working.dirty {
        format!("{short_sha}+dirty")
    } else {
        short_sha.to_string()
    };
    let minor_count = regressions.len().saturating_sub(gating_count);

    let report = QualityDeltaReport {
        baseline_source: baseline_source.to_string(),
        head_sha: working.head_sha.clone(),
        at_stamp,
        head_basis: None,
        regressions,
        stale_acks,
        acked_count,
        gating_count,
        preexisting_worse_count,
        new_symbol_count,
        minor_count,
        api_new_surface,
        stale_collapse_replay: None,
        baseline_absorbed: 0,
        renames: renames.len(),
        acked_by_rename,
        base_ref: None,
    };
    (report, renames)
}

fn remove_file_entries_from_snapshot(snap: &mut QualitySnapshot, file_prefix: &str) {
    let prefix = format!("{file_prefix}::");
    snap.ccx_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.cx_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.loc_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.nest_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.params_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.dead_symbols.retain(|k| !k.starts_with(&prefix));
    snap.public_api.retain(|k| !k.starts_with(&prefix));
    snap.body_by_sym.retain(|k, _| !k.starts_with(&prefix));
}

fn merge_snapshot_entries(dst: &mut QualitySnapshot, src: QualitySnapshot) {
    dst.ccx_by_sym.extend(src.ccx_by_sym);
    dst.cx_by_sym.extend(src.cx_by_sym);
    dst.loc_by_sym.extend(src.loc_by_sym);
    dst.nest_by_sym.extend(src.nest_by_sym);
    dst.params_by_sym.extend(src.params_by_sym);
    dst.dead_symbols.extend(src.dead_symbols);
    dst.public_api.extend(src.public_api);
    dst.body_by_sym.extend(src.body_by_sym);
}
/// Fingerprints the working callables of the changed files: the rename pairing's "appeared" side.
fn attach_working_bodies(
    working: &mut QualitySnapshot,
    state: &QueryContext<'_>,
    changed_files: &[String],
) {
    if !changed_files.is_empty() {
        attach_body_fingerprints(
            working,
            state.ingest,
            state.interner,
            changed_files,
            state.sources,
        );
    }
}

/// Fingerprints the HEAD callables of the changed files: the rename pairing's "vanished" side. Skipped when the
/// working side has none to pair with (`--dmm` never pairs), so HEAD blobs are not tokenized again.
fn attach_head_bodies(
    delta: &mut QualitySnapshot,
    head_ingest: &IngestResult,
    head_interner: &Interner,
    changed_files: &[String],
    working: &QualitySnapshot,
    slices: &[(&str, crate::domain::Lang, &[u8])],
) {
    if working.body_by_sym.is_empty() {
        return;
    }
    let sources: Vec<(&str, &[u8])> = slices.iter().map(|(p, _, b)| (*p, *b)).collect();
    attach_body_fingerprints(
        delta,
        head_ingest,
        head_interner,
        changed_files,
        &sources.as_slice(),
    );
}

fn head_tree<'a>(repo: &'a gix::Repository) -> Option<gix::Tree<'a>> {
    repo.head_commit().ok()?.tree().ok()
}

fn resolve_rev_tree<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Tree<'a>> {
    match rev {
        "" | "HEAD" => head_tree(repo),
        rev => crate::git::repo::revision_tree(repo, rev).or_else(|| head_tree(repo)),
    }
}

fn collect_tree_blobs(
    tree: &gix::Tree<'_>,
    ingest: &IngestResult,
    interner: &Interner,
    changed_set: &std::collections::HashSet<&str>,
) -> Vec<(String, crate::domain::Lang, Vec<u8>)> {
    let mut rev_files = Vec::new();
    for f in &ingest.files {
        let rel_path = interner.resolve(&f.path);
        if changed_set.contains(rel_path)
            && let Ok(Some(entry)) = tree.lookup_entry_by_path(rel_path)
            && let Ok(obj) = entry.object()
        {
            rev_files.push((rel_path.to_string(), f.lang, obj.data.to_vec()));
        }
    }
    rev_files
}

fn read_changed_rev_blobs<P: AsRef<Path>>(
    root_path: P,
    rev: &str,
    ingest: &IngestResult,
    interner: &Interner,
    changed_set: &std::collections::HashSet<&str>,
) -> Vec<(String, crate::domain::Lang, Vec<u8>)> {
    let Ok(repo) = gix::open(root_path.as_ref()) else {
        return Vec::new();
    };
    let Some(tree) = resolve_rev_tree(&repo, rev) else {
        return Vec::new();
    };
    collect_tree_blobs(&tree, ingest, interner, changed_set)
}

#[must_use]
pub fn load_revision_baseline_incremental(
    root: &str,
    rev: &str,
    ingest: &IngestResult,
    interner: &Interner,
    working: &QualitySnapshot,
    changed_files: &[String],
    head_sha: &str,
) -> QualitySnapshot {
    let mut snap = working.clone();
    snap.head_sha = head_sha.to_string();
    let changed_set: std::collections::HashSet<&str> =
        changed_files.iter().map(String::as_str).collect();
    for cf in &changed_set {
        remove_file_entries_from_snapshot(&mut snap, cf);
    }
    let rev_files = read_changed_rev_blobs(root, rev, ingest, interner, &changed_set);
    if rev_files.is_empty() {
        return snap;
    }
    let head_interner = crate::domain::Interner::new();
    let Ok(registry) = crate::ingest::LanguageRegistry::new() else {
        return snap;
    };
    let slices: Vec<(&str, crate::domain::Lang, &[u8])> = rev_files
        .iter()
        .map(|(p, l, b)| (p.as_str(), *l, b.as_slice()))
        .collect();
    let head_ingest = crate::ingest::ingest_in_memory_files(&slices, &registry, &head_interner);
    let head_graph = crate::graph::Graph::build(&head_ingest, &head_interner);
    let mut delta_snap = compute_snapshot(&head_ingest, &head_graph, &head_interner, head_sha);
    attach_head_bodies(
        &mut delta_snap,
        &head_ingest,
        &head_interner,
        changed_files,
        working,
        &slices,
    );
    for k in delta_snap.ccx_by_sym.keys() {
        if working.dead_symbols.contains(k) {
            delta_snap.dead_symbols.insert(k.clone());
        }
    }
    merge_snapshot_entries(&mut snap, delta_snap);
    snap
}

fn load_head_baseline_incremental(
    root: &str,
    state: &QueryContext<'_>,
    working: &QualitySnapshot,
    changed_files: &[String],
    head_sha: &str,
) -> QualitySnapshot {
    load_revision_baseline_incremental(
        root,
        "HEAD",
        state.ingest,
        state.interner,
        working,
        changed_files,
        head_sha,
    )
}

fn matches_path(path: &str, file: &str) -> bool {
    path == file || path.ends_with(file)
}

pub fn symbol_matches_and_changed(
    sym: &crate::domain::Symbol,
    file_path: &str,
    ingest: &IngestResult,
    interner: &Interner,
    baseline: &QualitySnapshot,
) -> Option<(String, String)> {
    let sym_file = interner.resolve(&ingest.files[sym.file_id as usize].path);
    if !matches_path(sym_file, file_path) {
        return None;
    }
    let sym_name = interner.resolve(&sym.name);
    let key = match sym.scope {
        Some(sc) => format!("{sym_file}::{}::{sym_name}", interner.resolve(&sc)),
        None => format!("{sym_file}::{sym_name}"),
    };
    let &was_ccx = baseline.ccx_by_sym.get(&key)?;
    let was_loc = baseline.loc_by_sym.get(&key).copied().unwrap_or(0);
    if sym.ccx != was_ccx || sym.loc != was_loc {
        Some((sym_name.to_string(), sym_file.to_string()))
    } else {
        None
    }
}

fn find_preexisting_callables_in_file<'a>(
    file_path: &str,
    state: &'a QueryContext<'_>,
    baseline: &QualitySnapshot,
) -> Vec<(&'a crate::domain::Symbol, String, String)> {
    state
        .ingest
        .symbols
        .iter()
        .filter(|sym| {
            sym.kind == crate::domain::SymKind::Function
                || sym.kind == crate::domain::SymKind::Method
        })
        .filter_map(|sym| {
            symbol_matches_and_changed(sym, file_path, state.ingest, state.interner, baseline)
                .map(|(sym_name, sym_file)| (sym, sym_name, sym_file))
        })
        .collect()
}

fn check_churn_regressions(
    state: &QueryContext<'_>,
    baseline: &QualitySnapshot,
    changed_files: &[String],
    regressions: &mut Vec<QualityRegression>,
) {
    let Some(commits) = &state.commits else {
        return;
    };
    if commits.is_empty() {
        return;
    }

    for f in changed_files {
        let commit_count = commits
            .iter()
            .filter(|c| c.changed_files.iter().any(|cf| cf == f || cf.ends_with(f)))
            .count();
        if commit_count < 2 {
            continue;
        }
        for (sym, sym_name, sym_file) in find_preexisting_callables_in_file(f, state, baseline) {
            regressions.push(QualityRegression {
                kind: "short-horizon-churn".to_string(),
                sym_name,
                file_path: sym_file,
                line: sym.line,
                was_val: 0,
                now_val: commit_count as u32,
                is_gating: false,
                is_new_symbol: false,
                renamed_from: None,
            });
        }
    }
}

/// Body-text regression kinds read from code (and, for placeholder markers, comments) only.
#[must_use]
pub fn body_text_kinds(masked: &crate::metrics::source_mask::MaskedSource) -> Vec<&'static str> {
    let code = &masked.code;
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let mut kinds = Vec::new();
    let code_placeholder = ["todo!(", "unimplemented!(", "NotImplementedError"]
        .iter()
        .any(|m| code.contains(m));
    let comment_placeholder = ["TODO:", "FIXME:"]
        .iter()
        .any(|m| masked.comments.contains(m));
    if code_placeholder || comment_placeholder {
        kinds.push("placeholder");
    }
    if [
        "catch{}",
        "catch(_){}",
        "catch(...){}",
        "except:pass",
        ".catch(()=>{})",
    ]
    .iter()
    .any(|m| compact.contains(m))
    {
        kinds.push("error-masking");
    }
    if code.contains("std::unique(") && !code.contains("std::sort(") {
        kinds.push("defect-shape");
    }
    kinds
}

fn check_body_text_regressions(
    sym: &crate::domain::Symbol,
    sym_name: &str,
    file_path: &str,
    body: &str,
    is_new: bool,
    pair: Option<&RenamePair>,
    regressions: &mut Vec<QualityRegression>,
) {
    let masked = crate::metrics::source_mask::mask_source(body, sym.lang);
    for kind in body_text_kinds(&masked) {
        if pair.is_some_and(|p| p.old_text_kinds.contains(&kind)) {
            continue;
        }
        regressions.push(QualityRegression {
            kind: kind.to_string(),
            sym_name: sym_name.to_string(),
            file_path: file_path.to_string(),
            line: sym.line,
            was_val: 0,
            now_val: 1,
            is_gating: false,
            is_new_symbol: is_new,
            renamed_from: pair.map(|p| p.old_name.clone()),
        });
    }
}

pub fn collect_changed_clone_regressions(
    clones: Vec<crate::clones::CloneGroup>,
    state: &QueryContext<'_>,
    changed_files: &[String],
    regressions: &mut Vec<QualityRegression>,
) {
    for cg in clones {
        let Some(first_sym) = cg
            .members
            .first()
            .and_then(|&id| state.ingest.symbols.get(id as usize))
        else {
            continue;
        };
        let fp = state
            .ingest
            .files
            .get(first_sym.file_id as usize)
            .map_or("", |f| state.interner.resolve(&f.path));
        if !changed_files.iter().any(|cf| fp == cf || fp.ends_with(cf)) {
            continue;
        }
        let has_reused_helper = cg.members.iter().any(|&id| state.graph.in_degree(id) >= 2);
        let kind = if has_reused_helper {
            "new-clone-of-reused-helper"
        } else {
            "duplication"
        };
        regressions.push(QualityRegression {
            kind: kind.to_string(),
            sym_name: state.interner.resolve(&first_sym.name).to_string(),
            file_path: fp.to_string(),
            line: first_sym.line,
            was_val: 0,
            now_val: cg.token_count as u32,
            is_gating: false,
            is_new_symbol: true,
            renamed_from: None,
        });
    }
}

pub fn is_changed_callable(
    sym: &crate::domain::Symbol,
    file_path: &str,
    changed_files: &[String],
) -> bool {
    (sym.kind == crate::domain::SymKind::Function || sym.kind == crate::domain::SymKind::Method)
        && changed_files
            .iter()
            .any(|cf| file_path == cf || file_path.ends_with(cf))
}

fn check_source_and_clone_regressions(
    state: &QueryContext<'_>,
    baseline: &QualitySnapshot,
    changed_files: &[String],
    clones: &[crate::clones::CloneGroup],
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    use crate::domain::CorpusView;
    for sym in &state.ingest.symbols {
        let file_path = state.sym_path(sym);
        if is_changed_callable(sym, file_path, changed_files)
            && let Ok(bytes) = state.sources.read(file_path)
            && !sym.decl_bytes(&bytes).is_empty()
        {
            let body = String::from_utf8_lossy(sym.decl_bytes(&bytes));
            let sym_name = state.sym_name(sym);
            let key = format!("{file_path}::{sym_name}");
            let scope = sym.scope.map(|sc| state.interner.resolve(&sc));
            let pair = renames.pair_for(&symbol_canonical_key(sym_name, file_path, scope));
            let is_new = pair.is_none() && !baseline.ccx_by_sym.contains_key(&key);
            check_body_text_regressions(sym, sym_name, file_path, &body, is_new, pair, regressions);
        }
    }
    collect_changed_clone_regressions(clones.to_vec(), state, changed_files, regressions);
}

fn filter_regressions_by_scope(
    root: &str,
    regressions: &mut [QualityRegression],
    scope_spec: &str,
) {
    let changed = crate::git::repo::working_tree_changes(root);
    let pats: Vec<&str> = scope_spec.split(',').map(str::trim).collect();
    for r in regressions {
        let in_scope = if scope_spec == "diff" {
            changed
                .iter()
                .any(|cf| r.file_path == *cf || r.file_path.ends_with(cf))
        } else {
            pats.iter().any(|&p| r.file_path.contains(p))
        };
        if !in_scope {
            r.is_gating = false;
        }
    }
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

fn regression_content_id(state: &QueryContext<'_>, r: &QualityRegression) -> Option<u64> {
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

pub fn stale_replay_for(scope: Option<&str>) -> String {
    match scope {
        Some(s) => format!("--quality-delta --scope={s}"),
        None => "--quality-delta".to_string(),
    }
}

pub fn update_report_counts(report: &mut QualityDeltaReport) {
    report.gating_count = report.regressions.iter().filter(|r| r.is_gating).count();
    report.minor_count = report.regressions.len().saturating_sub(report.gating_count);
    report.preexisting_worse_count = report
        .regressions
        .iter()
        .filter(|r| !r.is_new_symbol)
        .count();
}

pub fn apply_cli_delta_options_with_writer<W: std::io::Write>(
    state: &QueryContext<'_>,
    report: &mut QualityDeltaReport,
    opts: DeltaCliOptions<'_>,
    ack_writer: impl FnOnce() -> io::Result<W>,
) -> io::Result<()> {
    let root = state.root;
    let scope_opt = opts.scope.filter(|s| !s.is_empty());
    if let Some(scope_spec) = scope_opt {
        filter_regressions_by_scope(root, &mut report.regressions, scope_spec);
    }
    if !opts.show_stale {
        report.stale_collapse_replay = Some(stale_replay_for(scope_opt));
    }
    if let Some(reason) = opts.quality_ack {
        let ack_pats = opts.ack_only.map(split_ack_patterns).unwrap_or_default();
        let mut file = ack_writer()?;
        let cid_for = |r: &QualityRegression| regression_content_id(state, r);
        ack_matching_regressions(report, &ack_pats, reason, cid_for, &mut file)?;
    }
    report.gating_count = report.regressions.iter().filter(|r| r.is_gating).count();
    report.minor_count = report.regressions.len().saturating_sub(report.gating_count);
    Ok(())
}

pub fn apply_cli_delta_options(
    state: &QueryContext<'_>,
    report: &mut QualityDeltaReport,
    opts: DeltaCliOptions<'_>,
) -> io::Result<()> {
    apply_cli_delta_options_with_writer(state, report, opts, || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(state.root).join(DEFAULT_ACKS_FILE))
    })
}

pub fn split_ack_patterns(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaBasis {
    PinnedOrHead,
    HeadOnly,
}

pub fn evaluate_quality_delta(
    root: &str,
    state: &QueryContext<'_>,
) -> io::Result<QualityDeltaReport> {
    evaluate_quality_delta_with_basis(root, state, DeltaBasis::PinnedOrHead)
}

#[must_use]
pub fn resolve_delta_head_sha(at_stamp: &str) -> &str {
    at_stamp
        .strip_suffix("+dirty")
        .unwrap_or(if at_stamp.is_empty() {
            "HEAD"
        } else {
            at_stamp
        })
}

pub struct QualityDeltaFacts<'a> {
    pub working: &'a QualitySnapshot,
    pub baseline: &'a QualitySnapshot,
    pub source_name: &'static str,
    pub head_basis: Option<&'static str>,
    pub at_stamp: &'a str,
    pub changed_files: &'a [String],
    pub acks: Option<&'a QualityAcks>,
    pub clones: &'a [crate::clones::CloneGroup],
}

pub fn assemble_quality_delta_report(
    state: &QueryContext<'_>,
    facts: &QualityDeltaFacts<'_>,
) -> QualityDeltaReport {
    let (mut report, renames) =
        compute_delta_and_renames(facts.working, facts.baseline, facts.source_name, facts.acks);
    report.head_basis = facts.head_basis;
    report.baseline_absorbed = facts.baseline.absorbed;
    if !facts.at_stamp.is_empty() {
        report.at_stamp = facts.at_stamp.to_string();
    }
    if !facts.changed_files.is_empty() {
        check_churn_regressions(
            state,
            facts.baseline,
            facts.changed_files,
            &mut report.regressions,
        );
        check_source_and_clone_regressions(
            state,
            facts.baseline,
            facts.changed_files,
            facts.clones,
            &renames,
            &mut report.regressions,
        );
        if let Some(ledger) = facts.acks {
            let _ = apply_quality_acks(&mut report.regressions, ledger);
        }
    }
    update_report_counts(&mut report);
    report
}

fn try_read_pinned_baseline(root: &str, basis: DeltaBasis) -> io::Result<Option<QualitySnapshot>> {
    if basis == DeltaBasis::PinnedOrHead {
        let baseline_path = Path::new(root).join(DEFAULT_BASELINE_FILE);
        if baseline_path.exists() {
            return read_baseline_file(&baseline_path).map(Some);
        }
    }
    Ok(None)
}

fn resolve_quality_baseline(
    root: &str,
    state: &QueryContext<'_>,
    basis: DeltaBasis,
    working: &QualitySnapshot,
    changed_files: &[String],
) -> io::Result<(QualitySnapshot, &'static str, Option<&'static str>)> {
    if let Some(snap) = try_read_pinned_baseline(root, basis)? {
        return Ok((snap, "sidecar", None));
    }
    if changed_files.is_empty() {
        Ok((working.clone(), "git-HEAD", Some("identity")))
    } else {
        let snap =
            load_head_baseline_incremental(root, state, working, changed_files, &working.head_sha);
        Ok((snap, "git-HEAD", None))
    }
}

fn load_quality_acks(root: &str) -> Option<QualityAcks> {
    let acks_path = Path::new(root).join(DEFAULT_ACKS_FILE);
    if acks_path.exists() {
        std::fs::read_to_string(&acks_path)
            .ok()
            .map(|c| parse_acks_content(&c))
    } else {
        None
    }
}

fn collect_delta_clones(
    state: &QueryContext<'_>,
    root: &str,
    has_changes: bool,
) -> Vec<crate::clones::CloneGroup> {
    if !has_changes {
        return Vec::new();
    }
    crate::clones::find_clones_in_root(
        state.ingest,
        state.interner,
        60,
        None,
        Some(Path::new(root)),
    )
}

pub fn evaluate_quality_delta_with_basis(
    root: &str,
    state: &QueryContext<'_>,
    basis: DeltaBasis,
) -> io::Result<QualityDeltaReport> {
    let at_stamp = crate::git::history::compute_git_at_stamp(root).unwrap_or_default();
    let head_sha = resolve_delta_head_sha(&at_stamp);
    let changed_files = crate::git::repo::working_tree_changes(root);
    let mut working = compute_snapshot(state.ingest, state.graph, state.interner, head_sha);
    working.dirty = !changed_files.is_empty();
    attach_working_bodies(&mut working, state, &changed_files);

    let (baseline, source_name, head_basis) =
        resolve_quality_baseline(root, state, basis, &working, &changed_files)?;
    let acks = load_quality_acks(root);
    let clones = collect_delta_clones(state, root, !changed_files.is_empty());

    let facts = QualityDeltaFacts {
        working: &working,
        baseline: &baseline,
        source_name,
        head_basis,
        at_stamp: &at_stamp,
        changed_files: &changed_files,
        acks: acks.as_ref(),
        clones: &clones,
    };
    Ok(assemble_quality_delta_report(state, &facts))
}

/// The delta against the merge base of `base` when given, else against the pinned sidecar or HEAD.
pub fn evaluate_quality_delta_from(
    state: &QueryContext<'_>,
    base: Option<&str>,
) -> io::Result<QualityDeltaReport> {
    match base {
        Some(spec) => evaluate_quality_delta_since_ref(state.root, state, spec),
        None => evaluate_quality_delta(state.root, state),
    }
}

/// Quality delta of the working tree against the merge base of `spec` (`REV` or `REV..HEAD`) and
/// HEAD, rebuilt from git: what everything since that base made worse. No sidecar is read.
pub fn evaluate_quality_delta_since_ref(
    root: &str,
    state: &QueryContext<'_>,
    spec: &str,
) -> io::Result<QualityDeltaReport> {
    let fail = |e: String| io::Error::other(format!("--quality-delta: {e}"));
    let changes = crate::git::repo::changes_since_spec(root, spec).map_err(fail)?;
    let at_stamp = crate::git::history::compute_git_at_stamp(root).unwrap_or_default();
    let head_sha = resolve_delta_head_sha(&at_stamp);
    let changed_files = changes.paths;
    let mut working = compute_snapshot(state.ingest, state.graph, state.interner, head_sha);
    working.dirty = !changed_files.is_empty();
    attach_working_bodies(&mut working, state, &changed_files);

    let (baseline, head_basis) = if changed_files.is_empty() {
        (working.clone(), Some("identity"))
    } else {
        let snap = load_revision_baseline_incremental(
            root,
            &changes.base,
            state.ingest,
            state.interner,
            &working,
            &changed_files,
            head_sha,
        );
        (snap, None)
    };
    let acks = load_quality_acks(root);
    let clones = collect_delta_clones(state, root, !changed_files.is_empty());

    let facts = QualityDeltaFacts {
        working: &working,
        baseline: &baseline,
        source_name: "git-ref",
        head_basis,
        at_stamp: &at_stamp,
        changed_files: &changed_files,
        acks: acks.as_ref(),
        clones: &clones,
    };
    let mut report = assemble_quality_delta_report(state, &facts);
    report.base_ref = Some(changes.base.get(..9).unwrap_or(&changes.base).to_string());
    Ok(report)
}
