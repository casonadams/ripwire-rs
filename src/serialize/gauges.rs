use crate::graph::GraphGauges;

impl GraphGauges {
    pub const ZERO_XML_ATTRS: &'static str =
        "graph_ambiguous=\"0\" graph_unresolved=\"0\" graph_unindexed=\"0\" counts_floor=\"1\"";
}

/// Trait unifying resolver gauge extraction and XML attribute formatting.
pub trait GraphGaugeProvider {
    fn gauges(&self) -> GraphGauges;

    #[must_use]
    fn xml_gauge_attrs(&self) -> String {
        let mut buf = Vec::new();
        let _ = self.write_xml_gauge_attrs(&mut buf);
        String::from_utf8(buf).unwrap_or_default()
    }

    fn write_xml_gauge_attrs<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        let g = self.gauges();
        write!(
            out,
            " graph_ambiguous=\"{}\" graph_unresolved=\"{}\" graph_unindexed=\"{}\" counts_floor=\"1\"",
            g.ambiguous, g.unresolved, g.unindexed
        )
    }

    fn write_summary_counts<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        let g = self.gauges();
        write!(
            out,
            " ambiguous={} unresolved={} external={} declined={}",
            g.ambiguous, g.unresolved, g.external, g.declined
        )
    }
}

impl GraphGaugeProvider for GraphGauges {
    #[inline]
    fn gauges(&self) -> GraphGauges {
        *self
    }
}

impl GraphGaugeProvider for crate::graph::Graph {
    #[inline]
    fn gauges(&self) -> GraphGauges {
        crate::graph::Graph::gauges(self)
    }
}

impl crate::graph::Graph {
    #[must_use]
    pub fn xml_gauge_attrs(&self) -> String {
        GraphGaugeProvider::xml_gauge_attrs(self)
    }
}
