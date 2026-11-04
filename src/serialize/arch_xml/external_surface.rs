use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

pub struct ExternalSurfaceReport<'a> {
    pub externals: &'a [(&'a str, &'a str, usize)],
    pub page: crate::serialize::PageRequest,
}

fn external_cost(&(name, lang, _): &(&str, &str, usize)) -> usize {
    crate::serialize::text_bytes(&[name, lang])
}

impl ExternalSurfaceReport<'_> {
    fn externals_page(&self) -> Paged<'_, (&str, &str, usize)> {
        Paged::new(self.externals, self.page, external_cost)
    }
}

impl crate::serialize::XmlReport for ExternalSurfaceReport<'_> {
    crate::xml_report_meta!(
        "external-surface",
        "ripwire-rs.external-surface/v1",
        root: false,
        comment: "ripwire-rs external-surface schema=ripwire-rs.external-surface/v1: names used but never defined in the index: <x n= lang= refs= calls=>."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let total = self.externals.len();
        let pw = self.externals_page().window;
        write!(out, " names=\"{total}\" builtins_excluded=\"14\"")?;
        pw.write_xml_attrs(out)?;
        if pw.capped {
            write!(
                out,
                " next=\"--external-surface --offset={}\"",
                pw.next_offset
            )?;
        }
        Ok(())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for (name, lang, refs) in self.externals_page().rows() {
            writeln!(
                out,
                "  <x n=\"{}\" lang=\"{lang}\" refs=\"{refs}\" calls=\"{refs}\"/>",
                name.xml_escaped()
            )?;
        }
        Ok(())
    }
}

/// Serialize `--external-surface` to Ripwire XML.
pub fn serialize_external_surface_xml<W: Write>(
    _root: &str,
    externals: &[(&str, &str, usize)],
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ExternalSurfaceReport { externals, page }.render_envelope(out)
}
