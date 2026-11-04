pub mod arch_xml;
mod budget;
pub mod defs_xml;
pub mod escape;
mod gauges;
pub mod graph_export;
pub mod html;
mod json;
mod layers;
pub mod legend;
mod paging;
pub mod query_xml;
pub mod workspace_xml;
pub mod writer;
pub mod xml;
mod xml_report;
pub use defs_xml::serialize_defs_xml;
pub use writer::{MinifyingWriter, XmlWriter, strip_xml_comments};

pub use query_xml::{
    AroundSymbolRow, CommunityModule, CommunitySummary, DepsFileRow, DepsReport,
    LEGEND_DICT_ENTRIES, LEGEND_DICT_VERSION, TreeFileSymbols, serialize_around_rows_xml,
    serialize_around_xml, serialize_communities_with_summary_xml, serialize_communities_xml,
    serialize_connect_xml, serialize_dead_code_xml, serialize_deps_report_xml, serialize_deps_xml,
    serialize_doc_drift_xml, serialize_exemplar_xml, serialize_exercises_xml, serialize_expand_xml,
    serialize_external_surface_xml, serialize_flags_xml, serialize_for_xml,
    serialize_from_trace_xml, serialize_grep_xml, serialize_legend_dict_xml, serialize_lego_xml,
    serialize_mentions_xml, serialize_owners_xml, serialize_path_between_xml,
    serialize_recall_text, serialize_safe_delete_xml, serialize_stray_content_xml,
    serialize_test_gate_xml, serialize_whereis_xml,
};

pub use html::{ColorBy, generate_call_graph_html, write_call_graph_html};

pub use budget::{budget_bytes, est_tokens, row_bytes, text_bytes};
pub use escape::{XmlEscapeExt, XmlEscaped, escape_xml_str, escape_xml_write, needs_escaping};
pub use gauges::GraphGaugeProvider;
pub use json::JsonValueExt;
pub use layers::PathLayerExt;
pub use paging::{
    PageRequest, PageWindow, Paged, window_rows, write_paged_rows, write_window_rows,
};
pub use xml::{
    CountingWriter, DEFAULT_PACK_BUDGET_BYTES, DEFAULT_STREAM_BUFFER_CAPACITY, RootElementContext,
    SerializeOptions, StreamXmlContext, bucket_symbols_by_rank, serialize_xml,
    serialize_xml_to_string, serialize_xml_with_ranks, write_file_header, write_root_open,
    write_root_open_with_rank_by, write_symbol_row,
};
pub use xml_report::{ToXmlString, XmlReport};

/// Render into an in-memory buffer. A `Vec` sink cannot fail, so a failure here is a bug in the renderer.
pub(crate) fn render_string(render: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>) -> String {
    let mut out = Vec::new();
    render(&mut out).expect("in-memory serialization should never fail");
    String::from_utf8(out).expect("valid utf-8 output")
}

pub fn write_cdata<W: std::io::Write>(out: &mut W, text: &str) -> std::io::Result<()> {
    out.write_all(b"<![CDATA[")?;
    let mut parts = text.split("]]>");
    if let Some(first) = parts.next() {
        out.write_all(first.as_bytes())?;
    }
    for part in parts {
        out.write_all(b"]]]]><![CDATA[>")?;
        out.write_all(part.as_bytes())?;
    }
    out.write_all(b"]]>")
}

/// Extension trait for a repository root's git stamp. A root that is not a git work tree has no
/// stamp, so callers omit `at=` rather than print a HEAD that does not exist.
pub trait GitStampExt {
    fn git_at_xml_attr(&self) -> String;
}

impl GitStampExt for str {
    fn git_at_xml_attr(&self) -> String {
        git_at_attr(crate::git::history::compute_git_at_stamp(self))
    }
}

/// The `at=` attribute for a git stamp: the stamp, nothing outside a git work tree, or a labelled
/// `at_unavailable` reason when the working tree cannot be read.
#[must_use]
pub fn git_at_attr(stamp: Result<Option<String>, String>) -> String {
    match stamp {
        Ok(Some(stamp)) if !stamp.is_empty() => format!(" at=\"{stamp}\""),
        Ok(_) => String::new(),
        Err(reason) => format!(
            " at_unavailable=\"{}\"",
            crate::serialize::XmlEscaped(&reason)
        ),
    }
}
