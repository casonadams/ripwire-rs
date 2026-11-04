use crate::serialize::JsonValueExt;
use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use serde_json::Value;
use std::io::{self, Write};

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
