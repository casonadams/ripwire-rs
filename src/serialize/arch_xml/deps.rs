use crate::serialize::Paged;
use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

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
    /// File pairs linked only by a `mod x;` declaration: containment, kept out of coupling and cycles.
    pub containment_edges: usize,
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
    write!(out, "    <cycle size=\"{sz}\" cost=\"{cost}\"")?;
    if let (Some(&first), Some(&second)) = (cyc.first(), cyc.get(1)) {
        write!(
            out,
            " cut=\"{} -&gt; {}\" cutrefs=\"1\"",
            first.xml_escaped(),
            second.xml_escaped()
        )?;
    }
    write!(out, ">")?;
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
        let containment = if self.containment_edges > 0 {
            format!(" containment_edges=\"{}\"", self.containment_edges)
        } else {
            String::new()
        };
        writeln!(
            out,
            "  <health files=\"{}\" dep_files=\"{}\" ccd=\"{}\" acd=\"{:.1}\" nccd=\"{:.2}\" shape=\"{shape}\"{containment} dep_langs=\"cpp,py,ts,go,rs,swift,objc,js,sh,java,rb,cs,c,php,lua,ex,kt\"/>",
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
        containment_edges: 0,
        page: crate::serialize::PageRequest::default(),
    };
    serialize_deps_report_xml(root, &rep, out)
}
