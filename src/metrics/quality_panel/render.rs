use super::families::{PANEL_FAMILIES, family_attrs};
use super::history::history_root_attrs;
use super::{QualityPanelReport, QualityPanelRow};
use crate::serialize::{PageWindow, row_bytes};
use std::io::{self, Write};

impl QualityPanelReport {
    fn window(&self) -> PageWindow {
        self.page.window_fit(self.rows.len(), |i| {
            row_bytes(|w| write_quality_row(&self.rows[i], self.unavail_row_attr(), w))
        })
    }

    fn unavail_row_attr(&self) -> &'static str {
        if self.cfiles == 0 {
            " unavail=\"confusion\""
        } else {
            ""
        }
    }
}

fn write_quality_row<W: Write + ?Sized>(
    r: &QualityPanelRow,
    unavail_row_attr: &str,
    w: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlEscapeExt;
    writeln!(
        w,
        "  <s p=\"{}:{}\" n=\"{}\" fam=\"{}\" of=\"{}\" fired=\"{}\"{unavail_row_attr}>",
        r.path.xml_escaped(),
        r.line,
        r.name.xml_escaped(),
        r.fam,
        r.of,
        r.fired
    )?;
    r.evidence
        .iter()
        .try_for_each(|e| writeln!(w, "    <e f=\"{}\"{}/>", e.family, e.attrs))?;
    writeln!(w, "  </s>")
}

impl crate::serialize::XmlReport for QualityPanelReport {
    crate::xml_report_meta!(
        "quality_panel",
        "ripwire-rs.quality-panel/v1",
        root: true,
        comment: "ripwire-rs quality-panel schema=ripwire-rs.quality-panel/v1: every quality family in ONE report. state = MUTABLE non-local state only (reassignable module/class bindings, and constants the indexed files mutate); reads of unmutated configuration constants are NOT state (state_lens=mutable). historical = git commits per file in the window at or above hcut (p90 of changed files, clamped to 3..14; hbasis says which)."
    );
    fn write_extra_attrs<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let pw = self.window();
        let ccut = if self.cfiles > 0 { 40 } else { 0 };
        let history_attrs = history_root_attrs(&self.history, self.hranked);
        let family = family_attrs(PANEL_FAMILIES.len(), &self.enabled, &self.unavailable);
        write!(
            w,
            "{} preset=\"{}\" cut=\"{}\" cut_reachable=\"1\" eligible=\"{}\" ranked=\"{}\" below_cut=\"{}\" no_family=\"{}\" bar_ccx=\"15\" bar_loc=\"60\" bar_nest=\"4\" bar_params=\"5\" rcut=\"40\" rmeasured=\"{}\" hranked=\"{}\"{history_attrs} cfiles=\"{}\" cscope=\"{}\" cranked=\"{}\" ccut=\"{ccut}\" lscope=\"{}\" sfiles=\"{}\" sscope=\"{}\" state_lens=\"mutable\" cells=\"{}\" tested_scope=\"{}\" deep_untested=\"{}\" shown=\"{}\" capped=\"{}\" total=\"{}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
            family,
            self.preset,
            self.cut,
            self.eligible,
            self.ranked,
            self.below_cut,
            self.no_family,
            self.eligible,
            self.hranked,
            self.cfiles,
            self.cscope,
            self.cranked,
            self.eligible,
            self.sfiles,
            self.sscope,
            self.cells,
            self.tested_scope,
            self.deep_untested,
            pw.shown,
            usize::from(pw.capped),
            pw.total,
            usize::from(pw.has_more),
            pw.next_offset,
            pw.offset,
            pw.limit
        )
    }
    fn write_body<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let unavail_row_attr = self.unavail_row_attr();
        let pw = self.window();
        for r in &self.rows[pw.begin..pw.end] {
            write_quality_row(r, unavail_row_attr, w)?;
        }
        Ok(())
    }
}
