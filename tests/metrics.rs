#[path = "support/b_ingest.rs"]
mod b_ingest;
#[path = "support/b_state.rs"]
mod b_state;
#[path = "support/nonlocal.rs"]
mod nonlocal_support;

mod metrics {
    mod analysis_lenses;
    mod complexity;
    mod dmm;
    mod eval;
    mod hotspots;
    mod layout;
    mod lint;
    mod nonlocal;
    mod nonlocal_python;
    mod nonlocal_typescript;
    mod paging;
    mod quality_panel;
    mod readability;
    mod report_paging;
    mod scip_eval;
    mod source_mask;
}
