use crate::serialize::XmlReport;
use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use crate::serialize::{JsonValueExt, Paged, write_paged_rows};
use serde_json::Value;
use std::io::{self, Write};

/// Serialize `doc_drift` out-of-date document references to Ripwire XML.
fn write_doc_drift_extra_attrs<W: Write>(out: &mut W, d: &Value) -> io::Result<()> {
    for key in ["sym", "got", "tgt", "fate", "commit", "date", "subject"] {
        if let Some(v) = d[key].as_str()
            && !v.is_empty()
        {
            write!(out, " {key}=\"{}\"", v.xml_escaped())?;
        }
    }
    Ok(())
}

fn write_doc_drift_anchor<W: Write>(out: &mut W, file: &str, d: &Value) -> io::Result<()> {
    let line = d.u64_any(&["line", "doc_line"], 0);
    let kind = d.str_or("kind", "symbol").xml_escaped();
    let why = d.str_or("why", "undefined").xml_escaped();
    let ref_str = d.str_any(&["referenced", "target_file"], "").xml_escaped();
    write!(
        out,
        "    <a k=\"{kind}\" l=\"{line}\" why=\"{why}\" p=\"{file}\" ref=\"{ref_str}\""
    )?;
    write_doc_drift_extra_attrs(out, d)?;
    writeln!(out, "/>")
}

fn write_doc_drift_header_counts<W: Write>(out: &mut W, val: &Value) -> io::Result<()> {
    let Some(clean) = val["clean_docs"].as_u64() else {
        return Ok(());
    };
    let dated = val.u64_or("dated_docs", 0);
    write!(out, " clean=\"{clean}\" dated=\"{dated}\"")?;
    if let Some(prose) = val["prose_docs"].as_u64() {
        write!(out, " prose=\"{prose}\"")?;
    }
    if let Some(corpus) = val["corpus"].as_u64() {
        write!(out, " corpus=\"{corpus}\"")?;
    }
    if let Some(anchors) = val["anchors"].as_u64() {
        let checked = val.u64_or("checked", 0);
        let unchecked = val.u64_or("unchecked", anchors.saturating_sub(checked));
        write!(
            out,
            " anchors=\"{anchors}\" checked=\"{checked}\" unchecked=\"{unchecked}\""
        )?;
    }
    Ok(())
}

fn group_doc_drift_items(val: &Value) -> Vec<(&str, Vec<&Value>)> {
    let mut grouped: Vec<(&str, Vec<&Value>)> = Vec::new();
    if let Some(items) = val["drift"].as_array() {
        for d in items {
            let doc = d["doc"].as_str().unwrap_or("");
            if let Some((_, list)) = grouped.iter_mut().find(|(p, _)| *p == doc) {
                list.push(d);
            } else {
                grouped.push((doc, vec![d]));
            }
        }
    }
    grouped
}

fn write_doc_drift_group<W: Write>(out: &mut W, doc: &str, list: &[&Value]) -> io::Result<()> {
    let file = escape_xml_str(doc);
    writeln!(out, "  <doc p=\"{file}\" drift=\"{}\">", list.len())?;
    for d in list {
        write_doc_drift_anchor(out, &file, d)?;
    }
    writeln!(out, "  </doc>")
}

pub struct DocDriftReport<'a> {
    pub val: &'a Value,
}

impl XmlReport for DocDriftReport<'_> {
    crate::xml_report_meta!("doc-drift", "ripwire-rs.doc-drift/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::serialize::GitStampExt;
        let count = self.val["drift"].as_array().map_or(0, Vec::len);
        let raw_root = self.val["root"].as_str().unwrap_or(".");
        let root = escape_xml_str(raw_root);
        let at_attr = raw_root.git_at_xml_attr();
        let grouped = group_doc_drift_items(self.val);
        let docs_cnt = self.val["docs_scanned"]
            .as_u64()
            .map_or(grouped.len(), |n| n as usize);
        let paged = Paged::new(&grouped[..], self.val.page_request(), |(doc, list)| {
            crate::serialize::row_bytes(|mut w| write_doc_drift_group(&mut w, doc, list))
        });
        write!(out, " root=\"{root}\" docs=\"{docs_cnt}\"")?;
        write_doc_drift_header_counts(out, self.val)?;
        write!(out, " drift=\"{count}\"")?;
        paged.window.write_xml_attrs(out)?;
        write!(out, "{at_attr}")
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let grouped = group_doc_drift_items(self.val);
        write_paged_rows(
            out,
            &grouped,
            self.val.page_request(),
            |mut w, (doc, list)| write_doc_drift_group(&mut w, doc, list),
        )
    }
}

pub fn serialize_doc_drift_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    DocDriftReport { val }.render_envelope(out)
}
