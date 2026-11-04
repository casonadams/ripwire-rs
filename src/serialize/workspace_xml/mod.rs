use crate::serialize::escape::{XmlEscapeExt, escape_xml_str};
use crate::serialize::{JsonValueExt, Paged};
use serde_json::Value;
use std::io::{self, Write};

mod doc_drift;
pub use doc_drift::{DocDriftReport, serialize_doc_drift_xml};

/// Serialize `whereis` branch symbol search to Ripwire XML.
fn emit_whereis_fate<W: Write>(out: &mut W, fate: &Value, sym: &str) -> io::Result<()> {
    let v = fate.str_or("v", "never");
    write!(out, "  <fate sym=\"{sym}\" v=\"{v}\"")?;
    if let Some(c) = fate.opt_str("commit") {
        write!(out, " commit=\"{c}\"")?;
    }
    if let Some(d) = fate.opt_str("date") {
        write!(out, " date=\"{d}\"")?;
    }
    if let Some(n) = fate.opt_str("note") {
        write!(out, " note=\"{}\"", n.xml_escaped())?;
    }
    writeln!(out, "/>")
}

fn emit_whereis_history<W: Write>(out: &mut W, hist: &Value, sym: &str) -> io::Result<()> {
    let head = hist.get("head").and_then(Value::as_str).unwrap_or("");
    writeln!(out, "  <history probed=\"1\" head=\"{head}\"/>")?;
    if let Some(fate) = hist.get("fate").and_then(Value::as_object) {
        let fate_val = Value::Object(fate.clone());
        emit_whereis_fate(out, &fate_val, sym)?;
    }
    Ok(())
}

fn emit_whereis_hit<W: Write>(out: &mut W, h: &Value, refs: usize) -> io::Result<()> {
    write!(out, "  <hit")?;
    if let Some(r) = h.opt_str("ref") {
        write!(out, " ref=\"{}\"", r.xml_escaped())?;
    }
    for key in ["tip", "date"] {
        if let Some(v) = h.opt_str(key).filter(|v| !v.is_empty()) {
            write!(out, " {key}=\"{}\"", v.xml_escaped())?;
        }
    }
    let path = h.str_any(&["path", "file"], "").xml_escaped();
    let line = h.u64_or("line", 0);
    let kind = h.str_or("kind", "def");
    write!(out, " p=\"{path}\" l=\"{line}\" kind=\"{kind}\"")?;
    if refs > 1 {
        write!(out, " refs=\"{refs}\"")?;
    }
    let text = h.str_or("text", "").xml_escaped();
    writeln!(out, " t=\"{text}\"/>")
}

/// Rows a `--whereis` listing prints: `(hit, refs holding the same line)`, plus the folded count.
struct WhereisRows<'a> {
    listed: Vec<(&'a Value, usize)>,
    folded: usize,
    ref_rows: usize,
}

fn whereis_rows<'a>(hits: &'a [Value], listing: &str) -> WhereisRows<'a> {
    let is_ref = |h: &Value| h.str_or("kind", "ref") == "ref";
    let ref_rows = hits.iter().filter(|h| is_ref(h)).count();
    if listing != "defs" {
        let listed = hits
            .iter()
            .filter(|h| listing == "all" || is_ref(h))
            .map(|h| (h, 1))
            .collect();
        return WhereisRows {
            listed,
            folded: 0,
            ref_rows,
        };
    }
    let mut listed: Vec<(&Value, usize)> = Vec::new();
    let mut first_by_line: std::collections::HashMap<(&str, &str), usize> =
        std::collections::HashMap::new();
    let mut folded = 0;
    for h in hits.iter().filter(|h| !is_ref(h)) {
        if h.bool_or("on_head", false) || h.str_or("ref", "HEAD") == "HEAD" {
            listed.push((h, 1));
            continue;
        }
        let key = (h.str_any(&["path", "file"], ""), h.str_or("text", ""));
        if let Some(&i) = first_by_line.get(&key) {
            listed[i].1 += 1;
            folded += 1;
        } else {
            first_by_line.insert(key, listed.len());
            listed.push((h, 1));
        }
    }
    WhereisRows {
        listed,
        folded,
        ref_rows,
    }
}

/// Serialize `whereis` branch symbol search to Ripwire XML.
pub struct WhereisReport<'a> {
    pub val: &'a Value,
}

impl WhereisReport<'_> {
    fn hits(&self) -> &[Value] {
        self.val
            .array_any(&["hits", "locations"])
            .map_or(&[], Vec::as_slice)
    }

    fn listing(&self) -> &str {
        self.val.str_or("listing", "defs")
    }
}

impl crate::serialize::XmlReport for WhereisReport<'_> {
    crate::xml_report_meta!("whereis", "ripwire-rs.whereis/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let sym = escape_xml_str(self.val.str_or("symbol", ""));
        let hits = self.hits();
        let rows = whereis_rows(hits, self.listing());
        let paged = Paged::new(&rows.listed[..], self.val.page_request(), |&(h, refs)| {
            crate::serialize::row_bytes(|mut w| emit_whereis_hit(&mut w, h, refs))
        });
        let pw = paged.window;
        let folded_attr = if rows.folded > 0 {
            format!(" folded=\"{}\"", rows.folded)
        } else {
            String::new()
        };
        let git = self.val.bool_or("git", false);
        let git_attrs = if git {
            let head_date = self
                .val
                .opt_str("head_date")
                .filter(|d| !d.is_empty())
                .map_or_else(String::new, |d| format!(" head_date=\"{d}\""));
            let at = self
                .val
                .opt_str("at")
                .filter(|s| !s.is_empty())
                .map_or_else(String::new, |s| format!(" at=\"{}\"", s.xml_escaped()));
            format!(
                " on-head=\"{}\" refs_scanned=\"{}\" head_labels=\"index\" ref_labels=\"{}\"{head_date}{at} complete=\"1\"",
                u8::from(self.val.bool_or("on_head", false)),
                self.val.u64_or("refs_scanned", 0),
                self.val.str_or("ref_labels", "parsed"),
            )
        } else {
            String::new()
        };
        let git_attr = if git { "" } else { " git=\"0\"" };
        write!(
            out,
            " sym=\"{sym}\"{git_attr}{git_attrs} blobs=\"{}\" hits=\"{}\"{} listing=\"{}\"{folded_attr} worktree=\"{}\"",
            self.val.u64_or("blobs", 0),
            hits.len(),
            pw.xml_attrs(),
            self.listing().xml_escaped(),
            self.val.str_or("worktree", "read"),
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let sym = escape_xml_str(self.val.str_or("symbol", ""));
        if let Some(hist) = self.val.get("history").and_then(Value::as_object) {
            emit_whereis_history(out, &Value::Object(hist.clone()), &sym)?;
        }
        let rows = whereis_rows(self.hits(), self.listing());
        let paged = Paged::new(&rows.listed[..], self.val.page_request(), |&(h, refs)| {
            crate::serialize::row_bytes(|mut w| emit_whereis_hit(&mut w, h, refs))
        });
        let shown = paged.rows();
        for &(h, refs) in shown {
            emit_whereis_hit(out, h, refs)?;
        }
        if self.listing() == "defs" && rows.ref_rows > 0 {
            writeln!(
                out,
                "  <refs count=\"{}\" next=\"--whereis={sym} --whereis-listing=refs\"/>",
                rows.ref_rows
            )?;
        }
        Ok(())
    }
}

pub fn serialize_whereis_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    WhereisReport { val }.render_envelope(out)
}

/// Serialize `owners` authorship to Ripwire XML.
fn count_uniform_owners(files: &[Value]) -> usize {
    files.iter().filter(|f| f.u64_or("authors", 1) == 1).count()
}

fn write_owner_entry<W: Write>(out: &mut W, f: &Value) -> io::Result<()> {
    let path = f.str_any(&["path", "file"], "").xml_escaped();
    let authors = f.u64_or("authors", 1);
    let bf = f.u64_or("bf", 1);
    let top = f.str_any(&["top_author", "top"], "").xml_escaped();
    let share = f.f64_any(&["top_share", "share"], 1.0);
    writeln!(
        out,
        "  <f p=\"{path}\" authors=\"{authors}\" bf=\"{bf}\" top=\"{top}\" share=\"{share:.2}\"/>"
    )
}

fn collect_owner_display_items(files: &[Value], detail: bool) -> Vec<&Value> {
    files
        .iter()
        .filter(|f| detail || f.u64_or("authors", 1) > 1)
        .collect()
}

/// Serialize `owners` authorship to Ripwire XML.
pub struct OwnersReport<'a> {
    pub val: &'a Value,
}

impl crate::serialize::XmlReport for OwnersReport<'_> {
    crate::xml_report_meta!("owners", "ripwire-rs.owners/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let files = self.val.array_any(&["files", "ownership"]);
        let count = files.map_or(0, Vec::len);
        let root = escape_xml_str(self.val.str_or("root", "."));
        let at_attr = match self.val.opt_str("at") {
            Some(at) if !at.is_empty() => format!(" at=\"{at}\""),
            _ => String::new(),
        };
        let detail = self.val.bool_or("detail", false);
        let display_items = files
            .map(|f| collect_owner_display_items(f, detail))
            .unwrap_or_default();
        let paged = Paged::new(&display_items[..], self.val.page_request(), |f| {
            crate::serialize::row_bytes(|mut w| write_owner_entry(&mut w, f))
        });
        let page_attrs = paged.attrs();
        write!(
            out,
            " files=\"{count}\"{page_attrs} root=\"{root}\"{at_attr}"
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let files = self.val.array_any(&["files", "ownership"]);
        let detail = self.val.bool_or("detail", false);
        let uniform_count = files.map_or(0, |f| count_uniform_owners(f));
        let display_items = files
            .map(|f| collect_owner_display_items(f, detail))
            .unwrap_or_default();
        let paged = Paged::new(&display_items[..], self.val.page_request(), |f| {
            crate::serialize::row_bytes(|mut w| write_owner_entry(&mut w, f))
        });
        let page = paged.rows();
        if !detail && uniform_count > 0 {
            writeln!(
                out,
                "  <uniform authors=\"1\" bf=\"1\" share=\"1.00\" files=\"{uniform_count}\"/>"
            )?;
        }
        for f in page {
            write_owner_entry(out, f)?;
        }
        Ok(())
    }
}

pub fn serialize_owners_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    OwnersReport { val }.render_envelope(out)
}

fn write_flag_gate_attrs<W: Write>(
    out: &mut W,
    g: &Value,
    reads: Option<&Vec<Value>>,
) -> io::Result<()> {
    if let Some(d) = g.opt_str("default").filter(|s| !s.is_empty()) {
        write!(out, " default=\"{}\"", d.xml_escaped())?;
    }
    if let Some(d) = g.get("dark").and_then(Value::as_u64) {
        write!(out, " dark=\"{d}\"")?;
    }
    if let Some(r) = g.get("regions").and_then(Value::as_u64) {
        write!(out, " regions=\"{r}\"")?;
    }
    if let Some(l) = g.get("loc").and_then(Value::as_u64) {
        write!(out, " loc=\"{l}\"")?;
    }
    let reads_count = g
        .get("reads_count")
        .and_then(Value::as_u64)
        .or_else(|| reads.map(|r| r.len() as u64));
    if let Some(c) = reads_count {
        write!(out, " reads=\"{c}\"")?;
    }
    Ok(())
}

fn write_flag_gate_entry<W: Write>(out: &mut W, g: &Value) -> io::Result<()> {
    let name = g.str_any(&["name", "flag"], "").xml_escaped();
    let kind = g.str_or("kind", "compile");
    let file = g.str_any(&["file", "path"], "").xml_escaped();
    let line = g.u64_or("line", 0);
    let reads = g["reads"].as_array();
    write!(out, "  <gate name=\"{name}\" kind=\"{kind}\"")?;
    write_flag_gate_attrs(out, g, reads)?;
    write!(out, " p=\"{file}\" l=\"{line}\"")?;
    match reads.filter(|r| !r.is_empty()) {
        Some(read_list) => {
            writeln!(out, ">")?;
            for r in read_list {
                let rp = r.str_any(&["path", "file"], "").xml_escaped();
                let rl = r.u64_or("line", 1);
                writeln!(out, "    <read p=\"{rp}\" l=\"{rl}\"/>")?;
            }
            writeln!(out, "  </gate>")
        }
        None => writeln!(out, "/>"),
    }
}

pub struct FlagsReport<'a> {
    pub val: &'a Value,
}

impl crate::serialize::XmlReport for FlagsReport<'_> {
    crate::xml_report_meta!("flags", "ripwire-rs.flags/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let empty = Vec::new();
        let items = self.val.array_any(&["gates", "flags"]).unwrap_or(&empty);
        let count = items.len();
        let files = self.val.u64_or("files", 0);
        let compile = self.val.u64_or("compile_count", 0);
        let cmake = self.val.u64_or("cmake_count", 0);
        let env = self.val.u64_or("env_count", 0);
        let dark = self.val.u64_or("dark_gates", 0);
        let paged = Paged::new(&items[..], self.val.page_request(), |g| {
            crate::serialize::row_bytes(|mut w| write_flag_gate_entry(&mut w, g))
        });
        let page_attrs = paged.attrs();
        write!(
            out,
            " files=\"{files}\" gates=\"{count}\" dark_gates=\"{dark}\" compile=\"{compile}\" cmake=\"{cmake}\" env=\"{env}\"{page_attrs}"
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let empty = Vec::new();
        let items = self.val.array_any(&["gates", "flags"]).unwrap_or(&empty);
        let paged = Paged::new(&items[..], self.val.page_request(), |g| {
            crate::serialize::row_bytes(|mut w| write_flag_gate_entry(&mut w, g))
        });
        let page = paged.rows();
        for g in page {
            write_flag_gate_entry(out, g)?;
        }
        Ok(())
    }
}

/// Serialize `flags` environment and compile gates to Ripwire XML.
pub fn serialize_flags_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    FlagsReport { val }.render_envelope(out)
}

/// Serialize `stray_content` branch divergence to Ripwire XML.
pub struct StrayContentReport<'a> {
    pub val: &'a Value,
}

impl crate::serialize::XmlReport for StrayContentReport<'_> {
    crate::xml_report_meta!("stray-content", "ripwire-rs.stray-content/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let head = self.val.str_or("head", "").xml_escaped();
        let head_ref = self.val.str_or("head_ref", "HEAD").xml_escaped();
        let refs = self.val.u64_or("refs", 0);
        let blobs = self.val.u64_or("blobs", 0);
        let unmerged = self.val.u64_or("unmerged", 0);
        let superseded = self.val.u64_or("superseded", 0);
        let merged = self.val.u64_or("merged", 0);
        let unknown = self.val.u64_or("unknown", 0);
        write!(
            out,
            " head=\"{head}\" head_ref=\"{head_ref}\" refs=\"{refs}\" blobs=\"{blobs}\" unmerged=\"{unmerged}\" superseded=\"{superseded}\" merged=\"{merged}\" unknown=\"{unknown}\""
        )?;
        if let Some(at) = self.val.opt_str("at").filter(|s| !s.is_empty()) {
            write!(out, " at=\"{}\"", at.xml_escaped())?;
        }
        Ok(())
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let Some(branches) = self.val["branches"].as_array() else {
            return Ok(());
        };
        for b in branches {
            let name = b.str_any(&["name", "branch"], "").xml_escaped();
            let tip = b.str_or("tip", "").xml_escaped();
            let date = b.str_or("date", "").xml_escaped();
            let base = b.str_or("base", "").xml_escaped();
            let verdict = b.str_or("verdict", "unmerged").xml_escaped();
            let stray = b.u64_or("stray", 0);
            let superseded = b.u64_or("superseded", 0);
            let files = b["files"].as_array().map_or(&[][..], Vec::as_slice);
            writeln!(
                out,
                "  <ref name=\"{name}\" tip=\"{tip}\" date=\"{date}\" base=\"{base}\" ok=\"1\" v=\"{verdict}\" stray=\"{stray}\" files=\"{}\" superseded=\"{superseded}\">",
                files.len()
            )?;
            for f in files.iter().take(STRAY_FILE_ROWS) {
                writeln!(
                    out,
                    "    <file p=\"{}\" v=\"{}\" stray=\"{}\" authored=\"{}\" del=\"{}\" redone=\"{}\" sim=\"{:.2}\" head-touched=\"{}\"/>",
                    f.str_or("p", "").xml_escaped(),
                    f.str_or("verdict", "unmerged").xml_escaped(),
                    f.u64_or("stray", 0),
                    f.u64_or("authored", 0),
                    f.u64_or("del", 0),
                    f.u64_or("redone", 0),
                    f["sim"].as_f64().unwrap_or(0.0),
                    u8::from(f["head_touched"].as_bool() == Some(true)),
                )?;
            }
            if files.len() > STRAY_FILE_ROWS {
                writeln!(
                    out,
                    "    <more files=\"{}\"/>",
                    files.len() - STRAY_FILE_ROWS
                )?;
            }
            writeln!(out, "  </ref>")?;
        }
        Ok(())
    }
}

/// File rows listed per ref in `--stray-content`; the rest are counted by `<more>`.
const STRAY_FILE_ROWS: usize = 12;

pub fn serialize_stray_content_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    StrayContentReport { val }.render_envelope(out)
}

/// Serialize `mentions` doc backtick occurrences to Ripwire XML.
pub struct MentionsReport<'a> {
    pub val: &'a Value,
}

impl crate::serialize::XmlReport for MentionsReport<'_> {
    crate::xml_report_meta!("mentions", "ripwire-rs.mentions/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let sym = self.val.str_or("symbol", "").xml_escaped();
        let count = self.val["docs"].as_array().map_or(0, Vec::len);
        write!(out, " of=\"{sym}\" root=\".\" docs=\"{count}\"")
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        if let Some(items) = self.val["docs"].as_array() {
            for d in items {
                let path = d.str_any(&["path", "file"], "").xml_escaped();
                let mentions = d.u64_or("mentions", 0);
                writeln!(out, "  <doc p=\"{path}\" mentions=\"{mentions}\"/>")?;
            }
        }
        Ok(())
    }
}

pub fn serialize_mentions_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    use crate::serialize::XmlReport;
    MentionsReport { val }.render_envelope(out)
}

/// Serialize `--expand` symbol bodies to Ripwire XML.
pub fn serialize_expand_xml<W: Write>(
    root: &str,
    items: &[(&crate::domain::Symbol, &str, &str, &str)],
    out: &mut W,
) -> io::Result<()> {
    let items: Vec<_> = items
        .iter()
        .map(|(symbol, name, path, body)| BodyItem {
            symbol,
            name,
            path,
            body,
            range: None,
        })
        .collect();
    crate::serialize::XmlReport::render_envelope(
        &BodyReport {
            root,
            items: &items,
        },
        out,
    )
}

pub struct BodyItem<'a> {
    pub symbol: &'a crate::domain::Symbol,
    pub name: &'a str,
    pub path: &'a str,
    pub body: &'a str,
    pub range: Option<(usize, usize, usize)>,
}

pub struct BodyReport<'a> {
    pub root: &'a str,
    pub items: &'a [BodyItem<'a>],
}

impl crate::serialize::XmlReport for BodyReport<'_> {
    fn schema_tag(&self) -> &'static str {
        "ctx"
    }
    fn schema_version(&self) -> &'static str {
        "ripwire-rs.expand/v1"
    }
    fn root_path(&self) -> &str {
        self.root
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        writeln!(
            out,
            "<bodies shown=\"{}\" total=\"{}\">",
            self.items.len(),
            self.items.len()
        )?;
        for item in self.items {
            write!(
                out,
                "  <b t=\"{}\" l=\"{}\" p=\"{}\" n=\"{}\"",
                item.symbol.kind,
                item.symbol.line,
                item.path.xml_escaped(),
                item.name.xml_escaped()
            )?;
            if let Some((total, start, end)) = item.range {
                write!(
                    out,
                    " total_lines=\"{total}\" start_line=\"{start}\" end_line=\"{end}\""
                )?;
            }
            out.write_all(b">")?;
            crate::serialize::write_cdata(out, item.body)?;
            writeln!(out, "</b>")?;
        }
        writeln!(out, "</bodies>")
    }
}

/// Top symbol representation in tree view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFileSymbols<'a> {
    pub path: &'a str,
    pub total_symbols: usize,
    pub top_symbols: Vec<(&'a str, &'a str)>,
}

/// Serialize `--tree` hierarchical file symbol views to Ripwire XML.
pub struct TreeReport<'a> {
    pub root: &'a str,
    pub file_symbols: &'a [TreeFileSymbols<'a>],
    pub files_unlisted: usize,
    pub pr_iters: u32,
    pub page: crate::serialize::PageRequest,
}

impl crate::serialize::XmlReport for TreeReport<'_> {
    fn schema_tag(&self) -> &'static str {
        "tree"
    }
    fn schema_version(&self) -> &'static str {
        "ripwire-rs.tree/v1"
    }
    fn root_path(&self) -> &str {
        self.root
    }
    fn include_root_attr(&self) -> bool {
        false
    }
    fn legend_comment(&self) -> Option<&'static str> {
        Some(
            "ripwire-rs tree schema=ripwire-rs.tree/v1: each file with its top 3 symbols by rank, files by best symbol: <file p= symbols=> of <s t= n=>.",
        )
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let total_files = self.file_symbols.len() + self.files_unlisted;
        let (pw, shown_slice) = self.shown();
        let total_shown: usize = shown_slice.iter().map(|f| f.top_symbols.len()).sum();
        let symbols_capped = usize::from(
            shown_slice
                .iter()
                .any(|f| f.total_symbols > f.top_symbols.len()),
        );
        let page_attrs = pw.xml_attrs();
        let next_attr = if pw.has_more {
            format!(" next=\"--tree --offset={}\"", pw.next_offset)
        } else {
            String::new()
        };
        write!(
            out,
            " files=\"{total_files}\" files_unlisted=\"{}\"{page_attrs} pr_iters=\"{}\" root=\"{}\" shown_symbols=\"{total_shown}\" symbols_capped=\"{symbols_capped}\"{next_attr}",
            self.files_unlisted,
            self.pr_iters,
            self.root.xml_escaped()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let (_, shown_slice) = self.shown();
        for f in shown_slice {
            write_tree_file(out, f)?;
        }
        Ok(())
    }
}

fn write_tree_file<W: Write>(out: &mut W, f: &TreeFileSymbols<'_>) -> io::Result<()> {
    write!(
        out,
        "<file p=\"{}\" symbols=\"{}\">",
        f.path.xml_escaped(),
        f.total_symbols
    )?;
    for &(kind, name) in &f.top_symbols {
        write!(out, "<s t=\"{kind}\" n=\"{}\"/>", name.xml_escaped())?;
    }
    write!(out, "</file>")
}

impl TreeReport<'_> {
    fn shown(&self) -> (crate::serialize::PageWindow, &[TreeFileSymbols<'_>]) {
        let paged = Paged::new(self.file_symbols, self.page, |f| {
            crate::serialize::row_bytes(|mut w| write_tree_file(&mut w, f))
        });
        (paged.window, paged.rows())
    }
}

/// Serialize `--pack-top-n` bodies to Ripwire XML.
pub fn serialize_pack_top_n_xml<W: Write>(
    root: &str,
    bodies: &[(&str, &str)],
    out: &mut W,
) -> io::Result<()> {
    writeln!(
        out,
        "<ctx schema=\"ripwire-rs.pack-top-n/v1\" root=\"{root}\">"
    )?;
    for (path, body) in bodies {
        writeln!(out, "<src p=\"{}\">\n{body}\n</src>", path.xml_escaped())?;
    }
    writeln!(out, "</ctx>")
}
