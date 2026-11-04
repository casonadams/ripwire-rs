use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

/// One untested impacted symbol row (`<u sym= p= l= ccx=/>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntestedSymbol<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub line: u32,
    pub ccx: u32,
}

pub struct TestGateReport<'a> {
    pub root: &'a str,
    pub at_stamp: &'a str,
    pub changed: usize,
    pub impacted: usize,
    pub tests: &'a [crate::gate::affected::AffectedTest],
    pub untested: &'a [UntestedSymbol<'a>],
    pub untested_modscope: usize,
    pub run_first: usize,
    pub script_gates_unmodelled: usize,
    pub script_gates_registered: usize,
    pub script_gates_mapped: usize,
    pub page: crate::serialize::PageRequest,
    pub gauge_attrs: &'a str,
}

fn untested_cost(u: &UntestedSymbol<'_>) -> usize {
    crate::serialize::row_bytes(|mut w| write_untested_row(u, &mut w))
}

fn test_gate_cost(t: &crate::gate::affected::AffectedTest) -> usize {
    crate::serialize::row_bytes(|mut w| write_test_gate_row(t, &mut w))
}

impl TestGateReport<'_> {
    fn untested_page(&self) -> Paged<'_, UntestedSymbol<'_>> {
        Paged::new(self.untested, self.page, untested_cost)
    }

    fn tests_page(&self) -> Paged<'_, crate::gate::affected::AffectedTest> {
        Paged::new(self.tests, self.page.nested(), test_gate_cost)
    }
}

fn write_untested_row<W: Write>(u: &UntestedSymbol<'_>, out: &mut W) -> io::Result<()> {
    write!(
        out,
        "<u sym=\"{}\" p=\"{}\" l=\"{}\" ccx=\"{}\"/>",
        u.name.xml_escaped(),
        u.path.xml_escaped(),
        u.line,
        u.ccx
    )
}

fn test_row_flags(t: &crate::gate::affected::AffectedTest) -> String {
    let changed = if t.is_direct_change {
        " changed=\"1\""
    } else {
        ""
    };
    let partner = if t.is_partner { " partner=\"1\"" } else { "" };
    let evidence = if t.script_literal {
        " evidence=\"script_literal\""
    } else {
        ""
    };
    let hops = if (!t.is_partner && !t.is_direct_change && !t.script_literal) || t.hops > 0 {
        format!(" hops=\"{}\"", t.hops)
    } else {
        String::new()
    };
    format!("{changed}{partner}{hops}{evidence}")
}

fn write_test_gate_row<W: Write>(
    t: &crate::gate::affected::AffectedTest,
    out: &mut W,
) -> io::Result<()> {
    write!(
        out,
        "<t p=\"{}\"{}{}/>",
        t.path.xml_escaped(),
        test_row_flags(t),
        t.run_attr()
    )
}

/// Upstream folds runner-less rows that agree on every flag into one `<g n= p=>` row, listing
/// their paths in order at the first member's position. A path holding ',' never folds.
pub fn write_test_rows<W: Write>(
    rows: &[crate::gate::affected::AffectedTest],
    flags_of: impl Fn(&crate::gate::affected::AffectedTest) -> String,
    write_one: impl Fn(&crate::gate::affected::AffectedTest, &mut W) -> io::Result<()>,
    out: &mut W,
) -> io::Result<()> {
    let foldable =
        |t: &crate::gate::affected::AffectedTest| t.run.is_none() && !t.path.contains(',');
    let mut taken = vec![false; rows.len()];
    for (i, t) in rows.iter().enumerate() {
        if taken[i] {
            continue;
        }
        taken[i] = true;
        if !foldable(t) {
            write_one(t, out)?;
            continue;
        }
        let flags = flags_of(t);
        let mut paths = vec![t.path.as_str()];
        for (j, u) in rows.iter().enumerate().skip(i + 1) {
            if !taken[j] && foldable(u) && flags_of(u) == flags {
                taken[j] = true;
                paths.push(u.path.as_str());
            }
        }
        if paths.len() == 1 {
            write_one(t, out)?;
        } else {
            write!(
                out,
                "<g{flags} n=\"{}\" p=\"{}\"{}/>",
                paths.len(),
                paths.join(",").xml_escaped(),
                t.run_attr()
            )?;
        }
    }
    Ok(())
}

impl crate::serialize::XmlReport for TestGateReport<'_> {
    crate::xml_report_meta!(
        "test-gate",
        "ripwire-rs.test-gate/v1",
        root: false,
        comment: "ripwire-rs test-gate schema=ripwire-rs.test-gate/v1: tests + untested blast radius."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let t_len = self.tests.len();
        let u_len = self.untested.len();
        let tests = self.tests_page();
        let win = self.untested_page().window;
        let next = self
            .tests
            .iter()
            .find_map(|t| t.run.as_deref())
            .unwrap_or("--situ");
        write!(
            out,
            " changed=\"{}\" impacted=\"{}\" tests=\"{t_len}\" untested=\"{u_len}\" untested_modscope=\"{}\" shown_tests=\"{}\" tests_capped=\"{}\" shown_untested=\"{}\" untested_capped=\"{}\" script_gates_unmodelled=\"{}\" script_gates_registered=\"{}\" script_gates_mapped=\"{}\" script_gates_unresolved_dynamic=\"{}\" ccx_bar=\"15\"",
            self.changed,
            self.impacted,
            self.untested_modscope,
            tests.window.shown,
            usize::from(tests.window.capped),
            win.shown,
            usize::from(win.capped),
            self.script_gates_unmodelled,
            self.script_gates_registered,
            self.script_gates_mapped,
            self.script_gates_registered - self.script_gates_mapped,
        )?;
        if self.run_first > 0 {
            write!(out, " run_first=\"{}\"", self.run_first)?;
        }
        write!(out, " {}", self.gauge_attrs)?;
        if win.capped || win.offset > 0 || win.limit > 0 {
            write!(
                out,
                " total=\"{u_len}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
                usize::from(win.has_more),
                win.next_offset,
                win.offset,
                win.limit
            )?;
        }
        if !self.at_stamp.is_empty() {
            write!(out, " at=\"{}\"", self.at_stamp.xml_escaped())?;
        }
        write!(
            out,
            " root=\"{}\" next=\"{}\"",
            self.root.xml_escaped(),
            next.xml_escaped()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write_test_rows(
            self.tests_page().rows(),
            test_row_flags,
            write_test_gate_row,
            out,
        )?;
        for u in self.untested_page().rows() {
            write_untested_row(u, out)?;
        }
        Ok(())
    }
}

/// Serialize `--test-gate` to Ripwire XML.
pub fn serialize_test_gate_xml<W: Write>(
    root: &str,
    tests: &[crate::gate::affected::AffectedTest],
    untested: usize,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    let at_stamp = crate::git::history::compute_git_at_stamp(root).unwrap_or_default();
    TestGateReport {
        root,
        at_stamp: &at_stamp,
        changed: usize::from(!tests.is_empty() || untested > 0),
        impacted: untested,
        tests,
        untested: &[],
        untested_modscope: 0,
        run_first: usize::from(!tests.is_empty()),
        script_gates_unmodelled: 0,
        script_gates_registered: 0,
        script_gates_mapped: 0,
        page: crate::serialize::PageRequest::default(),
        gauge_attrs: crate::serialize::GraphGauges::ZERO_XML_ATTRS,
    }
    .render_envelope(out)
}
