use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

pub type DeadRow<'a> = (&'a str, &'static str, &'a str, u32);

pub struct DeadCodeReport<'a> {
    pub items: &'a [DeadRow<'a>],
    /// Functions whose every indexed caller is test code: listed as `<to>` rows, not counted in `count=`.
    pub test_only: &'a [DeadRow<'a>],
    pub gauges: crate::graph::GraphGauges,
    pub page: crate::serialize::PageRequest,
}

fn dead_row_cost(tag: &str, row: &DeadRow<'_>) -> usize {
    crate::serialize::row_bytes(|mut w| write_dead_row(tag, row, &mut w))
}

fn dead_page<'a>(
    rows: &'a [DeadRow<'a>],
    page: crate::serialize::PageRequest,
    tag: &'static str,
) -> Paged<'a, DeadRow<'a>> {
    Paged::new(rows, page, move |row| dead_row_cost(tag, row))
}

impl DeadCodeReport<'_> {
    fn paged(&self) -> Paged<'_, DeadRow<'_>> {
        dead_page(self.items, self.page, "d")
    }

    fn test_only_paged(&self) -> Paged<'_, DeadRow<'_>> {
        dead_page(self.test_only, self.page.nested(), "to")
    }
}

fn write_dead_row<W: Write>(tag: &str, &(n, t, p, l): &DeadRow<'_>, out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "  <{tag} n=\"{}\" t=\"{t}\" p=\"{}\" l=\"{l}\"/>",
        n.xml_escaped(),
        p.xml_escaped()
    )
}

fn write_dead_rows<W: Write>(tag: &str, rows: &[DeadRow<'_>], out: &mut W) -> io::Result<()> {
    rows.iter()
        .try_for_each(|row| write_dead_row(tag, row, out))
}

fn write_test_only_attrs<W: Write>(
    out: &mut W,
    to: crate::serialize::PageWindow,
) -> io::Result<()> {
    if to.total > 0 {
        write!(out, " to=\"{}\"", to.total)?;
        if to.capped {
            write!(out, " to_shown=\"{}\"", to.shown)?;
        }
    }
    Ok(())
}

impl crate::serialize::XmlReport for DeadCodeReport<'_> {
    crate::xml_report_meta!(
        "dead-code",
        "ripwire-rs.dead-code/v1",
        root: true,
        comment: "ripwire-rs dead-code schema=ripwire-rs.dead-code/v1: internal-linkage functions with no caller found in the index: <d n= t= p= l=>; <to> rows (to= count, outside count=) are functions whose every indexed caller is test code."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        Some(self.paged().window)
    }
    fn graph_gauges(&self) -> Option<crate::graph::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " count=\"{}\" evidence=\"internal-linkage+zero-callers\"",
            self.items.len()
        )?;
        write_test_only_attrs(out, self.test_only_paged().window)
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write_dead_rows("d", self.paged().rows(), out)?;
        write_dead_rows("to", self.test_only_paged().rows(), out)
    }
}

/// Serialize `--dead-code` candidates to Ripwire XML.
pub fn serialize_dead_code_xml<W: Write>(
    root: &str,
    items: &[(&str, &'static str, &str, u32)],
    out: &mut W,
) -> io::Result<()> {
    serialize_dead_code_paged_xml(
        root,
        items,
        crate::graph::GraphGauges::default(),
        crate::serialize::PageRequest::default(),
        out,
    )
}

pub fn serialize_dead_code_paged_xml<W: Write>(
    root: &str,
    items: &[DeadRow<'_>],
    gauges: crate::graph::GraphGauges,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    DeadCodeReport {
        items,
        test_only: &[],
        gauges,
        page,
    }
    .write_xml_report(root, out)
}
