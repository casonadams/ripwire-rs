use super::escape::XmlEscapeExt;
use super::gauges::GraphGaugeProvider;
use super::paging::PageWindow;
use crate::graph::GraphGauges;

/// Trait for types that can be serialized as XML and converted to a string.
pub trait ToXmlString {
    fn write_xml<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()>;

    fn to_xml_string(&self) -> String {
        let mut buf = Vec::new();
        let _ = self.write_xml(&mut buf);
        String::from_utf8(buf).unwrap_or_default()
    }
}

impl<T: XmlReport + ?Sized> ToXmlString for T {
    fn write_xml<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        self.render_envelope(out)
    }
}

#[macro_export]
macro_rules! xml_report_meta {
    ($tag:expr, $ver:expr, root: $inc:expr $(, comment: $comment:expr)?) => {
        fn schema_tag(&self) -> &'static str {
            $tag
        }
        fn schema_version(&self) -> &'static str {
            $ver
        }
        fn include_root_attr(&self) -> bool {
            $inc
        }
        $(
            fn legend_comment(&self) -> Option<&'static str> {
                Some($comment)
            }
        )?
    };
}

/// Unified XML envelope trait for structured Ripwire reports.
pub trait XmlReport {
    fn schema_tag(&self) -> &'static str;
    fn schema_version(&self) -> &'static str;
    fn root_path(&self) -> &str {
        "."
    }
    fn include_root_attr(&self) -> bool {
        true
    }
    fn legend_comment(&self) -> Option<&'static str> {
        None
    }
    fn page_window(&self) -> Option<PageWindow> {
        None
    }
    fn pr_iters(&self) -> Option<u32> {
        None
    }
    fn graph_gauges(&self) -> Option<GraphGauges> {
        None
    }
    fn write_extra_attrs<W: std::io::Write>(&self, _out: &mut W) -> std::io::Result<()> {
        Ok(())
    }
    fn write_body<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()>;

    fn write_xml_report<W: std::io::Write>(&self, root: &str, out: &mut W) -> std::io::Result<()> {
        let tag = self.schema_tag();
        let ver = self.schema_version();
        if let Some(comment) = self.legend_comment() {
            writeln!(out, "<!-- {comment} -->")?;
        }
        if self.include_root_attr() {
            write!(
                out,
                "<{tag} schema=\"{ver}\" root=\"{}\"",
                root.xml_escaped()
            )?;
        } else {
            write!(out, "<{tag} schema=\"{ver}\"")?;
        }
        if let Some(pw) = self.page_window() {
            pw.write_xml_attrs(out)?;
        }
        if let Some(iters) = self.pr_iters() {
            write!(out, " pr_iters=\"{}\"", iters.max(1))?;
        }
        self.write_extra_attrs(out)?;
        if let Some(g) = self.graph_gauges() {
            g.write_xml_gauge_attrs(out)?;
        }
        writeln!(out, ">")?;
        self.write_body(out)?;
        writeln!(out, "</{tag}>")
    }

    fn render_envelope<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        self.write_xml_report(self.root_path(), out)
    }
}
