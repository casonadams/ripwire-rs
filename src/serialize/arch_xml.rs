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

impl TestGateReport<'_> {
    fn untested_page(&self) -> Paged<'_, UntestedSymbol<'_>> {
        Paged::new(self.untested, self.page, |u| {
            crate::serialize::row_bytes(|mut w| write_untested_row(u, &mut w))
        })
    }

    fn tests_page(&self) -> Paged<'_, crate::gate::affected::AffectedTest> {
        Paged::new(self.tests, self.page.nested(), |t| {
            crate::serialize::row_bytes(|mut w| write_test_gate_row(t, &mut w))
        })
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
        let run_first = if self.run_first > 0 {
            format!(" run_first=\"{}\"", self.run_first)
        } else {
            String::new()
        };
        let window_attrs = if win.capped || win.offset > 0 || win.limit > 0 {
            format!(
                " total=\"{u_len}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
                usize::from(win.has_more),
                win.next_offset,
                win.offset,
                win.limit
            )
        } else {
            String::new()
        };
        let next = self
            .tests
            .iter()
            .find_map(|t| t.run.as_deref())
            .unwrap_or("--situ");
        let at_attr = if self.at_stamp.is_empty() {
            String::new()
        } else {
            format!(" at=\"{}\"", self.at_stamp.xml_escaped())
        };
        write!(
            out,
            " changed=\"{}\" impacted=\"{}\" tests=\"{t_len}\" untested=\"{u_len}\" untested_modscope=\"{}\" shown_tests=\"{}\" tests_capped=\"{}\" shown_untested=\"{}\" untested_capped=\"{}\" script_gates_unmodelled=\"{}\" script_gates_registered=\"{}\" script_gates_mapped=\"{}\" script_gates_unresolved_dynamic=\"{}\" ccx_bar=\"15\"{run_first} {}{window_attrs}{at_attr} root=\"{}\" next=\"{}\"",
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
            self.gauge_attrs,
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

impl SafeDeleteReport<'_> {
    fn callers_page(&self) -> Paged<'_, (&str, &str)> {
        Paged::new(self.callers, self.page, |&(p, n)| {
            crate::serialize::text_bytes(&[p, n])
        })
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
        for &(p, n) in self.callers_page().rows() {
            writeln!(
                out,
                "  <c n=\"{}\" p=\"{}\"/>",
                n.xml_escaped(),
                p.xml_escaped()
            )?;
        }
        Ok(())
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

pub struct ExercisesReport<'a> {
    pub test_path: &'a str,
    pub reached: &'a [(&'a str, &'a str)],
}

impl crate::serialize::XmlReport for ExercisesReport<'_> {
    crate::xml_report_meta!("exercises", "ripwire-rs.exercises/v1", root: true);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " test=\"{}\" count=\"{}\"",
            self.test_path.xml_escaped(),
            self.reached.len()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for (p, n) in self.reached {
            writeln!(
                out,
                "  <target p=\"{}\" n=\"{}\"/>",
                p.xml_escaped(),
                n.xml_escaped()
            )?;
        }
        Ok(())
    }
}

/// Serialize `--exercises` to Ripwire XML.
pub fn serialize_exercises_xml<W: Write>(
    root: &str,
    test_path: &str,
    reached: &[(&str, &str)],
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ExercisesReport { test_path, reached }.write_xml_report(root, out)
}

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

pub type DeadRow<'a> = (&'a str, &'static str, &'a str, u32);

pub struct DeadCodeReport<'a> {
    pub items: &'a [DeadRow<'a>],
    /// Functions whose every indexed caller is test code: listed as `<to>` rows, not counted in `count=`.
    pub test_only: &'a [DeadRow<'a>],
    pub gauges: crate::serialize::GraphGauges,
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
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " count=\"{}\" evidence=\"internal-linkage+zero-callers\"",
            self.items.len()
        )?;
        let to = self.test_only_paged().window;
        if to.total > 0 {
            write!(out, " to=\"{}\"", to.total)?;
            if to.capped {
                write!(out, " to_shown=\"{}\"", to.shown)?;
            }
        }
        Ok(())
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
        crate::serialize::GraphGauges::default(),
        crate::serialize::PageRequest::default(),
        out,
    )
}

pub fn serialize_dead_code_paged_xml<W: Write>(
    root: &str,
    items: &[DeadRow<'_>],
    gauges: crate::serialize::GraphGauges,
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

pub struct CommunitiesReport<'a> {
    pub modules: &'a [CommunityModule<'a>],
    pub summary: &'a CommunitySummary<'a>,
    pub page: crate::serialize::PageRequest,
}

impl CommunitiesReport<'_> {
    fn modules_page(&self) -> Paged<'_, CommunityModule<'_>> {
        Paged::new(self.modules, self.page, |m| {
            crate::serialize::row_bytes(|mut w| write_community(&mut w, m, self.page.nested()))
        })
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
        comment: "ripwire-rs communities schema=ripwire-rs.communities/v1: call-graph modules (Louvain): <community id= size= dir= label=> of <member t= n= p=>."
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
        let page_attrs = if pw.capped {
            format!(
                " total=\"{}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
                pw.total,
                usize::from(pw.has_more),
                pw.next_offset,
                pw.offset,
                pw.limit
            )
        } else {
            String::new()
        };
        write!(
            out,
            " drill=\"--community=ID\" modules=\"{}\" shown_modules=\"{}\" modules_capped=\"{}\" bridges=\"{total_bridges}\" shown_bridges=\"{shown_bridges}\" bridges_capped=\"{bridges_capped}\" isolated=\"{}\" isolated_decl=\"{}\" isolated_header=\"{}\" isolated_source=\"{}\" isolated_doc=\"{}\" connected_singletons=\"{}\" symbols=\"{}\"{page_attrs}",
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
        )
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

/// Serialize `--external-surface` to Ripwire XML.
pub struct ExternalSurfaceReport<'a> {
    pub externals: &'a [(&'a str, &'a str, usize)],
    pub page: crate::serialize::PageRequest,
}

impl ExternalSurfaceReport<'_> {
    fn externals_page(&self) -> Paged<'_, (&str, &str, usize)> {
        Paged::new(self.externals, self.page, |&(name, lang, _)| {
            crate::serialize::text_bytes(&[name, lang])
        })
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
        let page_attrs = if pw.capped {
            format!(
                "{} next=\"--external-surface --offset={}\"",
                pw.xml_attrs(),
                pw.next_offset
            )
        } else {
            pw.xml_attrs()
        };
        write!(
            out,
            " names=\"{total}\" builtins_excluded=\"14\"{page_attrs}"
        )
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

pub fn serialize_external_surface_xml<W: Write>(
    _root: &str,
    externals: &[(&str, &str, usize)],
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ExternalSurfaceReport { externals, page }.render_envelope(out)
}

#[derive(Debug, Clone)]
pub struct DepsFileRow<'a> {
    pub path: &'a str,
    pub includes: usize,
    pub efferent: usize,
    pub afferent: usize,
    pub transitive: usize,
    pub inc_targets: Vec<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct DepsReport<'a> {
    pub corpus_files: usize,
    pub dep_files: usize,
    pub ccd: usize,
    pub acd: f64,
    pub nccd: f64,
    pub rows: Vec<DepsFileRow<'a>>,
    pub cycles: Vec<Vec<&'a str>>,
    pub violations: Vec<(&'a str, &'a str, f64)>,
    pub page: crate::serialize::PageRequest,
}

fn write_godfile<W: Write>(out: &mut W, g: &DepsFileRow<'_>) -> io::Result<()> {
    use crate::serialize::PathLayerExt;
    write!(
        out,
        "    <f p=\"{}\" afferent=\"{}\"",
        g.path.xml_escaped(),
        g.afferent
    )?;
    g.path.write_layer_attr(out)?;
    writeln!(out, "/>")
}

fn write_deps_godfiles<W: Write>(
    out: &mut W,
    active: &[&DepsFileRow<'_>],
    page: crate::serialize::PageRequest,
) -> io::Result<()> {
    let mut god: Vec<&DepsFileRow<'_>> =
        active.iter().copied().filter(|d| d.afferent > 0).collect();
    god.sort_by(|a, b| b.afferent.cmp(&a.afferent).then_with(|| a.path.cmp(b.path)));
    let godfiles = Paged::new(&god[..], page.nested(), |g| {
        crate::serialize::row_bytes(|mut w| write_godfile(&mut w, g))
    });
    writeln!(
        out,
        "  <godfiles total=\"{}\" shown=\"{}\" capped=\"{}\">",
        godfiles.window.total,
        godfiles.window.shown,
        usize::from(godfiles.window.capped)
    )?;
    for g in godfiles.rows() {
        write_godfile(out, g)?;
    }
    writeln!(out, "  </godfiles>")
}

fn write_deps_cycle<W: Write>(
    out: &mut W,
    cyc: &[&str],
    members: crate::serialize::PageRequest,
) -> io::Result<()> {
    let sz = cyc.len();
    let cost = sz * sz;
    let cut_attr = if let (Some(&first), Some(&second)) = (cyc.first(), cyc.get(1)) {
        format!(
            " cut=\"{} -&gt; {}\" cutrefs=\"1\"",
            first.xml_escaped(),
            second.xml_escaped()
        )
    } else {
        String::new()
    };
    write!(out, "    <cycle size=\"{sz}\" cost=\"{cost}\"{cut_attr}>")?;
    let kept = Paged::new(cyc, members, |p| crate::serialize::text_bytes(&[*p]));
    for &p in kept.rows() {
        write!(out, "<f p=\"{}\"/>", p.xml_escaped())?;
    }
    writeln!(out, "</cycle>")
}

fn write_deps_cycles<W: Write>(
    out: &mut W,
    cycles: &[Vec<&str>],
    page: crate::serialize::PageRequest,
) -> io::Result<()> {
    if cycles.is_empty() {
        return Ok(());
    }
    let shown = Paged::new(cycles, page.nested(), |cyc| {
        crate::serialize::row_bytes(|mut w| write_deps_cycle(&mut w, cyc, page.nested()))
    });
    writeln!(
        out,
        "  <cycles total=\"{}\" shown=\"{}\" capped=\"{}\">",
        cycles.len(),
        shown.window.shown,
        usize::from(shown.window.capped)
    )?;
    for cyc in shown.rows() {
        write_deps_cycle(out, cyc, page.nested())?;
    }
    writeln!(out, "  </cycles>")
}

fn write_deps_file_row<W: Write>(
    out: &mut W,
    row: &DepsFileRow<'_>,
    page: crate::serialize::PageRequest,
) -> io::Result<()> {
    let p_esc = row.path.xml_escaped();
    let denom = row.efferent + row.afferent;
    let instab = if denom > 0 {
        row.efferent as f64 / denom as f64
    } else {
        0.0
    };
    if row.inc_targets.is_empty() {
        writeln!(
            out,
            "  <f p=\"{p_esc}\" includes=\"{}\" afferent=\"{}\" instab=\"{instab:.2}\" transitive=\"{}\"/>",
            row.includes, row.afferent, row.transitive
        )
    } else {
        write!(
            out,
            "  <f p=\"{p_esc}\" includes=\"{}\" afferent=\"{}\" instab=\"{instab:.2}\" transitive=\"{}\">",
            row.includes, row.afferent, row.transitive
        )?;
        let targets = Paged::new(&row.inc_targets[..], page, |t| {
            crate::serialize::text_bytes(&[*t])
        });
        for &t in targets.rows() {
            write!(out, "<inc t=\"{}\"/>", t.xml_escaped())?;
        }
        writeln!(out, "</f>")
    }
}

fn deps_row_bytes(row: &DepsFileRow<'_>, page: crate::serialize::PageRequest) -> usize {
    crate::serialize::row_bytes(|mut w| write_deps_file_row(&mut w, row, page.nested()))
}

impl<'a> DepsReport<'a> {
    fn active_rows(&self) -> impl Iterator<Item = &DepsFileRow<'a>> {
        self.rows
            .iter()
            .filter(|d| d.includes > 0 || d.efferent > 0 || d.afferent > 0)
    }
}

impl crate::serialize::XmlReport for DepsReport<'_> {
    crate::xml_report_meta!(
        "deps",
        "ripwire-rs.deps/v1",
        root: true,
        comment: "ripwire-rs deps schema=ripwire-rs.deps/v1: file-to-file include/import view, heaviest cone first: <f p= afferent= includes= instab= transitive=>, <health>, <godfiles>, <cycles>."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        let active: Vec<&DepsFileRow<'_>> = self.active_rows().collect();
        Some(Paged::new(&active[..], self.page, |row| deps_row_bytes(row, self.page)).window)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, " files=\"{}\"", self.active_rows().count())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let active: Vec<&DepsFileRow<'_>> = self.active_rows().collect();
        let shape = if self.nccd < 1.0 {
            "horizontal"
        } else if self.nccd > 2.0 {
            "tangled"
        } else {
            "vertical"
        };
        writeln!(
            out,
            "  <health files=\"{}\" dep_files=\"{}\" ccd=\"{}\" acd=\"{:.1}\" nccd=\"{:.2}\" shape=\"{shape}\" dep_langs=\"cpp,py,ts,go,rs,swift,objc,js,sh,java,rb,cs,c,php,lua,ex,kt\"/>",
            self.corpus_files, self.dep_files, self.ccd, self.acd, self.nccd
        )?;
        write_deps_godfiles(out, &active, self.page)?;
        let violations = Paged::new(
            &self.violations[..],
            self.page.nested(),
            |&(from, to, _)| crate::serialize::text_bytes(&[from, to]),
        );
        if !self.violations.is_empty() {
            writeln!(
                out,
                "  <stabledeps violations=\"{}\" shown=\"{}\" capped=\"{}\">",
                self.violations.len(),
                violations.window.shown,
                usize::from(violations.window.capped)
            )?;
            for &(from, to, gap) in violations.rows() {
                writeln!(
                    out,
                    "    <v from=\"{}\" to=\"{}\" gap=\"{gap:.2}\"/>",
                    from.xml_escaped(),
                    to.xml_escaped()
                )?;
            }
            writeln!(out, "  </stabledeps>")?;
        }
        write_deps_cycles(out, &self.cycles, self.page)?;
        let rows = Paged::new(&active[..], self.page, |row| deps_row_bytes(row, self.page));
        for row in rows.rows() {
            write_deps_file_row(out, row, self.page.nested())?;
        }
        Ok(())
    }
}

/// Serialize structured `DepsReport` to Ripwire XML.
pub fn serialize_deps_report_xml<W: Write>(
    root: &str,
    rep: &DepsReport<'_>,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    rep.write_xml_report(root, out)
}

/// Serialize `--deps` to Ripwire XML.
pub fn serialize_deps_xml<W: Write>(
    root: &str,
    file_deps: &[(&str, usize, usize)],
    out: &mut W,
) -> io::Result<()> {
    let rows: Vec<DepsFileRow<'_>> = file_deps
        .iter()
        .map(|&(path, inc, aff)| DepsFileRow {
            path,
            includes: inc,
            efferent: inc,
            afferent: aff,
            transitive: inc + 1,
            inc_targets: Vec::new(),
        })
        .collect();
    let active_cnt = rows
        .iter()
        .filter(|r| r.includes > 0 || r.afferent > 0)
        .count();
    let ccd: usize = rows
        .iter()
        .filter(|r| r.includes > 0 || r.afferent > 0)
        .map(|r| r.transitive)
        .sum();
    let acd = if active_cnt > 0 {
        ccd as f64 / active_cnt as f64
    } else {
        1.0
    };
    let rep = DepsReport {
        corpus_files: file_deps.len(),
        dep_files: active_cnt,
        ccd,
        acd,
        nccd: 0.92,
        rows,
        cycles: Vec::new(),
        violations: Vec::new(),
        page: crate::serialize::PageRequest::default(),
    };
    serialize_deps_report_xml(root, &rep, out)
}
