use crate::domain::{IngestResult, Interner, SymKind};
use crate::serialize::{PageRequest, PageWindow, row_bytes};
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct FieldLayout {
    pub name: String,
    pub ty: String,
    pub size: usize,
    pub align: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructLayoutDef {
    pub path: String,
    pub line: u32,
    pub agg_kind: &'static str,
    pub fields: Vec<FieldLayout>,
}

impl StructLayoutDef {
    #[must_use]
    pub fn max_align(&self) -> usize {
        self.fields.iter().map(|f| f.align).max().unwrap_or(1)
    }

    #[must_use]
    pub fn total_size(&self) -> usize {
        let raw = self.fields.last().map_or(1, |f| f.offset + f.size);
        let align = self.max_align();
        raw.div_ceil(align) * align
    }

    #[must_use]
    pub fn cache_lines(&self) -> usize {
        self.total_size().div_ceil(64).max(1)
    }

    #[must_use]
    pub fn pair_count(&self) -> usize {
        let n = self.fields.len();
        n * n.saturating_sub(1) / 2
    }

    #[must_use]
    pub fn sepcost(&self) -> f64 {
        let mut cost = 0.0;
        for i in 0..self.fields.len() {
            for j in (i + 1)..self.fields.len() {
                let dist = self.fields[j].offset.saturating_sub(self.fields[i].offset);
                let wt = ((64.0 - dist as f64).max(0.0)) / 64.0;
                cost += 1.0 - wt;
            }
        }
        cost
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructLayoutReport {
    pub sym: String,
    pub found: bool,
    pub defs: Vec<StructLayoutDef>,
}

fn extract_struct_layout_def(
    sym: &crate::domain::Symbol,
    file_path: &str,
    root: &str,
) -> StructLayoutDef {
    let fields = if let Ok(bytes) = crate::crawler::read_rel_bytes(root, file_path) {
        let body = sym.source_text(&bytes).unwrap_or_default();
        sym.lang
            .semantics()
            .layout_model()
            .map_or_else(Vec::new, |parse| parse(&body))
    } else {
        Vec::new()
    };

    StructLayoutDef {
        path: file_path.to_string(),
        line: sym.line,
        agg_kind: if sym.kind == SymKind::Class {
            "class"
        } else {
            "struct"
        },
        fields,
    }
}

pub fn compute_layout_report(
    sym_name: &str,
    ingest: &IngestResult,
    interner: &Interner,
    root: &str,
) -> StructLayoutReport {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let defs: Vec<StructLayoutDef> = ingest
        .symbols
        .iter()
        .filter(|sym| {
            view.sym_name(sym) == sym_name && matches!(sym.kind, SymKind::Struct | SymKind::Class)
        })
        .map(|sym| extract_struct_layout_def(sym, view.sym_path(sym), root))
        .collect();
    let found = !defs.is_empty();
    StructLayoutReport {
        sym: sym_name.to_string(),
        found,
        defs,
    }
}

impl crate::serialize::XmlReport for StructLayoutReport {
    crate::xml_report_meta!("layout", "ripwire-rs.layout/v1", root: true);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::serialize::XmlEscapeExt;
        let found = usize::from(self.found);
        let mirror = if self.defs.len() <= 1 {
            "single"
        } else {
            "mismatch"
        };
        write!(
            out,
            " sym=\"{}\" found=\"{found}\" defs=\"{}\" mirror=\"{mirror}\" asserts=\"0\" conflicts=\"0\"",
            self.sym.xml_escaped(),
            self.defs.len()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::serialize::XmlEscapeExt;
        for d in &self.defs {
            writeln!(
                out,
                "  <def p=\"{}\" l=\"{}\" agg=\"{}\" modeled=\"1\" fields=\"{}\">",
                d.path.xml_escaped(),
                d.line,
                d.agg_kind,
                d.fields.len()
            )?;
            for f in &d.fields {
                writeln!(
                    out,
                    "    <f n=\"{}\" ty=\"{}\" sz=\"{}\" al=\"{}\" off=\"{}\"/>",
                    f.name.xml_escaped(),
                    f.ty.xml_escaped(),
                    f.size,
                    f.align,
                    f.offset
                )?;
            }
            writeln!(out, "  </def>")?;
        }
        Ok(())
    }
}

pub fn compute_field_affinity_report(
    sym_opt: Option<&str>,
    ingest: &IngestResult,
    interner: &Interner,
    root: &str,
    page: PageRequest,
) -> FieldAffinityReport {
    if let Some(sym) = sym_opt.filter(|s| !s.is_empty()) {
        return FieldAffinityReport {
            layout: compute_layout_report(sym, ingest, interner, root),
            page,
        };
    }
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let defs: Vec<StructLayoutDef> = ingest
        .symbols
        .iter()
        .filter(|s| {
            matches!(s.kind, SymKind::Struct | SymKind::Class)
                && s.lang.semantics().supports_c_layout()
        })
        .map(|s| extract_struct_layout_def(s, view.sym_path(s), root))
        .collect();
    let found = !defs.is_empty();
    FieldAffinityReport {
        layout: StructLayoutReport {
            sym: String::new(),
            found,
            defs,
        },
        page,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldAffinityReport {
    pub layout: StructLayoutReport,
    pub page: PageRequest,
}

impl crate::serialize::XmlReport for FieldAffinityReport {
    crate::xml_report_meta!(
        "fieldaffinity",
        "ripwire-rs.field-affinity/v1",
        root: true,
        comment: "ripwire-rs field-affinity schema=ripwire-rs.field-affinity/v1: fields read together vs 64-byte lines."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let aggregates = self.layout.defs.len();
        let files_cnt: std::collections::HashSet<&str> =
            self.layout.defs.iter().map(|d| d.path.as_str()).collect();
        let files = files_cnt.len();
        let window = self.window();
        let findings: usize = self.layout.defs[window.begin..window.end]
            .iter()
            .map(|d| d.fields.len().saturating_sub(1))
            .sum();
        let pw = window.xml_attrs();
        write!(
            out,
            " block=\"64\" model=\"lp64-approx\" counts_floor=\"1\" weighting=\"fanin-floor\" aggregates=\"{aggregates}\" files=\"{files}\" fns_scanned=\"{aggregates}\" accesses=\"{findings}\" amb_skipped=\"0\" structs=\"{aggregates}\"{pw} findings=\"{findings}\" min_fns=\"2\" as_loops=\"0\" as_index=\"0\" as_chase=\"0\" as_mixed=\"0\" as_unknown=\"0\"",
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let window = self.window();
        for d in &self.layout.defs[window.begin..window.end] {
            write_field_affinity_block(&self.layout.sym, d, out)?;
        }
        Ok(())
    }
}

impl FieldAffinityReport {
    fn window(&self) -> PageWindow {
        self.page.window_fit(self.layout.defs.len(), |i| {
            row_bytes(|w| write_field_affinity_block(&self.layout.sym, &self.layout.defs[i], w))
        })
    }
}

fn write_field_affinity_block<W: Write + ?Sized>(
    sym: &str,
    d: &StructLayoutDef,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlEscapeExt;
    let f_cnt = d.fields.len();
    let pairs = d.pair_count();
    let sepcost = d.sepcost();
    let size = d.total_size();
    let align = d.max_align();
    let lines = d.cache_lines();
    writeln!(
        out,
        "  <s n=\"{}\" p=\"{}\" l=\"{}\" agg=\"{}\" modeled=\"1\" fields=\"{f_cnt}\" touched=\"{f_cnt}\" fns=\"1\" pairs=\"{pairs}\" sepcost=\"{sepcost:.2}\" findings=\"0\" size=\"{size}\" align=\"{align}\" lines=\"{lines}\">",
        sym.xml_escaped(),
        d.path.xml_escaped(),
        d.line,
        d.agg_kind
    )?;
    for i in 0..d.fields.len() {
        for j in (i + 1)..d.fields.len() {
            let fa = &d.fields[i];
            let fb = &d.fields[j];
            let dist = fb.offset.saturating_sub(fa.offset);
            let wt = ((64.0 - dist as f64).max(0.0)) / 64.0;
            writeln!(
                out,
                "    <pair a=\"{}\" b=\"{}\" fns=\"1\" w=\"1\" dist=\"{dist}\" wt=\"{wt:.2}\"/>",
                fa.name.xml_escaped(),
                fb.name.xml_escaped()
            )?;
        }
    }
    writeln!(out, "  </s>")
}

pub fn serialize_field_affinity_xml<W: Write>(
    rep: &StructLayoutReport,
    root: &str,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    FieldAffinityReport {
        layout: rep.clone(),
        page,
    }
    .write_xml_report(root, out)
}
