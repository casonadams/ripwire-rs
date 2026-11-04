pub mod analysis_lenses;
pub mod architecture;
pub mod complexity;
pub mod dmm;
pub mod eval;
pub mod hotspots;
pub mod layout;
pub mod lint;
mod lint_naming;
mod lint_report;
pub mod lint_rules;
pub mod nonlocal;
pub mod quality_panel;
pub mod readability;
pub mod scip_eval;
pub mod source_mask;

pub use analysis_lenses::{
    classify_identifier_casing, compute_comment_coherence, compute_comment_coherence_with_sources,
    compute_context_ratio, serialize_executive_report_md, serialize_naming_consistency_xml,
};
pub use dmm::{DmmProperty, DmmReport, compute_dmm_report, serialize_dmm_xml};
pub use eval::{run_cochange_eval, run_retrieval_eval, run_retrieval_eval_for_repo};
pub use layout::FieldLayout;
pub use quality_panel::{
    PanelHistory, PanelInputs, QualityPanelReport, QualityPanelRow, compute_quality_panel_report,
};

pub use complexity::{
    ComplexityMetrics, NESTING_BAR, compute_complexity, compute_complexity_from_node,
};
pub use hotspots::{HotspotFile, HotspotsReport, compute_hotspots};
pub use lint::{
    LINT_RULES, LintFinding, LintRuleDesc, run_lint_scan_in_root, serialize_lint_catalog,
    serialize_lint_sarif, serialize_lint_xml,
};
pub use nonlocal::{NonlocalFunction, find_nonlocal_state, serialize_nonlocal_state_xml};
pub use readability::{
    ReadabilityMetrics, compute_readability, execute_biggest_first_query, posnett_score,
};
