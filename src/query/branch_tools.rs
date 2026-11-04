mod history;
mod owners;
mod stray;
mod whereis;

pub use history::name_fate;
pub(super) use history::probe_name_history;
pub use owners::{call_owners, recency_weight, single_owner_flag};
pub use stray::{call_stray_content, is_stray_candidate, stray_content_value};
pub use whereis::{
    WhereisFacts, WhereisRequest, call_whereis, index_def_lines, validate_whereis_listing,
    whereis_from_sources_with_facts, whereis_with_sources_and_history,
};
