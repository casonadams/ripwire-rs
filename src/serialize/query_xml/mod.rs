pub use crate::serialize::arch_xml::*;
pub use crate::serialize::legend::*;
pub use crate::serialize::workspace_xml::*;

mod for_report;
mod from_trace;
mod graph_walk;
mod grep;
mod recall;

pub use for_report::{ForReport, serialize_for_xml};
pub use from_trace::serialize_from_trace_xml;
pub use graph_walk::{
    ConnectReport, serialize_connect_xml, serialize_exemplar_xml, serialize_lego_xml,
    serialize_path_between_xml,
};
pub use grep::{GrepReport, serialize_grep_xml};
pub use recall::serialize_recall_text;
