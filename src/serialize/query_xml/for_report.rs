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
