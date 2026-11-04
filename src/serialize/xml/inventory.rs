use super::rows::write_callees_tags;
use super::stream::StreamXmlContext;
use crate::domain::Symbol;
use crate::serialize::escape::XmlEscapeExt;
use crate::serialize::{PageRequest, Paged, row_bytes};
use std::io::{self, Write};

const INVENTORY_NAME_CEILING: usize = 2000;

fn is_inv_code_file(lang: crate::domain::Lang, path: &str) -> bool {
    use crate::serialize::PathLayerExt;
    path.is_inventory_code_file(lang)
}

fn inv_entry_symbols<'a>(ctx: &StreamXmlContext<'a>) -> Vec<&'a Symbol> {
    use crate::domain::CorpusView;
    ctx.ingest
        .symbols
        .iter()
        .filter(|sym| {
            let name = ctx.sym_name(sym);
            let scope = ctx.sym_scope(sym);
            let path = ctx.sym_path(sym);
            sym.lang
                .semantics()
                .is_entrypoint_symbol(name, scope, sym.kind, path)
        })
        .collect()
}

fn write_inv_entry<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    sym: &Symbol,
) -> io::Result<()> {
    use crate::domain::CorpusView;
    let name = ctx.sym_name(sym);
    let path = ctx.sym_path(sym);
    write!(
        w,
        "<entry p=\"{}\" n=\"{}\">",
        path.xml_escaped(),
        name.xml_escaped()
    )?;
    let callees: Vec<crate::graph::CalleeEdge> = ctx.graph.callee_edges(sym.id).collect();
    write_callees_tags(w, &callees, ctx.interner, &ctx.ingest.symbols)?;
    w.write_all(b"</entry>")
}

fn write_inv_entries<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    page: PageRequest,
) -> io::Result<()> {
    let entries = inv_entry_symbols(ctx);
    crate::serialize::write_paged_rows(w, &entries, page, |mut w, sym| {
        write_inv_entry(&mut w, ctx, sym)
    })
}

fn collect_inv_dir_map<'a>(
    ctx: &StreamXmlContext<'a>,
    listed_files: &[bool],
) -> (
    std::collections::BTreeMap<&'a str, (usize, Vec<&'a str>)>,
    usize,
) {
    let mut dir_map: std::collections::BTreeMap<&'a str, (usize, Vec<&'a str>)> =
        std::collections::BTreeMap::new();
    let mut names_total = 0usize;
    for (idx, f) in ctx.ingest.files.iter().enumerate() {
        if listed_files.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let p = ctx.interner.resolve(&f.path);
        let (dir, fname) = p.rsplit_once('/').unwrap_or((".", p));
        let entry = dir_map.entry(dir).or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        if is_inv_code_file(f.lang, p) {
            names_total += 1;
            if entry.1.len() < 8 {
                entry.1.push(fname);
            }
        }
    }
    (dir_map, names_total)
}

fn write_inv_dir<W: Write>(
    w: &mut W,
    dir: &str,
    cnt: usize,
    code_files: &[&str],
) -> io::Result<()> {
    if code_files.is_empty() {
        write!(w, "<ls p=\"{}\" n=\"{cnt}\"/>", dir.xml_escaped())
    } else {
        let f_list = code_files.join(",");
        write!(
            w,
            "<ls p=\"{}\" n=\"{cnt}\" f=\"{}\"/>",
            dir.xml_escaped(),
            f_list.xml_escaped()
        )
    }
}

pub(super) fn write_inv_section<W: Write>(
    w: &mut W,
    ctx: &StreamXmlContext<'_>,
    listed_files: &[bool],
    page: PageRequest,
) -> io::Result<()> {
    let listed = listed_files.iter().filter(|&&b| b).count();
    let unlisted = ctx.ingest.files.len().saturating_sub(listed);
    if unlisted == 0 {
        return Ok(());
    }
    let (dir_map, names_total) = collect_inv_dir_map(ctx, listed_files);
    if names_total > INVENTORY_NAME_CEILING {
        write!(
            w,
            "<inv listed=\"{listed}\" unlisted=\"{unlisted}\" names_capped=\"1\" names_total=\"{names_total}\" names_next=\"--tree --limit={}\">",
            ctx.ingest.files.len()
        )?;
    } else {
        write!(w, "<inv listed=\"{listed}\" unlisted=\"{unlisted}\">")?;
    }
    write_inv_entries(w, ctx, page)?;
    let dirs: Vec<_> = dir_map.into_iter().collect();
    let shown = Paged::new(&dirs, page, |(dir, (cnt, files))| {
        row_bytes(|mut w| write_inv_dir(&mut w, dir, *cnt, files))
    });
    for (dir, (cnt, files)) in shown.rows() {
        write_inv_dir(w, dir, *cnt, files)?;
    }
    w.write_all(b"</inv>")
}
