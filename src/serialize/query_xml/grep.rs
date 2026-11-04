use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use crate::serialize::{JsonValueExt, Paged, row_bytes};
use serde_json::Value;
use std::io::{self, Write};

fn write_grep_context<W: Write>(
    out: &mut W,
    tag: &str,
    rows: Option<&Vec<Value>>,
) -> io::Result<()> {
    for t in rows.into_iter().flatten().filter_map(Value::as_str) {
        write!(out, "<{tag}>")?;
        crate::serialize::write_cdata(out, t)?;
        write!(out, "</{tag}>")?;
    }
    Ok(())
}

fn write_grep_hit<W: Write>(out: &mut W, run: &[&Value]) -> io::Result<()> {
    let h = run[0];
    let line = h.u64_or("line", 1);
    write!(out, "<hit l=\"{line}\"")?;
    if let Some(e) = h.opt_str("enclosing").filter(|e| !e.is_empty()) {
        write!(out, " in=\"{}\"", e.xml_escaped())?;
    }
    if run.len() > 1 {
        write!(out, " n=\"{}\"", run.len())?;
    }
    out.write_all(b">")?;
    write_grep_context(out, "b", h["before"].as_array())?;
    crate::serialize::write_cdata(out, h.str_or("text", ""))?;
    write_grep_context(out, "a", h["after"].as_array())?;
    for repeat in &run[1..] {
        let repeat_line = repeat.u64_or("line", 1);
        write!(out, "<at l=\"{repeat_line}\"")?;
        if let Some(e) = repeat.opt_str("enclosing").filter(|e| !e.is_empty()) {
            write!(out, " in=\"{}\"", e.xml_escaped())?;
        }
        out.write_all(b"/>")?;
    }
    out.write_all(b"</hit>")
}

fn same_run_text(a: &Value, b: &Value) -> bool {
    a.str_any(&["file", "path"], "") == b.str_any(&["file", "path"], "")
        && a.str_or("text", "") == b.str_or("text", "")
}

fn write_grep_rows<W: Write>(out: &mut W, rows: &[Value], merge: bool) -> io::Result<()> {
    let mut i = 0;
    while i < rows.len() {
        let path = rows[i].str_any(&["file", "path"], "");
        write!(out, "<f p=\"{}\">", path.xml_escaped())?;
        while i < rows.len() && rows[i].str_any(&["file", "path"], "") == path {
            let mut run = vec![&rows[i]];
            while merge
                && i + run.len() < rows.len()
                && same_run_text(&rows[i], &rows[i + run.len()])
            {
                run.push(&rows[i + run.len()]);
            }
            i += run.len();
            write_grep_hit(out, &run)?;
        }
        writeln!(out, "</f>")?;
    }
    Ok(())
}

fn write_grep_enc_row<W: Write>(
    out: &mut W,
    name: &str,
    info: &Value,
    defs: usize,
) -> io::Result<()> {
    let n = name.xml_escaped();
    write!(out, "<enc n=\"{n}\"")?;
    if let Some(c) = info["callers"].as_u64() {
        write!(out, " callers=\"{c}\"")?;
    }
    if info["floor"].as_bool() == Some(true) {
        let next = info.str_or("floor_next", "").xml_escaped();
        write!(out, " callers_floor=\"1\" floor_next=\"{next}\"")?;
    }
    if defs > 1 {
        write!(out, " defs=\"{defs}\"")?;
    }
    let cx = info.u64_or("cx", 0);
    if cx > 0 {
        write!(out, " cx=\"{cx}\"")?;
    }
    let reads = info.u64_or("reads", 0);
    if reads > 0 {
        let at: Vec<&str> = info["reads_at"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        write!(
            out,
            " reads=\"{reads}\" reads_at=\"{}\"",
            at.join(" ").xml_escaped()
        )?;
    }
    writeln!(out, "/>")
}

fn grep_enc_order(shown: &[Value]) -> Vec<(&str, usize)> {
    let mut order: Vec<(&str, std::collections::BTreeSet<u64>)> = Vec::new();
    for h in shown {
        let Some(name) = h.opt_str("enclosing").filter(|e| !e.is_empty()) else {
            continue;
        };
        let idx = order
            .iter()
            .position(|(n, _)| *n == name)
            .unwrap_or_else(|| {
                order.push((name, std::collections::BTreeSet::new()));
                order.len() - 1
            });
        if let Some(id) = h["enc_id"].as_u64() {
            order[idx].1.insert(id);
        }
    }
    order.into_iter().map(|(n, ids)| (n, ids.len())).collect()
}

fn write_grep_enc_rows<W: Write>(out: &mut W, val: &Value, shown: &[Value]) -> io::Result<()> {
    for (name, defs) in grep_enc_order(shown) {
        write_grep_enc_row(out, name, &val["enc"][name], defs)?;
    }
    Ok(())
}

fn first_reads_carrier<'a>(val: &'a Value, shown: &'a [Value]) -> Option<&'a str> {
    grep_enc_order(shown)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| val["enc"][*n].u64_or("reads", 0) > 0)
}

pub struct GrepReport<'a> {
    pub val: &'a Value,
}

impl GrepReport<'_> {
    fn hits(&self) -> &[Value] {
        self.val["hits"].as_array().map_or(&[], Vec::as_slice)
    }

    fn unindexed(&self) -> &[Value] {
        self.val["unindexed"].as_array().map_or(&[], Vec::as_slice)
    }

    fn hit_page(&self) -> Paged<'_, Value> {
        Paged::new(self.hits(), self.val.page_request(), |h| {
            row_bytes(|mut w| write_grep_hit(&mut w, &[h]))
        })
    }

    fn unindexed_page(&self) -> Paged<'_, Value> {
        Paged::new(self.unindexed(), self.val.page_request().nested(), |r| {
            row_bytes(|mut w| write_grep_rows(&mut w, std::slice::from_ref(r), false))
        })
    }

    fn shown(&self) -> &[Value] {
        self.hit_page().rows()
    }

    fn next_hint(&self) -> String {
        let pat = self.val.str_or("pattern", "");
        let page = self.hit_page();
        let w = page.window;
        let shown = page.rows();
        if w.has_more {
            let verb = self.val.str_or("verb", "grep");
            format!("--{verb}={pat} --offset={} --legend=compact", w.next_offset)
        } else if let Some(name) = first_reads_carrier(self.val, shown) {
            format!("--uses={name}")
        } else if let Some(h) = shown.first() {
            format!(
                "--at={}:{}",
                h.str_any(&["file", "path"], ""),
                h.u64_or("line", 1)
            )
        } else {
            format!("--for={pat}")
        }
    }

    fn write_tier_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for key in ["suppressed_comment", "suppressed_string"] {
            if let Some(n) = self.val[key].as_u64() {
                write!(out, " {key}=\"{n}\"")?;
            }
        }
        let suppressed = self.val.get("suppressed_comment").is_some()
            || self.val.get("suppressed_string").is_some();
        let w = self.hit_page().window;
        if self.val["literal"].as_bool() == Some(true) && !suppressed && !w.capped {
            out.write_all(b" complete=\"1\"")?;
        }
        if let Some(t) = self.val.opt_str("tier") {
            write!(out, " tier=\"{t}\"")?;
        }
        for key in ["string_hits", "tier_parsed", "tier_unclassified"] {
            if let Some(n) = self.val[key].as_u64() {
                write!(out, " {key}=\"{n}\"")?;
            }
        }
        let pruned = self.val.u64_or("corpus_pruned_dirs", 0);
        if pruned > 0 {
            write!(out, " corpus_pruned_dirs=\"{pruned}\"")?;
        }
        if let Some(scanned) = self.val["unindexed_files_scanned"].as_u64() {
            let hits = self.unindexed().len();
            write!(
                out,
                " unindexed_hits=\"{hits}\" unindexed_files_scanned=\"{scanned}\""
            )?;
        }
        if self.val["unindexed_candidates_capped"].as_bool() == Some(true) {
            out.write_all(b" unindexed_candidates_capped=\"1\"")?;
        }
        Ok(())
    }

    fn write_unindexed<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let rows = self.unindexed();
        if rows.is_empty() {
            return Ok(());
        }
        let page = self.unindexed_page();
        let w = page.window;
        writeln!(
            out,
            "<unindexed count=\"{}\" shown=\"{}\" capped=\"{}\">",
            rows.len(),
            w.shown,
            u8::from(w.capped)
        )?;
        write_grep_rows(out, page.rows(), self.merges())?;
        writeln!(out, "</unindexed>")
    }

    fn merges(&self) -> bool {
        self.val.get("limit").is_none()
            && self.val.get("offset").is_none()
            && self.val["context"].as_bool() != Some(true)
    }
}

impl crate::serialize::XmlReport for GrepReport<'_> {
    crate::xml_report_meta!("grep", "ripwire-rs.grep/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let hits = self.hits();
        let files = hits
            .iter()
            .map(|h| h.str_any(&["file", "path"], ""))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let page = self.hit_page();
        write!(
            out,
            " pattern=\"{}\" root=\"{}\" files=\"{files}\" hits=\"{}\"{} hits_capped=\"{}\"",
            escape_xml_str(self.val.str_or("pattern", "")),
            escape_xml_str(self.val.str_or("root", ".")),
            hits.len(),
            page.attrs(),
            u8::from(page.window.capped),
        )?;
        self.write_tier_attrs(out)?;
        write!(out, " next=\"{}\"", escape_xml_str(&self.next_hint()))
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let shown = self.shown();
        write_grep_rows(out, shown, self.merges())?;
        self.write_unindexed(out)?;
        write_grep_enc_rows(out, self.val, shown)?;
        if self.hits().is_empty() {
            let pat = escape_xml_str(self.val.str_or("pattern", ""));
            writeln!(out, "<suggest next=\"--for=&quot;{pat}&quot;\"/>")?;
        }
        Ok(())
    }
}

pub fn serialize_grep_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    GrepReport { val }.render_envelope(out)
}
