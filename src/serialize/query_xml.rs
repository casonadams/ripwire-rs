pub use super::arch_xml::*;
pub use super::legend::*;
pub use super::workspace_xml::*;

use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use crate::serialize::{JsonValueExt, PageRequest, PageWindow, Paged, row_bytes};
use serde_json::Value;
use std::io::{self, Write};
fn write_for_sig_metrics<W: Write>(out: &mut W, it: &Value) -> io::Result<()> {
    let ccx = it.u64_or("ccx", 0);
    let in_deg = it.u64_or("in", 0);
    let churn = it.u64_or("churn", 1);
    let amp = it.u64_or("amp", 1);
    if ccx > 0 {
        write!(out, " ccx=\"{ccx}\"")?;
    }
    if in_deg > 0 {
        write!(out, " in=\"{in_deg}\"")?;
    }
    if churn > 1 {
        write!(out, " churn=\"{churn}\"")?;
    }
    if amp > 1 {
        write!(out, " amp=\"{amp}\"")?;
    }
    if it.u64_or("tested", 0) > 0 {
        out.write_all(b" tested=\"1\"")?;
    }
    Ok(())
}

/// Serialize `for` task-lens ranking result to Ripwire XML.
fn write_for_sig_row<W: Write>(out: &mut W, idx: usize, it: &Value) -> io::Result<()> {
    use crate::serialize::PathLayerExt;
    let raw_name = it.str_or("name", "");
    let path = it.str_or("path", "");
    let line = it.u64_or("line", 0);
    let end_line = it.u64_or("end_line", line);
    let r = idx + 1;
    write!(out, "<d l=\"{line}\"")?;
    if end_line > line {
        write!(out, " e=\"{end_line}\"")?;
    }
    write!(
        out,
        " n=\"{}\" p=\"{}\"",
        raw_name.xml_escaped(),
        path.xml_escaped(),
    )?;
    path.write_layer_attr(out)?;
    write_for_sig_metrics(out, it)?;
    write!(
        out,
        " r=\"{r}\">{}</d>",
        it.str_or("sig", raw_name).xml_escaped()
    )
}
/// The window the retrieval layer cut the ranked symbols to, with the totals the bundle discloses.
fn for_window(val: &Value) -> PageWindow {
    let shown = val["symbols"].as_array().map_or(0, Vec::len);
    let window = &val["window"];
    let begin = window.u64_or("begin", 0) as usize;
    let total = window.u64_or("total", shown as u64) as usize;
    let end = begin + shown;
    PageWindow {
        begin,
        end,
        shown,
        total,
        capped: shown < total,
        has_more: end < total,
        next_offset: end,
        limit: window.u64_or("limit", 0) as usize,
        offset: window.u64_or("offset", 0) as usize,
    }
}

fn write_for_inner_body<W: Write>(
    out: &mut W,
    val: &Value,
    task: &str,
    raw_task: &str,
) -> io::Result<()> {
    let page = val.page_request();
    let window = for_window(val);
    write!(out, "<sigs{}", window.xml_attrs())?;
    if window.has_more {
        let size = if window.limit > 0 {
            format!(" --limit={}", window.limit)
        } else {
            String::new()
        };
        write!(
            out,
            " next=\"--for={task}{size} --offset={}\"",
            window.next_offset
        )?;
    }
    write!(out, ">")?;
    for (i, it) in val["symbols"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .enumerate()
    {
        write_for_sig_row(out, window.begin + i, it)?;
    }
    write!(out, "</sigs>")?;
    write_for_lego_section(out, val, raw_task)?;
    write_for_compose_section(out, val, raw_task)?;
    write_for_tree_and_docs(out, val)?;
    write_for_callers_section(out, val, page.nested())?;
    write_for_hops_section(out, val, page.nested())?;
    write_for_bodies(out, val, page.nested())
}

pub struct ForReport<'a> {
    pub val: &'a Value,
    pub inner: Vec<u8>,
}

/// The legend comment, root open tag and close tag around a `--for` body, which `est_tokens=` also prices.
const FOR_FRAME_BYTES: usize = 250;

impl crate::serialize::XmlReport for ForReport<'_> {
    crate::xml_report_meta!("ctx", "ripwire-rs.for/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let raw_task = self.val.str_or("task", "");
        let task = escape_xml_str(raw_task);
        let default_route = format!("name-exact({raw_task})");
        let route = escape_xml_str(self.val.str_or("route", &default_route));
        let raw_root = self.val.str_or("root", ".");
        let root = escape_xml_str(raw_root);
        let confidence = escape_xml_str(self.val.str_or("confidence", "high"));
        let margin_pct = self.val.u64_or("margin_pct", 0);
        let coverage = self.val.u64_or("coverage", 100);
        let bodies_cnt = self.val["symbols"].as_array().map_or(0, |items| {
            items.iter().filter(|item| item["body"].is_string()).count()
        });
        let est_tokens = crate::serialize::est_tokens(self.inner.len() + FOR_FRAME_BYTES);
        let bundle = escape_xml_str(self.val.str_or("bundle", "auto"));
        write!(
            out,
            " task=\"{task}\" route=\"{route}\" root=\"{root}\" confidence=\"{confidence}\" margin_pct=\"{margin_pct}\" coverage=\"{coverage}\" bundle=\"{bundle}\" bodies=\"{bodies_cnt}\" est_tokens=\"{est_tokens}\""
        )?;
        if let Some(reason) = self.val["reason"].as_str() {
            write!(out, " reason=\"{}\"", reason.xml_escaped())?;
        }
        if let Some(follow) = self.val["follow"].as_str() {
            write!(out, " follow=\"{}\"", follow.xml_escaped())?;
        }
        if let Some(at) = self.val["at"].as_str() {
            write!(out, " at=\"{}\"", at.xml_escaped())?;
        }
        Ok(())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.inner)
    }
}

pub fn serialize_for_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    let raw_task = val.str_or("task", "");
    let task = escape_xml_str(raw_task);
    let mut inner = Vec::with_capacity(4096);
    write_for_inner_body(&mut inner, val, &task, raw_task)?;
    ForReport { val, inner }.render_envelope(out)
}

fn for_section_requested(val: &Value, name: &str) -> bool {
    val["sections"]
        .as_array()
        .is_some_and(|s| s.iter().any(|v| v.as_str() == Some(name)))
}
fn write_for_tree_and_docs<W: Write>(out: &mut W, val: &Value) -> io::Result<()> {
    if let Some(tree) = val["tree"].as_array().filter(|a| !a.is_empty()) {
        for row in tree {
            let p = row.str_or("path", "").xml_escaped();
            let s = row.str_or("sym", "").xml_escaped();
            write!(out, "<t p=\"{p}\" sym=\"{s}\"/>")?;
        }
    }
    if let Some(docs) = val["docs"].as_array().filter(|a| !a.is_empty()) {
        for doc in docs {
            let p = doc.str_or("path", "").xml_escaped();
            let m = doc.u64_or("mentions", 1);
            write!(out, "<doc p=\"{p}\" mentions=\"{m}\"/>")?;
        }
    }
    Ok(())
}

fn write_for_lego_section<W: Write>(out: &mut W, val: &Value, raw_task: &str) -> io::Result<()> {
    let Some(ifaces) = val["lego"].as_array().filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    if !for_section_requested(val, "lego") {
        let replay = format!(
            "--for='{}' --sections=lego",
            raw_task.replace('\'', "'\\''")
        );
        return write!(
            out,
            "<!-- lego total=N shown=0 capped=1: N interfaces with implementors in the ranked files, collapsed; next= restores them. --><lego total=\"{}\" shown=\"0\" capped=\"1\" next=\"{}\"/>",
            ifaces.len(),
            replay.xml_escaped()
        );
    }
    write!(out, "<lego>")?;
    for iface in ifaces {
        let impls = iface["implementors"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        write!(
            out,
            "<iface n=\"{}\" p=\"{}:{}\" implementors=\"{}\">",
            iface["name"].as_str().unwrap_or("").xml_escaped(),
            iface["path"].as_str().unwrap_or("").xml_escaped(),
            iface["line"].as_u64().unwrap_or(0),
            impls.len()
        )?;
        for imp in impls {
            write!(
                out,
                "<impl n=\"{}\" p=\"{}:{}\"/>",
                imp["name"].as_str().unwrap_or("").xml_escaped(),
                imp["path"].as_str().unwrap_or("").xml_escaped(),
                imp["line"].as_u64().unwrap_or(0)
            )?;
        }
        write!(out, "</iface>")?;
    }
    write!(out, "</lego>")
}

fn write_for_compose_section<W: Write>(out: &mut W, val: &Value, raw_task: &str) -> io::Result<()> {
    let Some(fields) = val["compose"].as_array().filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    if !for_section_requested(val, "compose") {
        let replay = format!(
            "--for='{}' --sections=compose",
            raw_task.replace('\'', "'\\''")
        );
        return write!(
            out,
            "<compose total=\"{}\" shown=\"0\" capped=\"1\" next=\"{}\"/>",
            fields.len(),
            replay.xml_escaped()
        );
    }
    write!(out, "<compose>")?;
    for f in fields {
        write!(
            out,
            "<field name=\"{}\" owner=\"{}\" rel=\"{}\"/>",
            f.str_or("name", "").xml_escaped(),
            f.str_or("owner", "").xml_escaped(),
            f.str_or("rel", "uses").xml_escaped()
        )?;
    }
    write!(out, "</compose>")
}

fn write_for_hop_row(out: &mut dyn Write, h: &Value) -> io::Result<()> {
    let l = h.u64_or("line", 0);
    let p = h.str_or("path", "").xml_escaped();
    let name = h.str_or("name", "").xml_escaped();
    let total = h.u64_or("total", 0);
    write!(
        out,
        "<h l=\"{l}\" p=\"{p}\" n=\"{name}\"><calls total=\"{total}\">"
    )?;
    if let Some(calls) = h["calls"].as_array() {
        for c in calls {
            write!(
                out,
                "<c n=\"{}\" l=\"{}\"/>",
                c.str_or("name", "").xml_escaped(),
                c.u64_or("line", 0)
            )?;
        }
    }
    write!(out, "</calls></h>")
}

/// The opening tag of a `<tag extra window-attrs>` section.
struct SectionHead<'a> {
    tag: &'a str,
    extra: &'a str,
}

/// One windowed list of rows: `<tag extra shown= capped= ...>`, the rows in the window, `</tag>`.
fn write_paged_section<T, W: Write>(
    out: &mut W,
    head: &SectionHead<'_>,
    all: &[T],
    page: PageRequest,
    write_row: fn(&mut dyn Write, &T) -> io::Result<()>,
) -> io::Result<()> {
    let section = Paged::new(all, page, |row| row_bytes(|mut w| write_row(&mut w, row)));
    write!(out, "<{}{}{}>", head.tag, head.extra, section.attrs())?;
    section
        .rows()
        .iter()
        .try_for_each(|row| write_row(out, row))?;
    write!(out, "</{}>", head.tag)
}

fn write_for_caller_row(out: &mut dyn Write, c: &Value) -> io::Result<()> {
    let via = if c.bool_or("by_name", false) {
        " via=\"name\""
    } else {
        ""
    };
    write!(
        out,
        "<s t=\"{}\" n=\"{}\" p=\"{}:{}\"{via}/>",
        c.str_or("kind", "fn").xml_escaped(),
        c.str_or("name", "").xml_escaped(),
        c.str_or("path", "").xml_escaped(),
        c.u64_or("line", 0)
    )
}

/// `<callers of= total=>`: who calls the one definition a name-exact `--for` answers; `next=` lists them all.
fn write_for_callers_section<W: Write>(
    out: &mut W,
    val: &Value,
    page: PageRequest,
) -> io::Result<()> {
    let callers = &val["callers"];
    let Some(rows) = callers["rows"].as_array().filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    let of = callers.str_or("of", "");
    let extra = format!(
        " of=\"{}\" total=\"{}\" next=\"--callers={}\"",
        of.xml_escaped(),
        callers.u64_or("total", rows.len() as u64),
        of.xml_escaped()
    );
    let head = SectionHead {
        tag: "callers",
        extra: &extra,
    };
    write_paged_section(out, &head, rows, page, write_for_caller_row)
}

fn write_for_hops_section<W: Write>(out: &mut W, val: &Value, page: PageRequest) -> io::Result<()> {
    let Some(hops) = val["hops"].as_array().filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    let head = SectionHead {
        tag: "hops",
        extra: "",
    };
    write_paged_section(out, &head, hops, page, write_for_hop_row)
}

fn write_for_body_row(mut out: &mut dyn Write, item: &&Value) -> io::Result<()> {
    let Some(body) = item["body"].as_str() else {
        return Ok(());
    };
    write!(
        out,
        "<b t=\"{}\" l=\"{}\" p=\"{}\" n=\"{}\">",
        item.str_or("kind", "fn").xml_escaped(),
        item.u64_or("line", 0),
        item.str_or("path", "").xml_escaped(),
        item.str_or("name", "").xml_escaped()
    )?;
    crate::serialize::write_cdata(&mut out, body)?;
    write!(out, "</b>")
}

fn write_for_bodies<W: Write>(out: &mut W, val: &Value, page: PageRequest) -> io::Result<()> {
    let items: Vec<&Value> = val["symbols"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|item| item["body"].is_string())
        .collect();
    let head = SectionHead {
        tag: "bodies",
        extra: "",
    };
    write_paged_section(out, &head, &items, page, write_for_body_row)?;
    writeln!(out)
}

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

fn grep_enc_attr(h: &Value) -> String {
    h.opt_str("enclosing")
        .filter(|e| !e.is_empty())
        .map_or_else(String::new, |e| format!(" in=\"{}\"", e.xml_escaped()))
}

fn write_grep_hit<W: Write>(out: &mut W, run: &[&Value]) -> io::Result<()> {
    let h = run[0];
    let line = h.u64_or("line", 1);
    let enc = grep_enc_attr(h);
    let n = if run.len() > 1 {
        format!(" n=\"{}\"", run.len())
    } else {
        String::new()
    };
    write!(out, "<hit l=\"{line}\"{enc}{n}>")?;
    write_grep_context(out, "b", h["before"].as_array())?;
    crate::serialize::write_cdata(out, h.str_or("text", ""))?;
    write_grep_context(out, "a", h["after"].as_array())?;
    for repeat in &run[1..] {
        let repeat_line = repeat.u64_or("line", 1);
        write!(out, "<at l=\"{repeat_line}\"{}/>", grep_enc_attr(repeat))?;
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

/// Serialize `connect` subgraph to Ripwire XML.
fn write_connect_node_tag<W: Write>(out: &mut W, tag: &str, node: &Value) -> io::Result<()> {
    let n = escape_xml_str(node["name"].as_str().unwrap_or(""));
    let t = escape_xml_str(node["kind"].as_str().unwrap_or("fn"));
    let p = escape_xml_str(node["path"].as_str().unwrap_or(""));
    let l = node["line"].as_u64().unwrap_or(1);
    let extra = if let Some(c) = node["connects"].as_u64() {
        format!(" connects=\"{c}\"")
    } else if let Some(d) = node["defs"].as_u64().filter(|&d| d > 1) {
        format!(" defs=\"{d}\"")
    } else {
        String::new()
    };
    writeln!(out, "  <{tag} n=\"{n}\" t=\"{t}\" p=\"{p}:{l}\"{extra}/>")
}

pub struct ConnectReport<'a> {
    pub val: &'a Value,
}

impl crate::serialize::XmlReport for ConnectReport<'_> {
    crate::xml_report_meta!("connect", "ripwire-rs.connect/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let count = self.val["symbols"].as_array().map_or(0, Vec::len);
        let root = escape_xml_str(self.val["root"].as_str().unwrap_or("."));
        let t_cnt = self.val["terminals"].as_array().map_or(count, Vec::len);
        let s_cnt = self.val["steiner"].as_array().map_or(0, Vec::len);
        let e_cnt = self
            .val
            .array_any(&["edges", "connections"])
            .map_or(0, Vec::len);
        let nodes = t_cnt + s_cnt;
        let radius = self.val["radius"]
            .as_u64()
            .unwrap_or(u64::from(crate::query::graph::DEFAULT_CONNECT_RADIUS));
        write!(
            out,
            " root=\"{root}\" symbols=\"{count}\" terminals=\"{t_cnt}\" nodes=\"{nodes}\" edges=\"{e_cnt}\" radius=\"{radius}\""
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        if let Some(terms) = self.val["terminals"].as_array() {
            for t in terms {
                write_connect_node_tag(out, "t", t)?;
            }
        }
        if let Some(joins) = self.val["steiner"].as_array() {
            for s in joins {
                write_connect_node_tag(out, "s", s)?;
            }
        }
        if let Some(edge_list) = self.val.array_any(&["edges", "connections"]) {
            for e in edge_list {
                let f = e["from"].as_str().unwrap_or("").xml_escaped();
                let t = e["to"].as_str().unwrap_or("").xml_escaped();
                writeln!(out, "  <e f=\"{f}\" t=\"{t}\"/>")?;
            }
        }
        if let Some(conn_list) = self.val.array_any(&["connections", "edges"]) {
            for c in conn_list {
                let f = c["from"].as_str().unwrap_or("").xml_escaped();
                let t = c["to"].as_str().unwrap_or("").xml_escaped();
                writeln!(out, "  <edge from=\"{f}\" to=\"{t}\"/>")?;
            }
        }
        Ok(())
    }
}

pub fn serialize_connect_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ConnectReport { val }.render_envelope(out)
}

fn write_path_step<W: Write>(step: &Value, out: &mut W) -> io::Result<()> {
    if let Some(s) = step.as_str() {
        return writeln!(out, "  <s t=\"fn\" n=\"{}\"/>", s.xml_escaped());
    }
    let kind = step.str_or("kind", "fn").xml_escaped();
    let name = step.str_or("name", "").xml_escaped();
    let raw_path = step.str_or("path", "");
    let line = step.u64_or("line", 0);
    if !raw_path.is_empty() && line > 0 {
        let path = raw_path.xml_escaped();
        writeln!(out, "  <s t=\"{kind}\" n=\"{name}\" p=\"{path}:{line}\"/>")
    } else {
        writeln!(out, "  <s t=\"{kind}\" n=\"{name}\"/>")
    }
}

/// Serialize `path_between` call path to Ripwire XML.
pub fn serialize_path_between_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    let from = escape_xml_str(val["from"].as_str().unwrap_or(""));
    let to = escape_xml_str(val["to"].as_str().unwrap_or(""));
    let from_defs = val["from_defs"].as_u64().unwrap_or(1);
    let to_defs = val["to_defs"].as_u64().unwrap_or(1);
    let is_reach = val["reachable"].as_bool().unwrap_or(false);
    let reachable = if is_reach { "1" } else { "0" };
    let hops = val["hops"].as_u64().unwrap_or(0);
    write!(
        out,
        "<path schema=\"ripwire-rs.path/v1\" from=\"{from}\" to=\"{to}\" from_defs=\"{from_defs}\" to_defs=\"{to_defs}\" reachable=\"{reachable}\" hops=\"{hops}\""
    )?;
    if !is_reach {
        write!(
            out,
            " hint=\"no directed call path — try --connect={from},{to} (undirected: finds a shared caller), or --uses/--impact for non-call references\""
        )?;
    }
    writeln!(out, ">")?;
    if let Some(steps) = val
        .get("steps")
        .or_else(|| val.get("path"))
        .and_then(Value::as_array)
    {
        for step in steps {
            write_path_step(step, out)?;
        }
    }
    writeln!(out, "</path>")
}
/// Serialize `--exemplar` to Ripwire XML.
fn write_exemplar_calls_xml<W: Write>(out: &mut W, ex: &Value) -> io::Result<()> {
    let Some(calls) = ex["calls"].as_array().filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    write!(out, "<calls total=\"{}\">", calls.len())?;
    for c in calls {
        let cn = c.str_or("name", "").xml_escaped();
        let cl = c.u64_or("line", 1);
        let sig = c.str_or("sig", "").xml_escaped();
        write!(out, "<c n=\"{cn}\" l=\"{cl}\">{sig}</c>")?;
    }
    out.write_all(b"</calls>")
}

pub fn serialize_exemplar_xml<W: Write>(root: &str, val: &Value, out: &mut W) -> io::Result<()> {
    let ex = val.get("exemplar").unwrap_or(val);
    let k = val.str_any(&["kind", "role"], "fn");
    let n = ex.str_or("name", "").xml_escaped();
    let p = ex.str_or("path", "").xml_escaped();
    let l = ex.u64_or("line", 1);
    let c = ex.u64_any(&["callers", "in"], 0);
    let cx = ex.u64_or("ccx", 0);
    let cand_attr = match ex.get("candidates").and_then(Value::as_u64) {
        Some(cnt) => format!(" candidates=\"{cnt}\""),
        None => String::new(),
    };
    let tested = ex.u64_or("tested", 0);
    let b = ex.str_or("body", "");
    let root = root.xml_escaped();

    write!(
        out,
        "<!-- ripwire-rs exemplar schema=ripwire-rs.exemplar/v1: the best-in-class instance of kind= for the task. -->\n<exemplar schema=\"ripwire-rs.exemplar/v1\"{cand_attr} kind=\"{k}\" n=\"{n}\" p=\"{p}:{l}\" in=\"{c}\" ccx=\"{cx}\" root=\"{root}\" tested=\"{tested}\"><bodies shown=\"1\" total=\"1\" capped=\"0\"><b t=\"{k}\" l=\"{l}\" p=\"{p}\" n=\"{n}\">",
    )?;
    crate::serialize::write_cdata(out, b)?;
    write_exemplar_calls_xml(out, ex)?;
    writeln!(out, "</b></bodies></exemplar>")
}

/// Serialize `--lego` to Ripwire XML.
pub fn serialize_lego_xml<W: Write>(root: &str, val: &Value, out: &mut W) -> io::Result<()> {
    let iface = val.str_or("interface", "");
    let empty = Vec::new();
    let impls = val["implementors"].as_array().unwrap_or(&empty);
    let methods = val["methods"].as_array().unwrap_or(&empty);
    let defs = val.u64_or("defs", 1).max(1);
    let p_attr = val
        .opt_str("path")
        .filter(|p| !p.is_empty())
        .map_or_else(String::new, |p| format!(" p=\"{}\"", p.xml_escaped()));
    let m_attr = if methods.is_empty() {
        String::new()
    } else {
        format!(" methods=\"{}\"", methods.len())
    };
    let lego_attrs = val.xml_opt_gauge_attrs();
    let total = val["implementors_total"]
        .as_u64()
        .map_or(impls.len(), |total| total as usize);
    let window = if impls.len() < total {
        format!(" shown=\"{}\" total=\"{total}\" capped=\"1\"", impls.len())
    } else {
        String::new()
    };
    writeln!(
        out,
        "<ctx schema=\"ripwire-rs.lego/v1\" root=\"{root}\"><lego{lego_attrs}><iface n=\"{iface}\"{p_attr}{m_attr} defs=\"{defs}\" implementors=\"{total}\"{window}>"
    )?;
    for m in methods {
        let mn = m.str_or("name", "").xml_escaped();
        let ml = m.u64_or("line", 0);
        let mp = m.u64_or("params", 0);
        writeln!(out, "  <m n=\"{mn}\" l=\"{ml}\" params=\"{mp}\"/>")?;
    }
    for it in impls {
        let n = it.str_or("name", "");
        let p = it.str_or("path", "");
        let l = it.u64_or("line", 0);
        if l > 0 {
            writeln!(out, "  <impl n=\"{n}\" p=\"{p}:{l}\"/>")?;
        } else {
            writeln!(out, "  <impl n=\"{n}\" p=\"{p}\"/>")?;
        }
    }
    writeln!(out, "</iface></lego></ctx>")
}

fn push_recall_sections_note(buf: &mut String, s: &Value, whole_bytes: u64) {
    let selected = s.u64_or("selected", 0);
    let total = s.u64_or("total", 0);
    let lines = s.str_or("lines", "");
    let dropped = s.u64_or("dropped_by_budget", 0);
    if dropped == 0 {
        buf.push_str(&format!(
            "  [sections: {selected} of {total}, section-granular; whole doc {whole_bytes} B; lines=\"{lines}\"]"
        ));
    } else {
        let matched = s.u64_or("matched", 0);
        buf.push_str(&format!(
            "  [sections: {selected} of {matched} selected ({total} in doc), section-granular; whole doc {whole_bytes} B; lines=\"{lines}\"; dropped_by_budget={dropped}]"
        ));
    }
}

fn push_recall_doc(buf: &mut String, d: &Value) {
    buf.push_str(&format!(
        "━━ {}  (relevance {:.3}) ━━",
        d.str_or("path", ""),
        d.f64_any(&["score"], 0.0)
    ));
    if d.bool_or("generated", false) {
        buf.push_str("  [generated_demoted: marker]");
    }
    if let Some(s) = d.get("sections") {
        push_recall_sections_note(buf, s, d.u64_or("whole_bytes", 0));
    }
    if let Some(t) = d.get("truncated") {
        buf.push_str(&format!(
            "  [truncated: {} of {} bytes]",
            t.u64_or("shown_bytes", 0),
            t.u64_or("of_bytes", 0)
        ));
    }
    buf.push('\n');
    buf.push_str(d.str_or("content", ""));
    buf.push_str("\n\n");
}

fn push_recall_footer(buf: &mut String, val: &Value) {
    let relevant = val.u64_or("relevant", 0);
    if relevant == 0 {
        buf.push_str("(no relevant documents — try different terms)\n");
        return;
    }
    let by_k = val.u64_or("omitted_top_k", 0);
    let by_budget = val.u64_or("omitted_budget", 0);
    let mut parts = Vec::new();
    if by_k > 0 {
        parts.push(format!("raise --top-k/top_k for {by_k} more"));
    }
    if by_budget > 0 {
        let budget = val.u64_or("budget_bytes", 0);
        parts.push(format!(
            "raise --max-tokens/budget_tokens or narrow the query for {by_budget} more (~{budget}-byte budget)"
        ));
    }
    if !parts.is_empty() {
        buf.push_str(&format!(
            "(capped: {} of {relevant} relevant document files omitted — {})\n",
            by_k + by_budget,
            parts.join("; ")
        ));
    }
}

fn recall_header(query: &str, val: &Value, shown: usize, est_tokens: u64) -> String {
    let relevant = val.u64_or("relevant", 0);
    let mut h = format!(
        "ripwire-rs recall — \"{query}\" — {relevant} relevant of {} document files, best-first — total={relevant} shown={shown} capped={}",
        val.u64_or("documents", 0),
        u8::from(val.bool_or("capped", false))
    );
    for key in ["truncated", "generated_demoted"] {
        let n = val.u64_or(key, 0);
        if n > 0 {
            h.push_str(&format!(" {key}={n}"));
        }
    }
    let max_tokens = val.u64_or("max_tokens", 0);
    if est_tokens > max_tokens {
        h.push_str(" over_ceiling=1");
    }
    h.push_str(&format!(" max_tokens={max_tokens}"));
    if let Some(share) = val.get("share_bytes").and_then(Value::as_u64) {
        h.push_str(&format!(" share_bytes={share}"));
    }
    h.push_str(&format!(" est_tokens={est_tokens}\n\n"));
    h
}

/// Serialize `--recall` as plain text (upstream's rendering): header, ranked doc blocks, cap footer.
pub fn serialize_recall_text<W: Write>(query: &str, val: &Value, out: &mut W) -> io::Result<()> {
    let empty = Vec::new();
    let docs = val.array_any(&["docs"]).unwrap_or(&empty);
    let mut rest = String::new();
    for d in docs {
        push_recall_doc(&mut rest, d);
    }
    push_recall_footer(&mut rest, val);
    let mut est_tokens = 0u64;
    let mut header = recall_header(query, val, docs.len(), est_tokens);
    for _ in 0..4 {
        let total_bytes = (header.len() + rest.len()) as u64;
        let next = (total_bytes * 2).div_ceil(5);
        if next == est_tokens {
            break;
        }
        est_tokens = next;
        header = recall_header(query, val, docs.len(), est_tokens);
    }
    out.write_all(header.as_bytes())?;
    out.write_all(rest.as_bytes())
}

/// Serialize `--from-trace` to Ripwire XML.
pub fn serialize_from_trace_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    let empty = Vec::new();
    let suspects = val["suspects"].as_array().unwrap_or(&empty);
    writeln!(
        out,
        "<from-trace schema=\"ripwire-rs.from-trace/v1\" count=\"{}\">",
        suspects.len()
    )?;
    for s in suspects {
        let n = s.str_or("name", "");
        let p = s.str_any(&["path", "file"], "");
        let l = s.u64_or("line", 0);
        writeln!(out, "  <suspect n=\"{n}\" p=\"{p}\" l=\"{l}\"/>")?;
    }
    writeln!(out, "</from-trace>")
}
