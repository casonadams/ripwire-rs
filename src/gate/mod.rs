pub mod affected;
pub mod arch;
pub mod baseline;
pub mod contract;
pub mod edit_check;
pub mod identifier_evidence;
pub mod lanes;
pub mod merge_scout;
pub mod pr_context;
pub mod quality_delta;
pub mod rename;
pub mod runners;
pub mod script_gates;
pub mod seams;
pub mod test_gate;
pub use affected::{
    AffectedReport, AffectedTest, find_affected_tests, find_affected_tests_for_symbols,
};
pub use arch::{ArchConfig, ArchRule, ArchViolation, check_architecture_rules, serialize_arch_xml};
pub use baseline::{
    DEFAULT_BASELINE_FILE, QualitySnapshot, compute_snapshot, read_baseline_file,
    write_baseline_file,
};
pub use contract::{
    ContractStatus, SignatureContract, compare_contracts, load_head_symbol_contract,
};
pub use edit_check::{
    CallerCheckResult, EditCheckReport, IncompatibleCallSite, check_symbol_contract,
};
pub use lanes::{
    OverlapStats, PartitionPlan, compute_plan_lanes, compute_plan_lanes_with_brief,
    measure_partition_overlap, plan_task_partition,
};
pub use merge_scout::{MergeScoutReport, evaluate_merge_scout, serialize_merge_scout_xml};
pub use pr_context::{
    FileHistory, PrContextReport, PrContextRequest, compute_pr_context_from_facts,
    serialize_pr_context_xml,
};
pub use quality_delta::{
    QualityDeltaReport, QualityRegression, compute_quality_delta, evaluate_quality_delta,
};
pub use runners::attach_test_runners;
pub use seams::{SeamsReport, find_integration_seams};
pub use test_gate::{TestGateSummary, summarize_test_gate};
