use crate::domain::{Interner, Symbol};
use crate::serialize::escape::{XmlEscapeExt, escape_xml_write};
use std::io::{self, Write};

/// Metadata and callee context for serializing a single symbol row.
pub struct SymbolRowContext<'a> {
    pub sym: &'a Symbol,
    pub sym_name: &'a str,
    pub rank: f64,
    pub amb: u32,
    pub callees: &'a [crate::graph::CalleeEdge],
    pub include_pr: bool,
    pub metrics: bool,
    pub in_degree: usize,
    pub out_degree: usize,
    pub amp: usize,
}

/// Write the file opening element `<f p="...">`.
pub fn write_file_header<W: Write>(w: &mut W, path: &str) -> io::Result<()> {
    use crate::serialize::PathLayerExt;
    path.write_file_open_tag(w)
}

fn write_symbol_metrics_tag<W: Write>(w: &mut W, ctx: &SymbolRowContext<'_>) -> io::Result<()> {
    write!(
        w,
        " in=\"{}\" out=\"{}\" cx=\"{}\" ccx=\"{}\" loc=\"{}\" params=\"{}\" nest=\"{}\" amp=\"{}\"",
        ctx.in_degree,
        ctx.out_degree,
        ctx.sym.cx,
        ctx.sym.ccx,
        ctx.sym.loc,
        ctx.sym.params,
        ctx.sym.max_nest,
        ctx.amp,
    )
}

/// Write a single symbol row and its outgoing call edges.
pub fn write_symbol_row<W: Write>(
    w: &mut W,
    ctx: &SymbolRowContext<'_>,
    interner: &Interner,
    symbols: &[Symbol],
) -> io::Result<()> {
    w.write_all(b"<s t=\"")?;
    w.write_all(ctx.sym.kind.tag().as_bytes())?;
    w.write_all(b"\" n=\"")?;
    escape_xml_write(w, ctx.sym_name)?;
    w.write_all(b"\"")?;

    if let Some(sc_spur) = ctx.sym.scope {
        let sc = interner.resolve(&sc_spur);
        if !sc.is_empty() {
            w.write_all(b" sc=\"")?;
            escape_xml_write(w, sc)?;
            w.write_all(b"\"")?;
        }
    }

    if ctx.include_pr {
        write!(w, " k=\"{:.4}\"", ctx.rank)?;
    }
    if ctx.amb > 0 {
        write!(w, " amb=\"{}\"", ctx.amb)?;
    }
    if ctx.metrics {
        write_symbol_metrics_tag(w, ctx)?;
    }
    w.write_all(b">")?;
    write_callees_tags(w, ctx.callees, interner, symbols)?;
    w.write_all(b"</s>")
}

pub(super) fn write_callees_tags<W: Write>(
    w: &mut W,
    callees: &[crate::graph::CalleeEdge],
    interner: &Interner,
    symbols: &[Symbol],
) -> io::Result<()> {
    let mut by_name: Vec<(&str, crate::resolve::EdgeProv)> = Vec::with_capacity(callees.len());
    for edge in callees {
        let name = interner.resolve(&symbols[edge.to as usize].name);
        match by_name.iter_mut().find(|(n, _)| *n == name) {
            Some((_, prov)) => *prov = (*prov).max(edge.prov),
            None => by_name.push((name, edge.prov)),
        }
    }
    for (name, prov) in by_name {
        write_callee_tag(w, name, prov)?;
    }
    Ok(())
}

fn write_callee_tag<W: Write>(
    w: &mut W,
    name: &str,
    prov: crate::resolve::EdgeProv,
) -> io::Result<()> {
    write!(w, "<c n=\"{}\"", name.xml_escaped())?;
    if let Some(label) = prov.callee_label() {
        write!(w, " prov=\"{label}\"")?;
    }
    w.write_all(b"/>")
}
