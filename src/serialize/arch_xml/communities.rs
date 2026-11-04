use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityModule<'a> {
    pub id: u32,
    pub size: usize,
    pub dir: &'a str,
    pub label: &'a str,
    pub members: Vec<(&'static str, &'a str, &'a str, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunitySummary<'a> {
    pub isolated: usize,
    pub isolated_decl: usize,
    pub isolated_header: usize,
    pub isolated_source: usize,
    pub isolated_doc: usize,
    pub connected_singletons: usize,
    pub symbols: usize,
    pub pr_iters: u32,
    pub gauges: crate::serialize::GraphGauges,
    pub bridges: Vec<(u32, u32, &'a str, &'a str, usize)>,
}

pub struct CommunitiesReport<'a> {
    pub modules: &'a [CommunityModule<'a>],
    pub summary: &'a CommunitySummary<'a>,
    pub page: crate::serialize::PageRequest,
}

fn community_cost(nested: crate::serialize::PageRequest) -> impl Fn(&CommunityModule<'_>) -> usize {
    move |m| crate::serialize::row_bytes(|mut w| write_community(&mut w, m, nested))
}

impl CommunitiesReport<'_> {
    fn modules_page(&self) -> Paged<'_, CommunityModule<'_>> {
        Paged::new(self.modules, self.page, community_cost(self.page.nested()))
    }

    fn bridges_page(&self) -> Paged<'_, (u32, u32, &str, &str, usize)> {
        Paged::new(
            &self.summary.bridges[..],
            self.page.nested(),
            |&(a, b, la, lb, edges)| {
                crate::serialize::row_bytes(|mut w| write_bridge(&mut w, a, b, la, lb, edges))
            },
        )
    }
}

fn write_community<W: Write>(
    out: &mut W,
    m: &CommunityModule<'_>,
    members_page: crate::serialize::PageRequest,
) -> io::Result<()> {
    let members = Paged::new(
        &m.members[..],
        members_page,
        |&(kind, name, path, _line)| crate::serialize::text_bytes(&[kind, name, path]),
    );
    let top_n = members.window.shown;
    let m_capped = usize::from(top_n < m.size);
    writeln!(
        out,
        "  <community id=\"{}\" size=\"{}\" dir=\"{}\" label=\"{}\" shown=\"{top_n}\" capped=\"{m_capped}\">",
        m.id,
        m.size,
        m.dir.xml_escaped(),
        m.label.xml_escaped()
    )?;
    for &(kind, name, path, line) in members.rows() {
        writeln!(
            out,
            "    <member t=\"{kind}\" n=\"{}\" p=\"{}:{line}\"/>",
            name.xml_escaped(),
            path.xml_escaped()
        )?;
    }
    writeln!(out, "  </community>")
}

fn write_bridge<W: Write>(
    out: &mut W,
    a: u32,
    b: u32,
    la: &str,
    lb: &str,
    edges: usize,
) -> io::Result<()> {
    writeln!(
        out,
        "  <bridge a=\"{a}\" b=\"{b}\" from_label=\"{}\" to_label=\"{}\" edges=\"{edges}\"/>",
        la.xml_escaped(),
        lb.xml_escaped()
    )
}

impl crate::serialize::XmlReport for CommunitiesReport<'_> {
    crate::xml_report_meta!(
        "communities",
        "ripwire-rs.communities/v1",
        root: true,
        comment: "ripwire-rs communities schema=ripwire-rs.communities/v1: call-graph modules (Leiden): <community id= size= dir= label=> of <member t= n= p=>."
    );
    fn pr_iters(&self) -> Option<u32> {
        Some(self.summary.pr_iters.max(1))
    }
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
        Some(self.summary.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let pw = self.modules_page().window;
        let total_bridges = self.summary.bridges.len();
        let bridges = self.bridges_page();
        let shown_bridges = bridges.window.shown;
        let bridges_capped = usize::from(bridges.window.capped);
        write!(
            out,
            " drill=\"--community=ID\" modules=\"{}\" shown_modules=\"{}\" modules_capped=\"{}\" bridges=\"{total_bridges}\" shown_bridges=\"{shown_bridges}\" bridges_capped=\"{bridges_capped}\" isolated=\"{}\" isolated_decl=\"{}\" isolated_header=\"{}\" isolated_source=\"{}\" isolated_doc=\"{}\" connected_singletons=\"{}\" symbols=\"{}\"",
            pw.total,
            pw.shown,
            usize::from(pw.capped),
            self.summary.isolated,
            self.summary.isolated_decl,
            self.summary.isolated_header,
            self.summary.isolated_source,
            self.summary.isolated_doc,
            self.summary.connected_singletons,
            self.summary.symbols,
        )?;
        if pw.capped {
            write!(
                out,
                " total=\"{}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
                pw.total,
                usize::from(pw.has_more),
                pw.next_offset,
                pw.offset,
                pw.limit
            )?;
        }
        Ok(())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for m in self.modules_page().rows() {
            write_community(out, m, self.page.nested())?;
        }
        for &(a, b, la, lb, edges) in self.bridges_page().rows() {
            write_bridge(out, a, b, la, lb, edges)?;
        }
        Ok(())
    }
}

/// Serialize `--communities` to Ripwire XML.
pub fn serialize_communities_xml<W: Write>(
    root: &str,
    modules: &[CommunityModule<'_>],
    out: &mut W,
) -> io::Result<()> {
    let default_sum = CommunitySummary {
        symbols: modules.iter().map(|m| m.size).sum(),
        pr_iters: 1,
        ..Default::default()
    };
    serialize_communities_with_summary_xml(
        root,
        modules,
        &default_sum,
        crate::serialize::PageRequest::default(),
        out,
    )
}

pub fn serialize_communities_with_summary_xml<W: Write>(
    root: &str,
    modules: &[CommunityModule<'_>],
    summary: &CommunitySummary<'_>,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    CommunitiesReport {
        modules,
        summary,
        page,
    }
    .write_xml_report(root, out)
}
