use crate::domain::{IngestResult, Interner};
use crate::serialize::{PageWindow, row_bytes};
use std::collections::HashMap;
use std::io::{self, Write};

/// Metrics and worst function details for a single hotspot file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotFile {
    pub file_path: String,
    pub churn: u32,
    pub ccx_sum: u32,
    pub score: u64,
    pub top_fn_name: String,
    pub top_fn_ccx: u32,
    pub top_fn_line: u32,
}

/// Aggregated hotspots report for a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotsReport {
    pub files: Vec<HotspotFile>,
    pub total_files: usize,
    pub ranked_count: usize,
    pub unranked_no_churn: usize,
    pub unranked_no_complexity: usize,
    pub root_path: String,
    pub at_stamp: String,
    pub page: crate::serialize::PageRequest,
}

impl HotspotsReport {
    pub fn with_root(mut self, root: &str) -> Result<Self, String> {
        self.root_path = root.to_string();
        self.at_stamp = crate::git::history::compute_git_at_stamp(root)?.unwrap_or_default();
        Ok(self)
    }

    #[must_use]
    pub fn with_page(mut self, page: crate::serialize::PageRequest) -> Self {
        self.page = page;
        self
    }

    fn window(&self) -> PageWindow {
        self.page.window_fit(self.files.len(), |i| {
            row_bytes(|w| write_hotspot_row(&self.files[i], w))
        })
    }
}

fn write_hotspot_row<W: Write + ?Sized>(f: &HotspotFile, out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "  <f p=\"{}\" churn=\"{}\" ccx=\"{}\" score=\"{}\" top=\"{}\" top_ccx=\"{}\" top_l=\"{}\"/>",
        f.file_path, f.churn, f.ccx_sum, f.score, f.top_fn_name, f.top_fn_ccx, f.top_fn_line
    )
}

impl crate::serialize::XmlReport for HotspotsReport {
    crate::xml_report_meta!(
        "hotspots",
        "ripwire-rs.hotspots/v1",
        root: false,
        comment: "ripwire-rs hotspots schema=ripwire-rs.hotspots/v1: maintenance pain = churn x ccx over window=."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::serialize::XmlEscapeExt;
        let pw = self.window().xml_attrs();
        let at_attr = if self.at_stamp.is_empty() {
            String::new()
        } else {
            format!(" at=\"{}\"", self.at_stamp.xml_escaped())
        };
        write!(
            out,
            " window=\"12mo@HEAD\" files=\"{}\" ranked=\"{}\"{pw} unranked_no_churn=\"{}\" unranked_no_complexity=\"{}\" root=\"{}\"{at_attr}",
            self.total_files,
            self.ranked_count,
            self.unranked_no_churn,
            self.unranked_no_complexity,
            self.root_path.xml_escaped(),
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let window = self.window();
        for f in &self.files[window.begin..window.end] {
            write_hotspot_row(f, out)?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct FileSymbolStats {
    ccx_sum: u32,
    worst_name: String,
    worst_ccx: u32,
    worst_line: u32,
}

fn aggregate_file_symbols(ingest: &IngestResult, interner: &Interner) -> Vec<FileSymbolStats> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let mut stats: Vec<FileSymbolStats> = (0..ingest.files.len())
        .map(|_| FileSymbolStats::default())
        .collect();
    for s in &ingest.symbols {
        if let Some(st) = stats.get_mut(s.file_id as usize) {
            st.ccx_sum = st.ccx_sum.saturating_add(s.ccx);
            if s.ccx > st.worst_ccx || st.worst_name.is_empty() {
                st.worst_ccx = s.ccx;
                st.worst_name = view.sym_name(s).to_string();
                st.worst_line = s.line;
            }
        }
    }
    stats
}

/// Compute maintenance hotspots (churn * ccx) across repository files.
#[must_use]
pub fn compute_hotspots(
    ingest: &IngestResult,
    interner: &Interner,
    file_churn: &HashMap<String, u32>,
) -> HotspotsReport {
    let total_files = ingest.files.len();
    let file_stats = aggregate_file_symbols(ingest, interner);

    let mut ranked = Vec::new();
    let mut unranked_no_churn = 0usize;
    let mut unranked_no_complexity = 0usize;

    for (f, stats) in ingest.files.iter().zip(file_stats) {
        let path = interner.resolve(&f.path);
        let churn = file_churn.get(path).copied().unwrap_or(0);

        if churn == 0 {
            unranked_no_churn += 1;
        } else if stats.ccx_sum == 0 {
            unranked_no_complexity += 1;
        } else {
            let score = (churn as u64) * (stats.ccx_sum as u64);
            ranked.push(HotspotFile {
                file_path: path.to_string(),
                churn,
                ccx_sum: stats.ccx_sum,
                score,
                top_fn_name: stats.worst_name,
                top_fn_ccx: stats.worst_ccx,
                top_fn_line: stats.worst_line,
            });
        }
    }

    ranked.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    let ranked_count = ranked.len();
    HotspotsReport {
        files: ranked,
        total_files,
        ranked_count,
        unranked_no_churn,
        unranked_no_complexity,
        root_path: ".".to_string(),
        at_stamp: String::new(),
        page: crate::serialize::PageRequest::default(),
    }
}
