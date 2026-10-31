use crate::serialize::JsonValueExt;
use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use serde_json::Value;
use std::io::{self, Write};

struct DefsReport<'a> {
    val: &'a Value,
}

fn write_def_row<W: Write>(out: &mut W, d: &Value) -> io::Result<()> {
    let n = d.str_or("name", "").xml_escaped();
    let t = d.str_or("kind", "fn").xml_escaped();
    let p = d.str_or("path", "").xml_escaped();
    let l = d.u64_or("line", 1);
    write!(out, "  <d n=\"{n}\" t=\"{t}\" p=\"{p}:{l}\"")?;
    if let Some(sc) = d.opt_str("scope") {
        write!(out, " sc=\"{}\"", sc.xml_escaped())?;
    }
    write!(out, " lang=\"{}\"", d.str_or("lang", "").xml_escaped())?;
    if d.bool_or("test", false) {
        write!(out, " test=\"1\"")?;
    }
    let callers = d.u64_or("callers", 0);
    if callers > 0 {
        write!(out, " in=\"{callers}\"")?;
    }
    if let Some(sel) = d.opt_str("sel") {
        write!(out, " sel=\"{}\"", sel.xml_escaped())?;
    }
    writeln!(out, "/>")
}

impl crate::serialize::XmlReport for DefsReport<'_> {
    crate::xml_report_meta!("defs", "ripwire-rs.defs/v1", root: false);

    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let root = escape_xml_str(self.val.str_or("root", "."));
        write!(
            out,
            " root=\"{root}\" names=\"{}\"",
            self.val.u64_or("names", 0)
        )?;
        let total = self.val.u64_or("total", 0);
        let shown = self.val.u64_or("shown", 0);
        if shown < total {
            write!(out, " shown=\"{shown}\" total=\"{total}\" capped=\"1\"")?;
        }
        Ok(())
    }

    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for d in self.val["defs"].as_array().into_iter().flatten() {
            write_def_row(out, d)?;
        }
        for name in self.val["missing"].as_array().into_iter().flatten() {
            let n = name.as_str().unwrap_or("").xml_escaped();
            writeln!(out, "  <nf n=\"{n}\"/>")?;
        }
        Ok(())
    }
}

/// Serialize `defs` rows to Ripwire XML.
pub fn serialize_defs_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    DefsReport { val }.render_envelope(out)
}
