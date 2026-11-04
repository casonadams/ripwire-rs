use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

use super::pair_rows::write_pair_rows;

pub struct SafeDeleteReport<'a> {
    pub root: &'a str,
    pub sym_name: &'a str,
    pub kind_tag: &'a str,
    pub def_loc: &'a str,
    pub defs: usize,
    pub callers: &'a [(&'a str, &'a str)],
    pub ambiguous_callers: usize,
    pub uses: usize,
    pub tested_self: usize,
    pub impact_reaches: usize,
    pub radius_tested: usize,
    pub radius_untested: usize,
    pub gauge_attrs: &'a str,
    pub page: crate::serialize::PageRequest,
}

fn caller_cost(&(p, n): &(&str, &str)) -> usize {
    crate::serialize::text_bytes(&[p, n])
}

impl SafeDeleteReport<'_> {
    fn callers_page(&self) -> Paged<'_, (&str, &str)> {
        Paged::new(self.callers, self.page, caller_cost)
    }
}

impl crate::serialize::XmlReport for SafeDeleteReport<'_> {
    crate::xml_report_meta!(
        "safe-delete",
        "ripwire-rs.safe-delete/v1",
        root: false,
        comment: "ripwire-rs safe-delete schema=ripwire-rs.safe-delete/v1: can sym= go: callers= impact_reaches= uses= tested_self= risk=."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let risk = if self.callers.is_empty() && self.impact_reaches == 0 {
            "low"
        } else {
            "uses-exist"
        };
        let c_len = self.callers.len();
        let paged = self.callers_page();
        let shown = paged.window.shown;
        let capped = usize::from(paged.window.capped);
        let dead_cand = usize::from(c_len == 0 && self.impact_reaches == 0);
        write!(
            out,
            " sym=\"{}\" t=\"{}\" p=\"{}\" defs=\"{}\" callers=\"{c_len}\" ambiguous_callers=\"{}\" impact_reaches=\"{}\" uses=\"{}\" tested_self=\"{}\" radius_tested=\"{}\" radius_untested=\"{}\" dead_code_candidate=\"{dead_cand}\" risk=\"{risk}\" shown=\"{shown}\" capped=\"{capped}\" {} root=\"{}\"",
            self.sym_name.xml_escaped(),
            self.kind_tag,
            self.def_loc.xml_escaped(),
            self.defs,
            self.ambiguous_callers,
            self.impact_reaches,
            self.uses,
            self.tested_self,
            self.radius_tested,
            self.radius_untested,
            self.gauge_attrs,
            self.root
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let rows = self.callers_page().rows().iter().map(|&(p, n)| (n, p));
        write_pair_rows(out, "c", ["n", "p"], rows)
    }
}

/// Serialize `--safe-delete` to Ripwire XML.
pub fn serialize_safe_delete_xml<W: Write>(
    report: &SafeDeleteReport<'_>,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    report.render_envelope(out)
}
