pub mod corpus;
pub mod test_path;

mod ingest_result;
mod interner;
mod kinds;
mod reference;
mod symbol;

pub use crate::languages::Lang;
pub use corpus::CorpusView;
pub use ingest_result::{
    CallableBinding, DynamicImport, FileInfo, ForeignBinding, Implementation, ImportAlias,
    IngestResult, LocalBinding, ScipSite, ScipTarget, SubmoduleImport,
};
pub use interner::Interner;
pub use kinds::{Binding, RefRole, SymKind};
pub use reference::Reference;
pub use symbol::Symbol;

/// 32-bit index into symbol tables. Not a pointer.
pub type NodeId = u32;

/// Sentinel value indicating no node.
pub const NO_NODE: NodeId = u32::MAX;

/// Name of the synthetic per-file owner of references outside every definition (`SymKind::ModuleScope`).
pub const MODULE_SCOPE_NAME: &str = "<file-scope>";
