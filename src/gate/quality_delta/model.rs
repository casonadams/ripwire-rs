use super::acks::{QualityAcks, StaleAck};
use crate::gate::baseline::QualitySnapshot;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QualityRegression {
    pub kind: String,
    pub sym_name: String,
    pub file_path: String,
    pub line: u32,
    pub was_val: u32,
    pub now_val: u32,
    pub is_gating: bool,
    /// `sev="minor"`: a small change on a symbol that existed at the baseline; never gates.
    pub is_minor: bool,
    pub is_new_symbol: bool,
    /// Clone kinds name the whole group: every member's name, in group order.
    pub members: Vec<String>,
    /// The recognized body shape every member of a clone group spells (`idiom=`).
    pub idiom: Option<&'static str>,
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

impl QualityDeltaReport {
    /// True if there are major unacknowledged regressions requiring failure exit code.
    #[must_use]
    pub fn has_gating_failures(&self) -> bool {
        self.gating_count > 0
    }
}

pub fn update_report_counts(report: &mut QualityDeltaReport) {
    let count = |keep: fn(&QualityRegression) -> bool| {
        report.regressions.iter().filter(|r| keep(r)).count()
    };
    report.gating_count = count(|r| r.is_gating);
    report.minor_count = count(|r| r.is_minor);
    report.new_symbol_count = count(|r| r.is_new_symbol);
    report.preexisting_worse_count = report.regressions.len() - report.new_symbol_count;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaBasis {
    PinnedOrHead,
    HeadOnly,
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
