pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 64 * 1024;
pub const DEFAULT_PACK_BUDGET_BYTES: usize = 8 * 1024;

/// Configuration options for streaming XML serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct SerializeOptions {
    pub root: String,
    pub top_k: Option<usize>,
    pub include_pr: bool,
    pub pack_budget_bytes: Option<usize>,
    pub rank_by: Option<String>,
    pub emit_legend: bool,
    pub legend_full: bool,
    pub map_diff_at: Option<String>,
    pub metrics: bool,
    pub no_inventory: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            root: ".".to_string(),
            top_k: None,
            include_pr: true,
            pack_budget_bytes: Some(DEFAULT_PACK_BUDGET_BYTES),
            rank_by: None,
            emit_legend: false,
            legend_full: false,
            map_diff_at: None,
            metrics: false,
            no_inventory: false,
        }
    }
}
impl SerializeOptions {
    #[inline]
    #[must_use]
    pub fn has_full_pack_budget(&self) -> bool {
        self.pack_budget_bytes
            .is_none_or(|b| b >= DEFAULT_PACK_BUDGET_BYTES)
    }

    #[inline]
    #[must_use]
    pub fn allows_inventory_tier(&self) -> bool {
        self.emit_legend
            && !self.no_inventory
            && self.map_diff_at.is_none()
            && self.rank_by.is_none()
            && self.has_full_pack_budget()
    }
}
