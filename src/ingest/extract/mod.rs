mod arguments;
mod capture;
mod chains;
mod crossfile;
mod def;
mod macros;
mod params;
mod parse;
mod project;
mod qualifier;
mod refs;
mod scope;
mod types;

pub use capture::{Capture, classify_capture};
pub use parse::parse_file_tree;
pub use project::resolve_project_types;
pub use scope::distinct_scope_tail;
pub use types::{FileParseResult, RawDef, RawRef};
