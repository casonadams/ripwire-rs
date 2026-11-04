mod comment_coherence;
mod context_ratio;
mod executive;
mod naming;

pub use comment_coherence::{
    CommentCoherenceReport, CommentCoherenceRow, compute_comment_coherence,
    compute_comment_coherence_in_root, compute_comment_coherence_with_sources,
};
pub use context_ratio::{ContextRatioReport, ContextRatioRow, compute_context_ratio};
pub use executive::serialize_executive_report_md;
pub use naming::{classify_identifier_casing, serialize_naming_consistency_xml};
