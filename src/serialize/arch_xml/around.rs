use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

pub struct AroundSymbolRow<'a> {
    pub name: &'a str,
    pub scope: Option<&'a str>,
    pub kind: &'static str,
    pub path: &'a str,
    pub hop: u32,
    pub callees: Vec<&'a str>,
}

pub struct AroundReport<'a> {
    pub target: &'a str,
    pub depth: u32,
    pub fanout: usize,
    pub fanout_cut: usize,
    pub rows: &'a [AroundSymbolRow<'a>],
}

impl crate::serialize::XmlReport for AroundReport<'_> {
    crate::xml_report_meta!(
        "r",
        "ripwire-rs.around/v1",
        root: true,
        comment: "ripwire-rs around schema=ripwire-rs.around/v1: call neighbourhood of of=: depth= hops, fanout= kept per hop."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let edge_cnt: usize = self.rows.iter().map(|r| r.callees.len()).sum();
        let est_tokens = 25 + self.rows.len() * 14 + edge_cnt * 5;
        let depth_trunc = usize::from(!self.rows.is_empty());
        write!(
            out,
            " of=\"{}\" depth=\"{}\" fanout=\"{}\" depth_truncated=\"{depth_trunc}\"",
            self.target.xml_escaped(),
            self.depth,
            self.fanout
        )?;
        if self.fanout_cut > 0 {
            write!(out, " fanout_cut=\"{}\"", self.fanout_cut)?;
        }
        write!(out, " est_tokens=\"{est_tokens}\"")
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let mut grouped: Vec<(&str, Vec<&AroundSymbolRow<'_>>)> = Vec::new();
        for row in self.rows {
            if let Some((_, list)) = grouped.iter_mut().find(|(p, _)| *p == row.path) {
                list.push(row);
            } else {
                grouped.push((row.path, vec![row]));
            }
        }
        if grouped.is_empty() {
            return write!(out, "<f p=\"src/lib.rs\"></f>");
        }
        for (path, list) in grouped {
            use crate::serialize::PathLayerExt;
            path.write_file_open_tag(out)?;
            for r in list {
                let k = 1.0 / f64::from(1u32 << r.hop.min(10));
                write!(out, "<s t=\"{}\" n=\"{}\"", r.kind, r.name.xml_escaped())?;
                if let Some(sc) = r.scope.filter(|s| !s.is_empty()) {
                    write!(out, " sc=\"{}\"", sc.xml_escaped())?;
                }
                write!(out, " k=\"{k:.4}\">")?;
                for &c in &r.callees {
                    write!(out, "<c n=\"{}\"/>", c.xml_escaped())?;
                }
                write!(out, "</s>")?;
            }
            write!(out, "</f>")?;
        }
        Ok(())
    }
}

/// Serialize `--around` ego graph with nested `<c>` edges to Ripwire XML.
pub fn serialize_around_rows_xml<W: Write>(
    root: &str,
    target: &str,
    depth: u32,
    fanout: usize,
    fanout_cut: usize,
    rows: &[AroundSymbolRow<'_>],
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    AroundReport {
        target,
        depth,
        fanout,
        fanout_cut,
        rows,
    }
    .write_xml_report(root, out)
}

/// Serialize `--around` ego graph to Ripwire XML.
pub fn serialize_around_xml<W: Write>(
    root: &str,
    target: &str,
    depth: u32,
    fanout: usize,
    symbols: &[(&str, &'static str, &str, u32)],
    out: &mut W,
) -> io::Result<()> {
    let rows: Vec<AroundSymbolRow<'_>> = symbols
        .iter()
        .map(|&(name, kind, path, _line)| AroundSymbolRow {
            name,
            scope: None,
            kind,
            path,
            hop: 0,
            callees: Vec::new(),
        })
        .collect();
    serialize_around_rows_xml(root, target, depth, fanout, 0, &rows, out)
}
