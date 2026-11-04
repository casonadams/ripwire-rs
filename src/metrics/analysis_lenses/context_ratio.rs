use crate::domain::{IngestResult, Interner, SymKind};
use crate::graph::Graph;
use crate::serialize::{PageRequest, PageWindow, XmlEscapeExt, row_bytes};
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct ContextRatioRow {
    pub path: String,
    pub line: u32,
    pub name: String,
    pub kind: &'static str,
    pub sites: usize,
    pub ents: usize,
    pub ents_out: usize,
    pub ent_ratio: f64,
    pub files: usize,
    pub files_out: usize,
    pub rtok: usize,
    pub rtok_out: usize,
    pub read_ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextRatioReport {
    pub units: usize,
    pub file_units: usize,
    pub rows: Vec<ContextRatioRow>,
    pub page: PageRequest,
}

fn estimate_symbol_bytes(sym: &crate::domain::Symbol) -> usize {
    (sym.end_byte.saturating_sub(sym.sig_start_byte) as usize)
        .max((sym.loc as usize).saturating_mul(38))
        .max(4)
}

fn build_context_ratio_row(
    sym: &crate::domain::Symbol,
    callees: &[crate::graph::CalleeEdge],
    ingest: &IngestResult,
    interner: &Interner,
) -> ContextRatioRow {
    let sites = callees.len();
    let mut ents_out = 0usize;
    let mut touched_files = std::collections::HashSet::new();
    touched_files.insert(sym.file_id);
    let mut ext_files = std::collections::HashSet::new();
    let mut total_bytes = estimate_symbol_bytes(sym);
    let mut out_bytes = 0usize;

    for edge in callees {
        let callee = &ingest[edge.to];
        let c_bytes = estimate_symbol_bytes(callee);
        total_bytes += c_bytes;
        touched_files.insert(callee.file_id);
        if callee.file_id != sym.file_id {
            ents_out += 1;
            ext_files.insert(callee.file_id);
            out_bytes += c_bytes;
        }
    }

    let ent_ratio = (ents_out as f64) / (sites as f64);
    let rtok = ((total_bytes as f64) / 3.8).round().max(1.0) as usize;
    let rtok_out = ((out_bytes as f64) / 3.8).round() as usize;
    let read_ratio = (rtok_out as f64) / (rtok as f64);
    let file_path = ingest.sym_file_path(sym, interner);
    let name = &interner[sym.name];

    ContextRatioRow {
        path: file_path.to_string(),
        line: sym.line,
        name: name.to_string(),
        kind: sym.kind.tag(),
        sites,
        ents: sites,
        ents_out,
        ent_ratio,
        files: touched_files.len(),
        files_out: ext_files.len(),
        rtok,
        rtok_out,
        read_ratio,
    }
}

pub fn compute_context_ratio(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: PageRequest,
) -> ContextRatioReport {
    let mut rows = Vec::new();
    let mut files_set = std::collections::HashSet::new();
    let mut units = 0usize;

    for sym in &ingest.symbols {
        if sym.kind != SymKind::Function && sym.kind != SymKind::Method {
            continue;
        }
        units += 1;
        files_set.insert(sym.file_id);
        let callees: Vec<_> = graph.callee_edges(sym.id).collect();
        if !callees.is_empty() {
            rows.push(build_context_ratio_row(sym, &callees, ingest, interner));
        }
    }

    rows.sort_by(|a, b| {
        b.ent_ratio
            .partial_cmp(&a.ent_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.sites.cmp(&a.sites))
    });

    ContextRatioReport {
        units,
        file_units: files_set.len(),
        rows,
        page,
    }
}

impl ContextRatioReport {
    fn window(&self) -> PageWindow {
        self.page.window_fit(self.rows.len(), |i| {
            let r = &self.rows[i];
            row_bytes(|w| write_ratio_sym_row(r, w)) + row_bytes(|w| write_ratio_file_row(r, w))
        })
    }
}

fn write_ratio_sym_row<W: Write + ?Sized>(r: &ContextRatioRow, w: &mut W) -> io::Result<()> {
    writeln!(
        w,
        "  <s p=\"{}:{}\" n=\"{}\" t=\"{}\" sites=\"{}\" ents=\"{}\" ents_out=\"{}\" ent_ratio=\"{:.3}\" files=\"{}\" files_out=\"{}\" rtok=\"{}\" rtok_out=\"{}\" read_ratio=\"{:.3}\" ext=\"0\" amb_names=\"0\"/>",
        r.path.xml_escaped(),
        r.line,
        r.name.xml_escaped(),
        r.kind,
        r.sites,
        r.ents,
        r.ents_out,
        r.ent_ratio,
        r.files,
        r.files_out,
        r.rtok,
        r.rtok_out,
        r.read_ratio
    )
}

fn write_ratio_file_row<W: Write + ?Sized>(r: &ContextRatioRow, w: &mut W) -> io::Result<()> {
    writeln!(
        w,
        "  <f p=\"{}\" sites=\"{}\" ents=\"{}\" ents_out=\"{}\" ent_ratio=\"{:.3}\" files=\"{}\" files_out=\"{}\" rtok=\"{}\" rtok_out=\"{}\" read_ratio=\"{:.3}\" ext=\"0\" amb_names=\"0\"/>",
        r.path.xml_escaped(),
        r.sites,
        r.ents,
        r.ents_out,
        r.ent_ratio,
        r.files,
        r.files_out,
        r.rtok,
        r.rtok_out,
        r.read_ratio
    )
}

impl crate::serialize::XmlReport for ContextRatioReport {
    crate::xml_report_meta!(
        "contextratio",
        "ripwire-rs.context-ratio/v1",
        root: true,
        comment: "ripwire-rs context-ratio schema=ripwire-rs.context-ratio/v1: LOCAL-REASONING lens."
    );
    fn write_extra_attrs<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let pw = self.window();
        write!(
            w,
            " units=\"{}\" file_units=\"{}\" defs_per_name_cap=\"8\" body_bytes_per_token=\"3.80\" shown_syms=\"{}\" syms_capped=\"{}\" shown_files=\"{}\" files_capped=\"{}\" total=\"{}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\" defs_capped=\"1\" counts_floor=\"1\"",
            self.units,
            self.file_units,
            pw.shown,
            usize::from(pw.capped),
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
        let pw = self.window();
        let page_rows = &self.rows[pw.begin..pw.end];
        for r in page_rows {
            write_ratio_sym_row(r, w)?;
        }
        for r in page_rows {
            write_ratio_file_row(r, w)?;
        }
        Ok(())
    }
}
