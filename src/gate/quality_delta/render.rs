use super::acks::StaleAck;
use super::model::{QualityDeltaReport, QualityRegression};
use super::numeric::numeric_bar;
use crate::gate::baseline::DEFAULT_BASELINE_FILE;
use crate::serialize::XmlEscapeExt;
use std::io::{self, Write};

#[must_use]
pub fn stale_by_kind(stale: &[StaleAck]) -> String {
    let mut by_kind: Vec<(&str, usize)> = Vec::new();
    for sa in stale {
        let kind = sa.kind.split(':').next().unwrap_or(&sa.kind);
        match by_kind.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => by_kind.push((kind, 1)),
        }
    }
    by_kind
        .iter()
        .map(|(k, n)| format!("{k}:{n}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The subject and values of a row: `members= tokens=` for a duplication group, else `sym= was= now=`
/// with `bar=` on the numeric kinds.
fn regression_subject(r: &QualityRegression) -> String {
    if r.kind == "duplication" {
        let idiom = r
            .idiom
            .map_or_else(String::new, |i| format!(" idiom=\"{i}\""));
        return format!(
            " members=\"{}\" tokens=\"{}\"{idiom}",
            r.members.join(" | ").xml_escaped(),
            r.now_val
        );
    }
    let sym = if r.members.is_empty() {
        r.sym_name.clone()
    } else {
        r.members.join(" | ")
    };
    let bar = numeric_bar(&r.kind).map_or_else(String::new, |b| format!(" bar=\"{b}\""));
    format!(
        " sym=\"{}\" was=\"{}\" now=\"{}\"{bar}",
        sym.xml_escaped(),
        r.was_val,
        r.now_val
    )
}

/// `gating="1"` with the `--expand` follow-up, `sev="minor"`, and `origin=`, each only when it applies.
fn regression_flags(r: &QualityRegression) -> String {
    let mut flags = String::new();
    if r.is_gating {
        flags.push_str(" gating=\"1\"");
    }
    if r.is_minor {
        flags.push_str(" sev=\"minor\"");
    }
    if r.is_new_symbol {
        flags.push_str(" origin=\"new-symbol\"");
    }
    if let Some(old) = &r.renamed_from {
        flags.push_str(&format!(" renamed_from=\"{}\"", old.xml_escaped()));
    }
    if r.is_gating && r.kind != "duplication" {
        let name = r.sym_name.rsplit("::").next().unwrap_or(&r.sym_name);
        let next = format!("--expand={}:{name}", r.file_path);
        flags.push_str(&format!(" next=\"{}\"", next.xml_escaped()));
    }
    flags
}

fn write_regression_row<W: Write>(r: &QualityRegression, out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "  <r kind=\"{}\"{} p=\"{}:{}\"{}/>",
        r.kind.xml_escaped(),
        regression_subject(r),
        r.file_path.xml_escaped(),
        r.line,
        regression_flags(r)
    )
}

impl QualityDeltaReport {
    fn collapsed_stale_replay(&self) -> Option<&str> {
        let eligible = self.regressions.is_empty() && !self.stale_acks.is_empty();
        self.stale_collapse_replay.as_deref().filter(|_| eligible)
    }

    /// Root attributes emitted only when set: `head_basis=`, `base_ref=` and the non-zero counts.
    fn optional_attrs(&self) -> String {
        let text = [
            ("head_basis", self.head_basis.map(str::to_string)),
            ("base_ref", self.base_ref.clone()),
            (
                "sidecar",
                (self.baseline_source == "sidecar").then(|| DEFAULT_BASELINE_FILE.to_string()),
            ),
            ("baseline_absorbed", nonzero(self.baseline_absorbed)),
            ("renames", nonzero(self.renames)),
            ("acked_by_rename", nonzero(self.acked_by_rename)),
        ];
        text.into_iter()
            .filter_map(|(name, value)| value.map(|v| format!(" {name}=\"{}\"", v.xml_escaped())))
            .collect()
    }

    /// A clean identity delta with nothing to disclose but collapsed stale rows.
    fn is_verdict_only(&self, collapsed: bool) -> bool {
        collapsed
            && self.head_basis == Some("identity")
            && self.acked_count == 0
            && self.baseline_absorbed == 0
            && self.api_new_surface == 0
            && self.renames == 0
            && self.acked_by_rename == 0
    }
}

fn nonzero(n: usize) -> Option<String> {
    (n > 0).then(|| n.to_string())
}

impl crate::serialize::ToXmlString for QualityDeltaReport {
    fn write_xml<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let hb_attr = self.optional_attrs();
        let collapse = self.collapsed_stale_replay();
        let stale_attr = collapse.map_or_else(String::new, |replay| {
            format!(
                " stale_by_kind=\"{}\" stale_next=\"{} --show-stale\"",
                stale_by_kind(&self.stale_acks).xml_escaped(),
                replay.xml_escaped()
            )
        });
        if self.is_verdict_only(collapse.is_some()) {
            return writeln!(
                out,
                "<!-- ripwire-rs quality-delta schema=ripwire-rs.quality-delta/v1: only what the change made WORSE vs baseline=: regressions= minor= gating=; <r kind= sym= p= was= now= bar= gating=1 sev=minor origin=> (clone rows: members= tokens=), <sa> acked; with no regressions the <sa> rows collapse to stale_by_kind= and stale_next= prints them. -->\n<quality-delta schema=\"ripwire-rs.quality-delta/v1\" baseline=\"{}\" regressions=\"0\" gating=\"0\" stale=\"{}\" at=\"{}\"{hb_attr}{stale_attr}></quality-delta>",
                self.baseline_source.xml_escaped(),
                self.stale_acks.len(),
                self.at_stamp.xml_escaped()
            );
        }
        writeln!(
            out,
            "<!-- ripwire-rs quality-delta schema=ripwire-rs.quality-delta/v1: only what the change made WORSE vs baseline=: regressions= minor= gating=; <r kind= sym= p= was= now= bar= gating=1 sev=minor origin=> (clone rows: members= tokens=), <sa> acked; with no regressions the <sa> rows collapse to stale_by_kind= and stale_next= prints them. baseline_absorbed=N: the pinned sidecar was taken on a dirty tree that already held N gating findings, so a green exit means clean since the pin. sidecar=FILE: the pinned file read (baseline=sidecar); without it the baseline is git-HEAD. -->\n<quality-delta schema=\"ripwire-rs.quality-delta/v1\" baseline=\"{}\" regressions=\"{}\" minor=\"{}\" acked=\"{}\" stale=\"{}\" preexisting-worse=\"{}\" new-symbol=\"{}\" gating=\"{}\" api-new-surface=\"{}\" at=\"{}\"{hb_attr}{stale_attr}>",
            self.baseline_source.xml_escaped(),
            self.regressions.len(),
            self.minor_count,
            self.acked_count,
            self.stale_acks.len(),
            self.preexisting_worse_count,
            self.new_symbol_count,
            self.gating_count,
            self.api_new_surface,
            self.at_stamp.xml_escaped()
        )?;
        for r in &self.regressions {
            write_regression_row(r, out)?;
        }
        if collapse.is_none() {
            for sa in &self.stale_acks {
                writeln!(
                    out,
                    "  <sa kind=\"{}\" key=\"{}\" why=\"{}\" sym=\"{}\" p=\"{}\"/>",
                    sa.kind.xml_escaped(),
                    sa.key.xml_escaped(),
                    sa.why.xml_escaped(),
                    sa.sym.xml_escaped(),
                    sa.path.xml_escaped()
                )?;
            }
        }
        writeln!(out, "</quality-delta>")
    }
}

pub fn stale_replay_for(scope: Option<&str>) -> String {
    match scope {
        Some(s) => format!("--quality-delta --scope={s}"),
        None => "--quality-delta".to_string(),
    }
}
