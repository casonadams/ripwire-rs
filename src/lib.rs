pub mod cache;
pub mod cli;
pub mod clones;
pub mod commands;
pub mod crawler;
pub mod domain;
pub mod gate;
pub mod git;
pub mod graph;
pub mod ingest;
pub mod languages;
pub mod mcp;
pub mod metrics;
pub mod navigate;
pub mod query;
pub mod rank;
pub mod resolve;
pub mod serialize;
pub mod skills;
pub mod slice;
pub mod workspace;
pub use cli::{Cli, parse_from, run_cli, run_cli_args};
pub use clones::{
    CloneGroup, CloneIdiom, CloneType, DEFAULT_MIN_CLONE_TOKENS, execute_clones_command,
    find_clones, find_clones_in_root, render_clones_xml,
};
pub use crawler::{
    CrawledFile, CrawlerOptions, DEFAULT_MAX_FILE_SIZE, DENYLIST_DIRS, crawl_directory,
};
pub use domain::{
    CorpusView, FileInfo, IngestResult, Interner, Lang, MODULE_SCOPE_NAME, NO_NODE, NodeId,
    RefRole, Reference, SymKind, Symbol,
};
pub use gate::{
    AffectedReport, AffectedTest, CallerCheckResult, ContractStatus, DEFAULT_BASELINE_FILE,
    EditCheckReport, IncompatibleCallSite, QualityDeltaReport, QualityRegression, QualitySnapshot,
    SignatureContract, check_symbol_contract, compare_contracts, compute_quality_delta,
    compute_snapshot, evaluate_quality_delta, find_affected_tests, find_affected_tests_for_symbols,
    is_test_path, load_head_symbol_contract, read_baseline_file, write_baseline_file,
};
pub use git::cochange::{
    CoPair, CoPartner, CochangeOptions, StaticCouplingMap, check_pair_dependency_capability,
    mine_cochange_for_file, mine_repo_cochange_pairs,
};
pub use git::history::{
    CHURN_DECAY_HALF_LIFE_DAYS, CommitInfo, DEFAULT_CHURN_MONTHS, DEFAULT_COCHANGE_MONTHS,
    FileChurn, GitHistoryError, MERGE_BOMB_MAX_FILES, SinceScope, civil_from_days,
    compute_file_churn, days_from_civil, filter_commits_by_scope, months_before_epoch,
    parse_since_scope, walk_repository_history,
};
pub use graph::{CsrMatrix, Graph};
pub use ingest::{LanguageRegistry, ingest_crawled_files, ingest_in_memory_files};
pub use mcp::{dispatch_json_rpc, run_mcp_stdio, run_mcp_stream};
pub use metrics::{
    ComplexityMetrics, HotspotFile, HotspotsReport, NESTING_BAR, ReadabilityMetrics,
    compute_complexity, compute_hotspots, compute_readability, posnett_score,
};
pub use navigate::{
    ImpactNode, NavNeighborhood, SymbolLookup, SymbolNavInfo, find_symbols_by_name,
    get_symbol_nav_info, navigate_1hop, serialize_uses_xml, transitive_impact,
};
pub use rank::{
    HitsConfig, HitsResult, PageRankConfig, PageRankResult, biased_teleport, compute_hits,
    compute_hits_by_file, page_rank, uniform_teleport,
};
pub use resolve::{EdgeAcc, EdgeProv, ResolutionResult, ResolvedRef, resolve_all_references};
pub use serialize::{
    ColorBy, CommunityModule, CountingWriter, DEFAULT_PACK_BUDGET_BYTES, RootElementContext,
    SerializeOptions, StreamXmlContext, ToXmlString, TreeFileSymbols, escape_xml_str,
    escape_xml_write, generate_call_graph_html, serialize_around_xml, serialize_communities_xml,
    serialize_connect_xml, serialize_dead_code_xml, serialize_deps_xml, serialize_doc_drift_xml,
    serialize_exemplar_xml, serialize_exercises_xml, serialize_expand_xml,
    serialize_external_surface_xml, serialize_flags_xml, serialize_for_xml,
    serialize_from_trace_xml, serialize_grep_xml, serialize_lego_xml, serialize_mentions_xml,
    serialize_owners_xml, serialize_pack_top_n_xml, serialize_path_between_xml,
    serialize_recall_text, serialize_safe_delete_xml, serialize_stray_content_xml,
    serialize_test_gate_xml, serialize_whereis_xml, serialize_xml, serialize_xml_to_string,
    serialize_xml_with_ranks, write_call_graph_html,
};
pub use skills::{SkillInstallReport, default_skills_dir, install_skills, resolve_skills_dir};
pub use slice::{
    OccRole, SliceContext, SliceFlowDir, SliceRequest, SliceStep, VariableOcc,
    compute_dataflow_slice, parse_slice_target, query_slice,
};
pub use workspace::Workspace;
